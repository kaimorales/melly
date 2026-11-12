#!/usr/bin/env python3
"""Private App Server trace and explicit high-effort benchmark override.

No model messages, prompts, tools or results are rewritten. Only reasoning effort
is pinned so Melly's production low-effort worker default cannot skew comparison.
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
            record("recv", json.loads(line))
        except ValueError:
            record("recv-invalid", line.decode(errors="replace"))
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
    sys.exit(process.wait())
