#!/usr/bin/env python3
"""Exercise the full terminal and app-server path through a pseudo-terminal."""

import argparse
import errno
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import shutil
import struct
import subprocess
import tempfile
import termios
import time
import json
from terminal_screen import Screen

screens = {}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("--live", action="store_true", help="use a signed-in Codex and run one Luna turn")
parser.add_argument("--features", action="store_true", help="exercise model, effort, skill and compaction menus with the fixture")
parser.add_argument("--scroll", action="store_true", help="exercise chat wheel and keyboard scrolling with the fixture")
parser.add_argument("--live-model", default="gpt-6-luna", help="model for the opt-in live test")
args = parser.parse_args()
binary = args.binary.resolve(strict=True)
fake_codex = Path(__file__).with_name("fake_codex.py").resolve(strict=True)


def expect(master, needle, timeout):
    deadline = time.monotonic() + timeout
    output = bytearray()
    while time.monotonic() < deadline:
        readable, _, _ = select.select([master], [], [], max(0, deadline - time.monotonic()))
        if not readable:
            break
        try:
            chunk = os.read(master, 64 * 1024)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        output.extend(chunk)
        screen = screens.setdefault(master, Screen())
        screen.feed(chunk)
        if (needle in (output if b"\x1b" in needle else screen.text()) if isinstance(needle, bytes) else needle.search(output)):
            return bytes(output)
    tail = bytes(output[-1500:]).decode(errors="replace")
    raise AssertionError(f"Terminal did not show {needle!r}. Output tail: {tail!r}")


def run(directory):
    subprocess.run(["git", "init", "-q", directory], check=True)
    codex = fake_codex
    if args.live:
        installed_codex = os.environ.get("MELLY_CODEX_BIN") or shutil.which("codex")
        if not installed_codex:
            raise RuntimeError("Install Codex before running the live smoke")
        codex = Path(directory, "smoke-codex")
        codex.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys\n"
            f"executable = {installed_codex!r}\n"
            "os.execv(executable, [executable, '-c', 'sandbox_mode=\"workspace-write\"', "
            "'-c', 'approval_policy=\"never\"', *sys.argv[1:]])\n"
        )
        codex.chmod(0o700)
    pid, master = pty.fork()
    if pid == 0:
        os.chdir(directory)
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 110, 0, 0))
        os.environ["MELLY_DATA_DIR"] = str(Path(directory, "melly-state"))
        os.environ["TERM"] = "xterm-256color"
        os.environ["MELLY_CODEX_BIN"] = str(codex)
        if not args.live:
            os.environ["MELLY_TEST_RPC_LOG"] = str(Path(directory, "rpc.jsonl"))
        os.execv(binary, [str(binary), "--model", args.live_model if args.live else "gpt-6-luna"])

    try:
        startup = expect(master, b"Ready", 30)
        if args.live:
            os.write(master,b"\x0f")  # Ctrl+O: explicitly expand tool output for shell verification.
        if args.features:
            assert not args.live
            os.write(master, b"/mod\t")
            expect(master, b"Choose model", 5)
            os.write(master, b"astra\t\r")
            expect(master, b"Astra", 5)
            os.write(master, b"/effort \x1b[B\x1b[B\r")
            expect(master, b"high", 5)
            os.write(master, b"/skills\r")
            expect(master, b"review", 5)
            os.write(master, b"\r")
            expect(master, b"$review", 5)
        prompt = (
            b"Create hello.txt containing melly-smoke-ok and a newline. "
            b"Use a shell command to print its contents, then reply with MELLY_SMOKE_OK."
            if args.live else b"Test the terminal connection"
        )
        os.write(master, prompt + b"\r")
        if args.live:
            # Ratatui can leave the unchanged "R" on screen when Running becomes Ready.
            ready_header = re.compile(rb"\x1b\[1;\d+H[^\n]{0,150}R?eady")
            output = expect(master, ready_header, 120)
            path = Path(directory, "hello.txt")
            assert path.exists(), f"Codex did not create the file. Output tail: {output[-2000:]!r}"
            assert path.read_text() == "melly-smoke-ok\n"
            assert b"Exited" in output, "No shell exit status was visible in the terminal"
        else:
            output = expect(master, b"MELLY_SMOKE_OK", 10)
            assert b"+ Thought" in screens[master].text()
            assert b"Checking" not in screens[master].text()
            os.write(master, b"\x14")  # Ctrl+T reveals the supplied summary.
            expect(master, b"Checking", 5)
            if args.features:
                requests = [json.loads(line) for line in Path(directory,"rpc.jsonl").read_text().splitlines()]
                turn = next(r for r in requests if r.get("method") == "turn/start")["params"]
                assert turn["model"] == "gpt-6-astra" and turn["effort"] == "high", turn
                assert turn["input"][1]["type"] == "skill", turn
                os.write(master, b"/compact\r")
                expect(master, b"Context compacted", 5)
        if args.scroll:
            assert not args.live
            os.write(master, b"Scroll fixture\r")
            expect(master, b"SCROLL_LINE_099", 5)
            assert b"SCROLL_LINE_000" not in screens[master].text()
            # SGR mouse reports, as emitted by mouse wheels and terminal trackpads.
            os.write(master, b"\x1b[<64;10;10M" * 35)
            expect(master, b"SCROLL_LINE_000", 5)
            assert b"SCROLL_LINE_099" not in screens[master].text()
            assert b"\x1b[?1006h" in startup, "Terminal never enabled SGR mouse input"
            os.write(master, b"\x1b[<65;10;10M" * 40)
            expect(master, b"SCROLL_LINE_099", 5)
            os.write(master, b"\x1b[5~" * 12)  # Page Up
            expect(master, b"SCROLL_LINE_000", 5)
            os.write(master, b"\x1b[1;5F")  # Ctrl+End returns to latest.
            expect(master, b"SCROLL_LINE_099", 5)
            # Easing must finish; an idle chat must not keep repainting.
            deadline = time.monotonic() + 0.3
            while time.monotonic() < deadline:
                if select.select([master], [], [], max(0, deadline - time.monotonic()))[0]:
                    screens[master].feed(os.read(master, 65536))
            assert not select.select([master], [], [], 0.2)[0], "Scroll animation kept redrawing after it stopped"
        os.write(master, b"\x11")  # Ctrl+Q
        cleanup = expect(master, b"\x1b[?1049l", 5)  # Drain the PTY while the terminal is restored.
        if args.scroll:
            assert b"\x1b[?1006l" in cleanup, "Mouse capture was not restored on exit"
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            result, status = os.waitpid(pid, os.WNOHANG)
            if result:
                pid = None
                assert os.waitstatus_to_exitcode(status) == 0
                return
            time.sleep(0.05)
        raise AssertionError("Melly did not quit after Ctrl+Q")
    finally:
        os.close(master)
        if pid is not None:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)


with tempfile.TemporaryDirectory(prefix="melly-terminal-", dir="/tmp") as directory:
    run(directory)
print(f"Terminal smoke passed ({'live Codex' if args.live else 'fixture server'}).")
