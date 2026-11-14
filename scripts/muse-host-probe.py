#!/usr/bin/env python3
"""Probe one pinned Muse host with synthetic data, behind a Mac sandbox.

The mock Responses frames follow Meta's MIT-licensed SDK quickstart fixture:
https://github.com/meta-models/muse-code-sdk/blob/main/clients/sdk-quickstart/src/provider.ts
See docs/licenses/meta-muse-sdk-MIT.txt. This is a research tool, not an adapter.
"""

import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import signal
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
import uuid

PINNED_HOSTS = {
    "3f6da0d865ceb97dfe3002372ff1551b6a187dccc740866456a546510b1412e7": (
        "1.4.18",
        "sha256:77dda18550adef14e257b015fe4e6490b722723f7d526df5ba4867939fff68a3",
    ),
    "89964f68f7f020896fc75679dc2f3975e5e15a6efe8537d91512a1b45ffb37aa": (
        "1.4.20",
        "sha256:283b1b125dcc9172928fb5d6711a92a353796d030a7c186988ed527a13cf07f0",
    ),
}
FAKE_KEY = "melly-synthetic-only-no-account"
MODEL = "melly-fake-model"
ATTACKS = (
    (
        "bash",
        "MELLY_FORCE_BASH_PROBE",
        {"command": "/usr/bin/true", "description": "Synthetic blocked action"},
    ),
    ("read_file", "MELLY_FORCE_READ_PROBE", {"path": "synthetic-only.txt"}),
    (
        "write_file",
        "MELLY_FORCE_WRITE_PROBE",
        {"path": "synthetic-write.txt", "content": "synthetic"},
    ),
    ("web_search", "MELLY_FORCE_WEB_PROBE", {"query": "synthetic"}),
)


def record(test, **details):
    print(json.dumps({"test": test, **details}), flush=True)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def uuid7():
    timestamp = int(time.time() * 1000) & ((1 << 48) - 1)
    random_bits = uuid.uuid4().int & ((1 << 76) - 1)
    random_bits = (random_bits & ~(3 << 62)) | (2 << 62)
    return str(uuid.UUID(int=(timestamp << 80) | (7 << 76) | random_bits))


def profile(root, executable, port):
    # The test host can read system files and its disposable root, but not the
    # user's home or mounted volumes. It can write only inside the test root,
    # execute only itself, and use the selected probe's network policy.
    network = (
        f'(allow network-outbound (remote ip "localhost:{port}"))'
        if port is not None
        else '(allow network-outbound (remote tcp "*:443"))\n'
        '(deny network-outbound (remote ip "localhost:*"))'
    )
    return f"""(version 1)
      (allow default)
      (deny file-read* file-write* (subpath "/Users"))
      (deny file-read* file-write* (subpath "/Volumes"))
      (deny file-read* file-write* (subpath "/private/var/folders"))
      (deny file-read* file-write* (subpath "/private/tmp"))
      (allow file-read* (subpath "{root}"))
      (deny file-write*)
      (allow file-write* (subpath "{root}"))
      (allow file-write* (literal "/dev/null"))
      (deny process-exec)
      (allow process-exec (literal "{executable}"))
      (deny network-outbound)
      (deny network-inbound)
      {network}"""


def verify_sandbox(root, port):
    sentinel = Path(__file__).resolve()
    require(str(sentinel).startswith("/Users/"), "Sandbox self-check needs this script in /Users.")
    require(sentinel.is_file(), "Sandbox self-check cannot read its sentinel.")
    policy = profile(root, "/bin/bash", port)

    def check(script, *args):
        try:
            return subprocess.run(
                ["/usr/bin/sandbox-exec", "-p", policy, "/bin/bash", "-c", script, "--", *args],
                env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"},
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
                timeout=3,
                check=False,
            )
        except subprocess.TimeoutExpired as error:
            raise RuntimeError("Sandbox self-check timed out.") from error

    require(check(":").returncode == 0, "Could not start the sandbox self-check.")
    require(
        check('IFS= read -r line < "$1"', str(sentinel)).returncode != 0,
        "Sandbox did not block home-directory reads.",
    )
    with tempfile.NamedTemporaryFile(
        mode="w", dir="/private/tmp", prefix="melly-sandbox-sentinel-"
    ) as outside:
        outside.write("synthetic sentinel\n")
        outside.flush()
        require(
            check('IFS= read -r line < "$1"', outside.name).returncode != 0,
            "Sandbox did not block outside temporary files.",
        )
    require(check("/usr/bin/true").returncode != 0, "Sandbox did not block child executables.")
    if port is not None:
        require(
            check('exec 3<>/dev/tcp/127.0.0.1/"$1"', str(port)).returncode == 0,
            "Sandbox did not allow the fake endpoint.",
        )
    denied = check(": >/dev/tcp/192.0.2.1/9")
    require(
        denied.returncode != 0 and "Operation not permitted" in denied.stderr,
        "Sandbox did not deny outside network access with EPERM.",
    )
    record("sandbox", home_reads="blocked", child_exec="blocked", non_https_egress="blocked")


def sse(value):
    return ("data: " + json.dumps(value, separators=(",", ":")) + "\n\n").encode()


def response_frame(identity, status, **extra):
    return {
        "id": identity,
        "object": "response",
        "model": MODEL,
        "status": status,
        "output": [],
        **extra,
    }


class FixtureServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True

    def __init__(self):
        super().__init__(("127.0.0.1", 0), FixtureHandler)
        self.requests = []
        self.bad_credential = False
        self.attack_target = None
        self.attacks_sent = set()
        self.lock = threading.Lock()
        self.pending_started = threading.Event()
        self.release_pending = threading.Event()

    def handle_error(self, *_):
        # Cancellation intentionally resets the pending fixture connection.
        pass


class FixtureHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_GET(self):
        if not self.path.endswith("/muse-code/models"):
            self.send_error(404)
            return
        body = json.dumps(
            {
                "object": "list",
                "data": [
                    {
                        "id": MODEL,
                        "object": "model",
                        "metadata": {
                            "muse-code": {
                                "release_date": "2026-01-01",
                                "is_hidden": False,
                                "limit": {"context": 1000000, "output": 1024},
                            }
                        },
                    }
                ],
            }
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        fixture = self.server
        if not self.path.endswith("/responses"):
            self.send_error(404)
            return
        # Never print or retain an authorization header; reject anything other
        # than the dummy value generated exclusively for this fixture.
        if self.headers.get("Authorization") != "Bearer " + FAKE_KEY:
            fixture.bad_credential = True
            self.send_error(403)
            return
        size = int(self.headers.get("Content-Length", "0"))
        if size > 4_000_000:
            self.send_error(413)
            return
        raw = self.rfile.read(size)
        try:
            payload = json.loads(raw)
        except json.JSONDecodeError:
            self.send_error(400)
            return
        pending = b"MELLY_CANCEL_PROBE" in raw
        with fixture.lock:
            target = fixture.attack_target
            attack = bool(
                target and target[1].encode() in raw and target[0] not in fixture.attacks_sent
            )
            if attack:
                fixture.attacks_sent.add(target[0])
            fixture.requests.append(
                {
                    "tools": payload.get("tools") or [],
                    "attack": target[0] if attack else None,
                }
            )
        identity = "resp_probe_" + target[0] if attack else "resp_probe_text"
        head = sse(
            {
                "type": "response.created",
                "sequence_number": 1,
                "response": response_frame(identity, "in_progress"),
            }
        )
        if attack:
            head += sse(
                {
                    "type": "response.function_call_arguments.done",
                    "sequence_number": 2,
                    "output_index": 0,
                    "item_id": "fc_" + target[0],
                    "name": target[0],
                    "call_id": "call_" + target[0],
                    "arguments": json.dumps(target[2]),
                }
            )
        else:
            head += sse(
                {
                    "type": "response.output_text.delta",
                    "sequence_number": 2,
                    "output_index": 0,
                    "item_id": "msg_synthetic",
                    "content_index": 0,
                    "delta": "synthetic reply",
                }
            )
        tail = sse(
            {
                "type": "response.completed",
                "sequence_number": 3,
                "response": response_frame(
                    identity,
                    "completed",
                    usage={"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
                ),
            }
        )
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header(
            "Transfer-Encoding" if pending else "Content-Length",
            "chunked" if pending else str(len(head + tail)),
        )
        self.end_headers()
        try:
            if pending:
                self.wfile.write(f"{len(head):x}\r\n".encode() + head + b"\r\n")
                self.wfile.flush()
                fixture.pending_started.set()
                fixture.release_pending.wait(timeout=10)
                self.wfile.write(f"{len(tail):x}\r\n".encode() + tail + b"\r\n0\r\n\r\n")
            else:
                self.wfile.write(head + tail)
        except (BrokenPipeError, ConnectionResetError):
            pass


class Host:
    def __init__(self, root, binary, fixture, name, expected):
        self.expected = expected
        self.synthetic = fixture is not None
        self.root = root / name
        for part in ("config/muse", "data", "cache", "tmp", "workspace"):
            (self.root / part).mkdir(parents=True, mode=0o700)
        config = self.root / "config/muse"
        env = {
            "PATH": "/usr/bin:/bin",
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_DATA_HOME": str(self.root / "data"),
            "XDG_CACHE_HOME": str(self.root / "cache"),
            "TMPDIR": str(self.root / "tmp") + "/",
            "TBH_DISABLE_TELEMETRY": "1",
        }
        port = None
        model_args = []
        if fixture is not None:
            port = fixture.server_address[1]
            (config / "settings.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "provider": "meta",
                        "model": MODEL,
                        "endpoint_transport": {
                            "base_url": f"http://127.0.0.1:{port}",
                            "auth": "bearer",
                        },
                    }
                )
            )
            (config / "auth.json").write_text(
                json.dumps({"schema_version": 1, "providers": {"meta": {"api_key": FAKE_KEY}}})
            )
            os.chmod(config / "settings.json", 0o600)
            os.chmod(config / "auth.json", 0o600)
            env["TBH_CREDENTIAL_BACKEND"] = "file"
            model_args = ["--model", MODEL]
        args = [
            "/usr/bin/sandbox-exec",
            "-p",
            profile(root, binary, port),
            str(binary),
            "serve",
            "--provider",
            "meta",
            *model_args,
            "--preset",
            "empty",
            "--no-session-log",
            "--disable-shell",
            "--disable-write",
            "--sandbox-network",
            "restricted",
            "--contributor-mode",
            "off",
            "--disable-request-user-input",
        ]
        self.process = subprocess.Popen(
            args,
            cwd=self.root / "workspace",
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
            start_new_session=True,
        )
        self.lines = queue.Queue()
        self.events = []
        threading.Thread(
            target=self._read, args=(self.process.stdout, self.lines.put), daemon=True
        ).start()

    @staticmethod
    def _read(stream, sink):
        for line in stream:
            sink(line)

    def send(self, message):
        self.process.stdin.write(json.dumps(message, separators=(",", ":")) + "\n")
        self.process.stdin.flush()

    def wait_for(self, predicate, timeout=20):
        for message in self.events:
            if predicate(message):
                return message
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = self.lines.get(timeout=min(0.25, max(0.001, deadline - time.monotonic())))
            except queue.Empty:
                if self.process.poll() is not None:
                    raise RuntimeError(f"Host exited with code {self.process.returncode}.")
                continue
            message = json.loads(line)
            method = message.get("method", "")
            if not self.synthetic:
                require(
                    not method.startswith(("session/", "turn/", "item/"))
                    and not (method and "id" in message),
                    "Unexpected session, tool event, or host request.",
                )
            self.events.append(message)
            if predicate(message):
                return message
        raise RuntimeError("Timed out waiting for the host.")

    def rpc(self, identity, method, params):
        self.send({"jsonrpc": "2.0", "id": identity, "method": method, "params": params})
        reply = self.wait_for(lambda msg: msg.get("id") == identity)
        if "error" in reply:
            raise RuntimeError(f"{method} failed with code {reply['error'].get('code')}.")
        return reply["result"]

    def initialize(self):
        init = self.rpc(
            1,
            "initialize",
            {
                "clientInfo": {"name": "melly_muse_probe", "version": "1.0"},
                "capabilities": {"experimentalApi": True, "userInputDialogs": False},
            },
        )
        require(
            init.get("serverInfo", {}).get("version") == self.expected[0],
            "Unexpected host version.",
        )
        require(
            init.get("schema", {}).get("fingerprint") == self.expected[1], "Unexpected host schema."
        )
        require(
            init.get("sessionDurability") == "ephemeral", "Host did not use ephemeral sessions."
        )
        home_path = init.get("museHome")
        require(isinstance(home_path, str), "Host did not report its data root.")
        try:
            Path(home_path).resolve().relative_to(self.root)
        except ValueError as error:
            raise RuntimeError("Host data root is not isolated.") from error
        self.send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        if self.synthetic:
            require(
                self.rpc(2, "account/read", {}).get("state") == "apiKey",
                "Host did not use the dummy API-key lane.",
            )

    def start_session(self):
        result = self.rpc(
            3,
            "session/start",
            {
                "commandId": uuid7(),
                "workspaceRoot": str(self.root / "workspace"),
                "providerId": "meta",
                "modelId": MODEL,
                "approvalMode": "onRequest",
            },
        )
        return result["session"]["sessionId"]

    def close(self):
        try:
            self.process.stdin.close()
        except BrokenPipeError:
            pass
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGTERM)
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait(timeout=3)
        require(
            self.process.returncode == 0, f"Host did not exit cleanly ({self.process.returncode})."
        )


def session_probe(host, fixture):
    host.send(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "session/start",
            "params": {
                "commandId": uuid7(),
                "workspaceRoot": str(host.root / "workspace"),
                "providerId": "meta",
                "modelId": MODEL,
                "config": {"allowedBuiltInTools": []},
            },
        }
    )
    restriction = host.wait_for(lambda msg: msg.get("id") == 12)
    if host.expected[0] == "1.4.20":
        require(
            "error" in restriction,
            "Experimental tool restriction was unexpectedly admitted; inspect its enforcement before trusting it.",
        )
    record(
        "experimental_tool_restriction",
        host_version=host.expected[0],
        admitted="error" not in restriction,
        error_code=restriction.get("error", {}).get("code"),
    )
    session = host.start_session()
    (host.root / "workspace/synthetic-only.txt").write_text("synthetic fixture\n")

    def turn(identity, text):
        ack = host.rpc(
            identity,
            "turn/start",
            {"commandId": uuid7(), "sessionId": session, "input": [{"type": "text", "text": text}]},
        )
        result = host.wait_for(
            lambda msg: msg.get("method") == "turn/completed"
            and msg.get("params", {}).get("turnId") == ack.get("turnId"),
            25,
        )
        require(result["params"].get("terminal") == "completed", "Synthetic session turn failed.")

    turn(4, "MELLY_NORMAL_PROBE")
    for index, target in enumerate(ATTACKS):
        with fixture.lock:
            fixture.attack_target = target
        turn(5 + index, target[1])
    tool_items = [
        msg.get("params", {}).get("item", {})
        for msg in host.events
        if msg.get("method") == "item/completed"
    ]
    for name, _, _ in ATTACKS:
        rejected = any(
            item.get("kind") == "toolCall"
            and item.get("tool") == name
            and item.get("status") == "rejected"
            and "unknown tool" in item.get("failureReason", "").lower()
            for item in tool_items
        )
        require(
            name in fixture.attacks_sent and rejected,
            f"The synthetic {name} call was not rejected as unknown.",
        )
        require(
            any(entry["attack"] == name and not entry["tools"] for entry in fixture.requests),
            f"The main request advertised tools for {name}.",
        )
    require(not fixture.requests[0]["tools"], "The first main request advertised unexpected tools.")
    require(
        not (host.root / "workspace/synthetic-write.txt").exists(),
        "The synthetic write unexpectedly occurred.",
    )
    main_requests = [entry for entry in fixture.requests if not entry["tools"]]
    require(len(main_requests) >= 2, "Expected tool-free main requests were not observed.")
    agents = sorted(
        {
            msg.get("params", {}).get("item", {}).get("reminderAgentId")
            for msg in host.events
            if msg.get("params", {}).get("item", {}).get("reminderAgentId")
        }
    )
    record(
        "session",
        main_requests_without_tools=len(main_requests),
        forced_actions="rejected as unknown",
        reminder_agents=agents,
        synthetic_model_requests=len(fixture.requests),
    )


def completion_probe(host, fixture):
    command = uuid7()
    host.rpc(
        4,
        "model/complete",
        {
            "commandId": command,
            "messages": [
                {"role": "user", "text": "synthetic first turn"},
                {"role": "assistant", "text": "synthetic earlier reply"},
                {"role": "user", "text": "synthetic follow-up"},
            ],
            "maxTokens": 256,
        },
    )
    done = host.wait_for(
        lambda msg: msg.get("method") == "model/completed"
        and msg.get("params", {}).get("commandId") == command,
        25,
    )["params"]
    require(
        done.get("outcome") == "completed" and done.get("text") == "synthetic reply",
        "Stateless mock completion failed.",
    )
    require(
        len(fixture.requests) == 1 and not fixture.requests[0]["tools"],
        "Stateless completion used unexpected requests or tools.",
    )
    record("model_complete", outcome="completed", model_requests=1, tools=0)
    cancel = uuid7()
    host.rpc(
        5,
        "model/complete",
        {
            "commandId": cancel,
            "messages": [{"role": "user", "text": "MELLY_CANCEL_PROBE"}],
            "maxTokens": 256,
        },
    )
    require(
        fixture.pending_started.wait(timeout=10),
        "Pending completion did not reach the fake provider.",
    )
    start = time.monotonic()
    host.rpc(6, "model/cancel", {"commandId": cancel})
    done = host.wait_for(
        lambda msg: msg.get("method") == "model/completed"
        and msg.get("params", {}).get("commandId") == cancel,
        10,
    )["params"]
    require(done.get("outcome") == "cancelled", "Stateless cancellation did not settle cancelled.")
    record("model_cancel", outcome="cancelled", elapsed_ms=round((time.monotonic() - start) * 1000))

    for index, target in enumerate(ATTACKS, start=10):
        fixture.attack_target = target
        command = uuid7()
        before_requests = len(fixture.requests)
        before_events = len(host.events)
        host.rpc(
            index,
            "model/complete",
            {
                "commandId": command,
                "messages": [{"role": "user", "text": target[1]}],
                "maxTokens": 32,
            },
        )
        host.wait_for(
            lambda msg: msg.get("method") == "model/completed"
            and msg.get("params", {}).get("commandId") == command
        )
        require(
            fixture.requests[before_requests:] == [{"tools": [], "attack": target[0]}],
            "Stateless completion advertised tools or made extra model requests.",
        )
        require(
            not any(
                event.get("method", "").startswith(("session/", "turn/", "item/"))
                or ("method" in event and "id" in event)
                for event in host.events[before_events:]
            ),
            "Stateless completion produced session or tool events.",
        )
    require(
        not (host.root / "workspace/synthetic-write.txt").exists(),
        "Stateless completion wrote a synthetic file.",
    )
    record("model_completion_forced_actions", executed=0, tested=len(ATTACKS))


def stop_probe(host, fixture):
    session = host.start_session()
    ack = host.rpc(
        4,
        "turn/start",
        {
            "commandId": uuid7(),
            "sessionId": session,
            "input": [{"type": "text", "text": "MELLY_CANCEL_PROBE"}],
        },
    )
    require(
        fixture.pending_started.wait(timeout=10), "Pending session did not reach the fake provider."
    )
    start = time.monotonic()
    host.rpc(
        5, "turn/interrupt", {"commandId": uuid7(), "sessionId": session, "turnId": ack["turnId"]}
    )
    done = host.wait_for(
        lambda msg: msg.get("method") == "turn/completed"
        and msg.get("params", {}).get("turnId") == ack["turnId"],
        10,
    )["params"]
    require(done.get("terminal") == "cancelled", "Session Stop did not settle cancelled.")
    record(
        "turn_interrupt", outcome="cancelled", elapsed_ms=round((time.monotonic() - start) * 1000)
    )


def subscription_probe(host, command_id, *, infer):
    def check_account(identity):
        account = host.rpc(identity, "account/read", {})
        require(
            account.get("state") != "loggedOut",
            "The confined host could not access an existing Muse login. "
            "Check 'muse usage' to distinguish a missing login from a sandbox handoff failure.",
        )
        require(
            account.get("state") == "accountLogin" and account.get("credentialRequired") is True,
            "Muse host did not confirm browser sign-in. API-key and unknown routes are blocked.",
        )

    check_account(2)
    plan = host.rpc(3, "usage/refresh", {})
    require(plan.get("subscriptionActive") is True, "Host did not confirm an active Muse Code plan.")
    record("subscription", active=True, usage_meter=isinstance(plan.get("usage"), dict))
    if not infer:
        return False

    check_account(4)
    ack = host.rpc(
        5,
        "model/complete",
        {
            "commandId": command_id,
            "messages": [{"role": "user", "text": "Reply with exactly MELLY_PROBE_OK"}],
            "maxTokens": 32,
        },
    )
    require(
        ack.get("status") == "accepted" and ack.get("commandId") == command_id,
        "Host did not acknowledge the test completion.",
    )
    done = host.wait_for(
        lambda msg: msg.get("method") == "model/completed"
        and msg.get("params", {}).get("commandId") == command_id,
        50,
    )["params"]
    require(done.get("outcome") == "completed", "The stateless Muse completion failed.")
    require(done.get("text", "").strip() == "MELLY_PROBE_OK", "Unexpected test reply.")
    check_account(6)

    # A small request may not change whole-percentage meter readings. Treat
    # a fresh host observation as evidence, without claiming a billing receipt.
    before = plan.get("usage") or {}
    after = host.rpc(7, "usage/read", {}).get("usage") or {}
    before_ms = before.get("observedAtMs", 0)
    after_ms = after.get("observedAtMs", 0)
    record(
        "subscription_completion",
        outcome="completed",
        post_request_usage_observed=after_ms > before_ms,
    )
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--muse", required=True, type=Path, help="Path to a pinned official Muse host"
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--plan-status", action="store_true", help="Read plan status from Meta")
    mode.add_argument("--plan-test", action="store_true", help="Send one synthetic prompt on an active plan")
    args = parser.parse_args()
    require(sys.platform == "darwin", "This probe requires macOS sandbox-exec.")
    require(args.muse.is_file(), "Host is missing.")
    original_hash = sha256(args.muse)
    expected = PINNED_HOSTS.get(original_hash)
    require(expected is not None, "Host does not match a pinned official SHA-256.")
    if args.plan_status or args.plan_test:
        require(expected[0] == "1.4.20", "Plan checks require the pinned 1.4.20 host.")
    with tempfile.TemporaryDirectory(prefix="melly-muse-probe-", dir="/private/tmp") as temporary:
        root = Path(temporary).resolve()
        binary = root / "muse"
        # Copy first so the sandbox never needs read access to the user's home.
        shutil.copyfile(args.muse, binary)
        os.chmod(binary, 0o700)
        require(sha256(binary) == original_hash, "Copied host checksum mismatch.")
        if args.plan_status or args.plan_test:
            verify_sandbox(root, None)
            host = Host(root, binary, None, "plan", expected)
            try:
                host.initialize()
                subscription_probe(host, uuid7(), infer=args.plan_test)
            finally:
                host.close()
            return
        for index, (name, test) in enumerate(
            (("session", session_probe), ("complete", completion_probe), ("stop", stop_probe))
        ):
            fixture = FixtureServer()
            threading.Thread(target=fixture.serve_forever, daemon=True).start()
            host = None
            try:
                if index == 0:
                    verify_sandbox(root, fixture.server_address[1])
                host = Host(root, binary, fixture, name, expected)
                host.initialize()
                test(host, fixture)
                require(not fixture.bad_credential, "Fixture rejected an unexpected credential.")
            finally:
                fixture.release_pending.set()
                if host is not None:
                    host.close()
                fixture.shutdown()
                fixture.server_close()
    record(
        "result",
        status="passed",
        host_version=expected[0],
        scope="synthetic only; no subscription verified",
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as error:
        print(f"Muse probe failed: {error}", file=sys.stderr)
        sys.exit(1)
