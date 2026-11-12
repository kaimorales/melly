#!/usr/bin/env python3
"""Exercise conductor-only input and workspace restoration through a real PTY."""
from contextlib import contextmanager
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
    if b"\x1b" not in needle and fd in screens and needle in screens[fd].text():
        return
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


def dismiss(fd):
    os.write(fd, b"\x1b")
    time.sleep(0.1)


@contextmanager
def terminal(directory, log, *args, hangup=False):
    pid, fd = pty.fork()
    if pid == 0:
        os.chdir(directory)
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
        os.environ.update(
            TERM="xterm-256color",
            MELLY_CODEX_BIN=str(peer),
            MELLY_TEST_RPC_LOG=str(log),
            MELLY_DATA_DIR=str(Path(directory, "state")),
        )
        os.execv(binary, [str(binary), *args])
    try:
        yield fd
        if hangup:
            os.kill(pid, signal.SIGHUP)
        else:
            os.write(fd, b"\x11")
        expect(fd, b"\x1b[?1049l")
        _, status = os.waitpid(pid, 0)
        pid = None
        assert os.waitstatus_to_exitcode(status) == 0
    finally:
        screens.pop(fd, None)
        os.close(fd)
        if pid is not None:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)


with tempfile.TemporaryDirectory(prefix="melly-team-pty-", dir="/tmp") as directory:
    log = Path(directory, "rpc.jsonl")
    with terminal(directory, log, hangup=True) as fd:
        expect(fd, b"Ready")
        os.write(fd, b"Delegate two checks\r")
        expect(fd, b"TEAM_COMPLETE")
        os.write(fd, b"\x1b2")
        expect(fd, b"Observing Luna")
        os.write(fd, b"\x12")  # Ctrl+R: inspect the selected worker's exact handoff.
        expect(fd, b"Instructions from conductor")
        os.write(fd, b"\t")
        expect(fd, b"Accepted by conductor")
        expect(fd, b"git diff --stat")
        os.write(fd, b"\x0e")  # Next worker has no fabricated verdict.
        expect(fd, b"No verdict recorded")
        dismiss(fd)
        expect(fd, b"Observing Luna")
        os.write(fd, b"never send this to Luna\r")
        dismiss(fd)
        os.write(fd, b"/agents\r")
        expect(fd, b"Enter open")
        dismiss(fd)
        os.write(fd, b"/settings\r")
        expect(fd, b"Lavender")
        os.write(fd, b"\x1b[C\x1b[B\r")
        expect(fd, b"Sky")
        dismiss(fd)
        os.write(fd, b"draft across restart\x1b2")
        expect(fd, b"Observing Luna")
    messages = [json.loads(line) for line in log.read_text().splitlines()]
    starts = [m for m in messages if m.get("method") == "thread/start"]
    assert len(starts) == 3, len(starts)
    denied = next(m for m in messages if m.get("id") == "auto-denied")
    assert denied["result"]["success"] is False
    turns = [m["params"] for m in messages if m.get("method") == "turn/start"]
    assert len(turns) == 3, len(turns)
    assert turns[1]["sandboxPolicy"] == {"type": "readOnly", "networkAccess": False}
    assert "never send this to Luna" not in log.read_text()
    saved = json.loads(next(Path(directory, "state", "workspaces").glob("*.json")).read_text())
    first = saved["team"]["workers"][0]["handoff"]
    assert first["prompt"] == turns[1]["input"][0]["text"]
    assert first["review"]["verdict"] == "accepted"
    assert first["review"]["commands"][0]["command"] == "git diff --stat"

    log.unlink()
    with terminal(directory, log, "--continue") as fd:
        expect(fd, b"Restored worker-1")
        expect(fd, b"Observing Luna")
        os.write(fd, b"\x12\t")
        expect(fd, b"Accepted by conductor")
        expect(fd, b"git diff --stat")
        dismiss(fd)
        dismiss(fd)
        expect(fd, b"draft across restart")
        os.write(fd, b"\x15/settings\r")
        expect(fd, b"Sky")
        assert b"Off" in screens[fd].text()
        dismiss(fd)
        os.write(fd, b"Delegate one more check\r")
        expect(fd, b"RESUMED_TEAM_COMPLETE")
        expect(fd, b"Context compacted")
    messages = [json.loads(line) for line in log.read_text().splitlines()]
    assert [m["params"]["threadId"] for m in messages if m.get("method") == "thread/resume"] == [
        "lead", "worker-1", "worker-2"
    ]
    assert [m["params"]["threadId"] for m in messages if m.get("method") == "thread/compact/start"] == [
        "lead"
    ]
    assert sum(m.get("method") == "thread/start" for m in messages) == 1

    workspaces = Path(directory, "state", "workspaces")
    original = workspaces / "lead.json"
    saved = json.loads(original.read_text())
    saved["team"].update(lead_id="missing-lead", lead_started=False)
    missing = workspaces / "missing-lead.json"
    missing.write_text(json.dumps(saved))
    original.unlink()
    with terminal(directory, log, "--continue") as fd:
        expect(fd, b"Ready")
        os.write(fd, b"\x1b2")
        expect(fd, b"Restored worker-1")
    recovered = json.loads(original.read_text())
    assert [w["thread_id"] for w in recovered["team"]["workers"]] == [
        w["thread_id"] for w in saved["team"]["workers"]
    ]
    assert not missing.exists()

print("Team terminal smoke passed (launch cap, restored roster and draft, saved settings, resumed delegation and compaction).")
