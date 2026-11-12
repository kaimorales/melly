#!/usr/bin/env python3
"""Run real signed-in Codex vs Melly workflows. This consumes plan allowance.

Both use Sol high + Luna high, identical incremental tasks and isolated git repos.
Raw traces stay in --output, outside this source checkout. No account defaults change.
"""
import argparse
from datetime import datetime
import errno
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import termios
import time

import fixture

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tests"))
from terminal_screen import Screen

def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")

def read_jsonl(path):
    if not path.exists(): return []
    rows = []
    with path.open() as source:
        for line in source:
            try: rows.append(json.loads(line))
            except ValueError: pass
    return rows

def session_logs(project):
    # Read only metadata from recent local sessions; retain only this exact workspace.
    root = Path.home() / ".codex" / "sessions"
    result = []
    for directory in sorted(root.glob("*/*/*"))[-3:]:
        for path in directory.glob("*.jsonl"):
            try:
                with path.open() as f: row = json.loads(f.readline())
                meta = row.get("payload", {})
                if Path(meta.get("cwd", "/")).resolve() == project.resolve():
                    result.append((path, meta))
            except (OSError, ValueError): pass
    return result

def snapshot(project, primary_id):
    """Sum per-response records exactly once, excluding inherited fork history."""
    totals = {key: 0 for key in ("input_tokens", "cached_input_tokens", "output_tokens", "reasoning_output_tokens", "total_tokens")}
    calls, threads, seen = [], [], set()
    for path, meta in session_logs(project):
        thread_id = meta["id"]
        own = []
        model, effort = None, None
        turns = []
        commands = []
        for row in read_jsonl(path):
            payload = row.get("payload", {})
            if row["type"] == "turn_context":
                model, effort = payload.get("model"), payload.get("effort")
            if row["type"] == "event_msg" and payload.get("type") in ("task_started", "task_complete"):
                turns.append({"at": row["timestamp"], **payload})
            if row["type"] == "response_item" and payload.get("type") in ("function_call", "custom_tool_call"):
                commands.append({"at": row["timestamp"], "name": payload.get("name"), "arguments": payload.get("arguments", payload.get("input", ""))})
            if row["type"] != "token_usage_record": continue
            if payload.get("thread_id") != thread_id: continue
            key = (thread_id, payload.get("response_id"))
            if key in seen: continue
            seen.add(key)
            usage = payload["usage"]
            call = {"at": row["timestamp"], "thread_id": thread_id, "turn_id": payload.get("turn_id"),
                    "model": model, "effort": effort, "role": "conductor" if thread_id == primary_id else "worker", "usage": usage}
            own.append(call)
            calls.append(call)
            for metric in totals: totals[metric] += usage.get(metric, 0)
        threads.append({"id": thread_id, "role": "conductor" if thread_id == primary_id else "worker",
                        "source": meta.get("source"), "path": str(path), "calls": len(own), "turns": turns, "commands": commands})
    totals["uncached_input_tokens"] = totals["input_tokens"] - totals["cached_input_tokens"]
    return {"totals": totals, "calls": calls, "threads": threads}

def stock_turn(args, project, out, prompt, thread_id):
    # Preserve the user's stock conductor workflow, replacing only its role name
    # with a project-scoped high-effort Luna definition and a shared turn cap.
    guidance = (Path.home() / ".codex" / "conductor.config.toml").read_text()
    import tomllib
    guidance = tomllib.loads(guidance)["developer_instructions"].replace("luna_implementer", "bench_luna")
    guidance += "\nUse at most two worker assignments/revisions per user turn. Never edit files yourself."
    config = {
        "model_reasoning_effort": "high", "service_tier": "default", "sandbox_mode": "workspace-write",
        "approval_policy": "never", "memories.use_memories": False, "memories.generate_memories": False,
        "skills.max_context_tokens": 3000, "agents.default_subagent_model": "gpt-6-luna",
        "agents.default_subagent_reasoning_effort": "high", "agents.max_concurrent_threads_per_session": 3,
        "developer_instructions": guidance,
        f'projects.{json.dumps(str(project.resolve()))}.trust_level': "trusted",
    }
    cmd = [str(args.codex), "-p", "conductor", "-m", "gpt-6.1-sol"]
    for key, value in config.items(): cmd += ["-c", key + "=" + json.dumps(value)]
    cmd += ["exec"]
    if thread_id: cmd += ["resume", thread_id]
    cmd += ["--json", prompt]
    start = time.monotonic()
    write_json(out / "invocation.json", {"argv": cmd, "started": datetime.now().astimezone().isoformat()})
    with (out / "stdout.jsonl").open("w") as stdout, (out / "stderr.txt").open("w") as stderr:
        process = subprocess.Popen(cmd, cwd=project, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True)
        try: code = process.wait(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            code = -1
    events = read_jsonl(out / "stdout.jsonl")
    thread_id = next((e["thread_id"] for e in events if e.get("type") == "thread.started"), thread_id)
    return thread_id, {"wall_seconds": time.monotonic() - start, "exit_code": code,
                       "completed": any(e.get("type") == "turn.completed" for e in events),
                       "final": next((e["item"]["text"] for e in reversed(events) if e.get("item", {}).get("type") == "agent_message"), "")}

class Melly:
    def __init__(self, args, project, out, resume=False):
        self.args, self.project, self.out = args, project, out
        self.rpc = out / "rpc.jsonl"
        self.state = out.parent / "melly-state"
        self.screen = Screen()
        self.trace = (out / "terminal.bin").open("wb")
        self.offset, self.remainder, self.events = 0, b"", []
        self.start = time.monotonic()
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(project)
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 130, 0, 0))
            os.environ.update(TERM="xterm-256color", MELLY_CODEX_BIN=str(Path(__file__).with_name("rpc_tap.py").resolve()),
                MELLY_BENCH_CODEX=str(args.codex), MELLY_BENCH_RPC_LOG=str(self.rpc), MELLY_DATA_DIR=str(self.state))
            os.execv(args.melly, [str(args.melly), *( ["--continue"] if resume else ["--model", "gpt-6.1-sol"] )])
        self.primary = None
        self.wait(lambda: self.find_primary() and b"Ready" in self.screen.text(), 45)
        self.startup_seconds = time.monotonic() - self.start
        os.write(self.fd, b"/effort high\r")
        self.pump(0.15)

    def find_primary(self):
        methods = {e["message"].get("id"): e["message"].get("method") for e in self.events if e["direction"] == "send"}
        for e in self.events:
            m = e["message"]
            if e["direction"] == "recv" and methods.get(m.get("id")) in ("thread/start", "thread/resume"):
                self.primary = m.get("result", {}).get("thread", {}).get("id")
                if self.primary: return True
        return False

    def pump(self, seconds=0.1):
        if select.select([self.fd], [], [], seconds)[0]:
            try: data = os.read(self.fd, 262144)
            except OSError as e:
                if e.errno != errno.EIO: raise
                data = b""
            if not data: raise RuntimeError("Melly terminal closed unexpectedly")
            self.trace.write(data)
            self.screen.feed(data)
        if self.rpc.exists():
            with self.rpc.open("rb") as log:
                log.seek(self.offset)
                data = log.read()
                self.offset += len(data)
            chunks = (self.remainder + data).split(b"\n")
            self.remainder = chunks.pop()
            self.events.extend(json.loads(x) for x in chunks if x)

    def wait(self, condition, timeout):
        deadline = time.monotonic() + timeout
        while not condition():
            if time.monotonic() >= deadline: raise TimeoutError("Melly benchmark wait expired")
            self.pump()

    def turn(self, prompt):
        begin = len(self.events)
        start = time.monotonic()
        # Bracketed paste avoids interpreting prompt punctuation as terminal controls.
        os.write(self.fd, b"\x1b[200~" + prompt.encode() + b"\x1b[201~\r")
        def finished():
            return any(e["direction"] == "recv" and e["message"].get("method") == "turn/completed"
                and e["message"].get("params", {}).get("threadId") == self.primary for e in self.events[begin:])
        try:
            self.wait(finished, self.args.timeout)
            completion = next(e["message"]["params"]["turn"] for e in reversed(self.events[begin:])
                if e["direction"] == "recv" and e["message"].get("method") == "turn/completed"
                and e["message"].get("params", {}).get("threadId") == self.primary)
            complete = completion.get("status") == "completed"
        except TimeoutError:
            os.write(self.fd, b"\x03")
            complete = False
            completion = {"status": "benchmark_timeout"}
        duration = time.monotonic() - start
        self.pump(0.1)
        finals = [e["message"]["params"]["item"].get("text", "") for e in self.events[begin:]
            if e["direction"] == "recv" and e["message"].get("method") == "item/completed"
            and e["message"].get("params", {}).get("threadId") == self.primary
            and e["message"]["params"].get("item", {}).get("type") == "agentMessage"]
        return {"wall_seconds": duration, "completed": complete, "completion_status": completion.get("status"),
                "exit_code": 0 if complete else -1, "final": finals[-1] if finals else ""}

    def close(self):
        if self.pid is None: return
        os.write(self.fd, b"\x11")
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            try: self.pump(0.05)
            except RuntimeError: break
            done, _ = os.waitpid(self.pid, os.WNOHANG)
            if done:
                self.pid = None
                break
        if self.pid is not None:
            done, _ = os.waitpid(self.pid, os.WNOHANG)
            if not done:
                os.kill(self.pid, signal.SIGTERM)
                os.waitpid(self.pid, 0)
            self.pid = None
        os.close(self.fd)
        self.trace.close()

def run_session(args, arm, repeat):
    output = args.output / f"repeat-{repeat}" / arm
    output.mkdir(parents=True)
    project = output / "project"
    baseline = fixture.create(project)
    primary, terminal, rows = None, None, []
    try:
        for stage, prompt in enumerate(fixture.PROMPTS, 1):
            step = output / f"stage-{stage}"
            step.mkdir()
            fixture.add_contract(project, stage)
            (step / "prompt.txt").write_text(prompt)
            print(json.dumps({"event": "start", "arm": arm, "repeat": repeat, "stage": stage}), flush=True)
            before = snapshot(project, primary)
            if arm == "stock":
                primary, result = stock_turn(args, project, step, prompt, primary)
            else:
                # Reopen before stage three, exercising restored team/context.
                if stage == 3:
                    terminal.close()
                    terminal = None
                if terminal is None:
                    terminal = Melly(args, project, step, resume=stage == 3)
                primary = terminal.primary
                result = terminal.turn(prompt)
                result["startup_seconds"] = terminal.startup_seconds if stage in (1, 3) else 0
            # Give the local usage recorder a bounded chance to flush.
            time.sleep(0.3)
            after = snapshot(project, primary)
            write_json(step / "usage.json", after)
            result["usage_delta"] = {k: after["totals"][k] - before["totals"][k] for k in after["totals"]}
            seen = {(c["thread_id"], c["at"]) for c in before["calls"]}
            result["calls"] = [c for c in after["calls"] if (c["thread_id"], c["at"]) not in seen]
            result["settings_valid"] = bool(result["calls"]) and all(c["effort"] == "high" and c["model"] == ("gpt-6.1-sol" if c["role"] == "conductor" else "gpt-6-luna") for c in result["calls"])
            result["worker_calls"] = sum(c["role"] == "worker" for c in result["calls"])
            result["conductor_calls"] = sum(c["role"] == "conductor" for c in result["calls"])
            conductor = next((t for t in after["threads"] if t["id"] == primary), {})
            complete = [t for t in conductor.get("turns", []) if t["type"] == "task_complete"]
            started = [t for t in conductor.get("turns", []) if t["type"] == "task_started"]
            end = next((t for t in reversed(complete) if started and t.get("turn_id") == started[-1].get("turn_id")), None)
            if started and end:
                result["turn_seconds"] = (datetime.fromisoformat(end["at"].replace("Z", "+00:00")) - datetime.fromisoformat(started[-1]["at"].replace("Z", "+00:00"))).total_seconds()
            result["grade"] = fixture.grade(project, stage)
            diff = subprocess.run(["git", "diff", "--stat"], cwd=project, capture_output=True, text=True, check=True).stdout
            (step / "diff-stat.txt").write_text(diff)
            result.update(arm=arm, repeat=repeat, stage=stage, primary_id=primary, baseline_sha256=baseline)
            write_json(step / "result.json", result)
            rows.append(result)
            write_json(output / "results.json", rows)
            print(json.dumps({"event": "done", **{k:result[k] for k in ("arm", "repeat", "stage", "wall_seconds", "settings_valid", "worker_calls", "conductor_calls", "usage_delta")}, "passed":result["grade"]["passed"]}), flush=True)
            if not result["completed"] or not result["settings_valid"] or result["worker_calls"] == 0:
                raise RuntimeError("Invalid or incomplete workflow; stopping before spending on more runs")
    finally:
        if terminal: terminal.close()
    return rows

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--codex", type=Path, default=Path.home()/".local/bin/codex")
    parser.add_argument("--melly", type=Path, default=Path.home()/".local/bin/melly")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--arm", choices=["stock", "melly"])
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {"created": datetime.now().astimezone().isoformat(), "repeats": args.repeats,
        "models": {"conductor": "gpt-6.1-sol", "worker": "gpt-6-luna", "effort": "high"},
        "codex": subprocess.check_output([args.codex,"--version"],text=True).strip(),
        "melly": subprocess.check_output([args.melly,"--version"],text=True).strip(),
        "timeout_seconds": args.timeout, "protocol": "stock exec/resume vs production Melly TUI with logged high-effort override",
        "task_count_per_session": 3, "order": "stock,melly; melly,stock; stock,melly",
        "memory": "disabled equally for reproducibility", "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "source_files_sha256": {name:hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
            for name in ("compare.py", "fixture.py", "rpc_tap.py", "holdout.py")}}
    write_json(args.output / "manifest.json", manifest)
    rows = []
    for repeat in range(1, args.repeats + 1):
        arms = [args.arm] if args.arm else (["stock", "melly"] if repeat % 2 else ["melly", "stock"])
        for arm in arms:
            rows.extend(run_session(args, arm, repeat))
            write_json(args.output / "results.json", rows)
    print(json.dumps({"event": "complete", "stages": len(rows)}), flush=True)

if __name__ == "__main__": main()
