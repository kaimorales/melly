#!/usr/bin/env python3
"""Exercise manual/automatic workers and independent drafts through a real PTY."""
import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time
from terminal_screen import Screen

screens = {}

binary = Path(sys.argv[1]).resolve(strict=True)
peer = Path(__file__).with_name("fake_agents.py").resolve(strict=True)


def expect(fd, needle, timeout=10):
    output = bytearray()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not select.select([fd], [], [], max(0, deadline-time.monotonic()))[0]:
            break
        try:
            chunk = os.read(fd, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        output.extend(chunk)
        screen = screens.setdefault(fd, Screen())
        screen.feed(chunk)
        if needle in (output if b"\x1b" in needle else screen.text()):
            return
    raise AssertionError(f"Missing {needle!r}: {bytes(output[-1800:])!r}")


with tempfile.TemporaryDirectory(prefix="melly-team-pty-", dir="/tmp") as directory:
    log = Path(directory, "rpc.jsonl")
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(directory)
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
        os.environ.update(TERM="xterm-256color", MELLY_CODEX_BIN=str(peer), MELLY_TEST_RPC_LOG=str(log))
        os.execv(binary, [str(binary)])
    try:
        expect(fd, b"Ready")
        os.write(fd, b"/spawn Inspect c.txt\r")
        expect(fd, b"luna-1")
        os.write(fd, b"keep this draft\x1b1Delegate two checks\r")
        expect(fd, b"TEAM_COMPLETE")
        os.write(fd, b"\x1b2\r")
        expect(fd, b"WORKER_FOLLOWUP")
        os.write(fd, b"/agents\r")
        expect(fd, b"Ctrl+D stop")
        os.write(fd, b"\x1b\x11")
        expect(fd, b"\x1b[?1049l")
        _, status = os.waitpid(pid, 0)
        pid = None
        assert os.waitstatus_to_exitcode(status) == 0
        messages = [json.loads(line) for line in log.read_text().splitlines()]
        starts = [m for m in messages if m.get("method") == "thread/start"]
        assert len(starts) == 4, len(starts)
        denied = next(m for m in messages if m.get("id") == "auto-denied")
        assert denied["result"]["success"] is False
        turns = [m["params"] for m in messages if m.get("method") == "turn/start"]
        followup = next(t for t in turns if t["input"][0]["text"] == "keep this draft")
        assert followup["threadId"] == "worker-1", followup
        assert len(turns) == 5, len(turns)
    finally:
        os.close(fd)
        if pid is not None:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
print("Team terminal smoke passed (2 capped automatic + 1 manual Luna, routed drafts).")
