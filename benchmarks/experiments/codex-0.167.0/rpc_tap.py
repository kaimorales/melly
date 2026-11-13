#!/usr/bin/env python3
"""PRIVATE EXPERIMENT: fork the live conductor for fresh Melly workers.

Retains the benchmark's High/High settings. Rewrites worker thread/start into
thread/fork, with the benchmark's existing workspace-write/never permissions.
This is not a production permission default. Original worker role, prompt,
tool results and source packet are preserved; every override is logged.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

log_path = Path(os.environ["MELLY_BENCH_RPC_LOG"])
lock = threading.Lock()
log_file = log_path.open("a", buffering=1)
lead_requests = set()
lead_thread = None

def record(direction, message, overrides=None):
    with lock:
        log_file.write(json.dumps({"at": time.monotonic(), "direction": direction,
            "message": message, "overrides": overrides}) + "\n")

flags = ["-c", 'model_reasoning_effort="high"', "-c", 'sandbox_mode="workspace-write"',
         "-c", 'approval_policy="never"', "-c", "memories.use_memories=false",
         "-c", "memories.generate_memories=false", "-c", 'service_tier="default"']
with log_path.with_suffix(".stderr.txt").open("a") as err:
    process = subprocess.Popen([os.environ["MELLY_BENCH_CODEX"], *flags, *sys.argv[1:]],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err)

    def forward():
        try:
            for line in sys.stdin.buffer:
                message = json.loads(line)
                overrides = None
                if message.get("method") == "turn/start":
                    params = message["params"]
                    overrides = {"effort": {"from": params.get("effort"), "to": "high"}}
                    params["effort"] = "high"
                elif message.get("method") in ("thread/start", "thread/resume"):
                    message["params"].setdefault("config", {})["model_reasoning_effort"] = "high"
                    params = message["params"]
                    guidance = params.get("developerInstructions", "")
                    if guidance.startswith("You are Melly's conductor"):
                        with lock:
                            lead_requests.add(message["id"])
                    elif message["method"] == "thread/start" and guidance.startswith("You are a Melly "):
                        with lock:
                            parent_id = lead_thread
                        if not parent_id:
                            raise RuntimeError("Fork experiment has no identified conductor")
                        allowed = {"cwd", "model", "config", "developerInstructions", "sandbox", "approvalPolicy"}
                        if set(params) - allowed:
                            raise RuntimeError("Unexpected worker-start keys; refusing implicit conversion")
                        overrides = {"experiment": "live_conductor_fork", "original_method": "thread/start", "parent": parent_id}
                        message["method"] = "thread/fork"
                        params["threadId"] = parent_id
                        params["excludeTurns"] = True
                        params.setdefault("sandbox", "workspace-write")
                        params.setdefault("approvalPolicy", "never")
                record("send", message, overrides)
                process.stdin.write((json.dumps(message) + "\n").encode())
                process.stdin.flush()
        except (BrokenPipeError, OSError):
            pass
        finally:
            process.stdin.close()

    threading.Thread(target=forward, daemon=True).start()
    for line in process.stdout:
        try:
            message = json.loads(line)
            with lock:
                if message.get("id") in lead_requests:
                    candidate = message.get("result", {}).get("thread", {}).get("id")
                    if candidate:
                        lead_thread = candidate
                    lead_requests.discard(message.get("id"))
            record("recv", message)
        except ValueError:
            record("recv-invalid", line.decode(errors="replace"))
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
    sys.exit(process.wait())
