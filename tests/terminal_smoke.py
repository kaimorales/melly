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

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary", type=Path)
parser.add_argument("--live", action="store_true", help="use a signed-in Codex and run one Luna turn")
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
        if (needle in output if isinstance(needle, bytes) else needle.search(output)):
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
        os.environ["TERM"] = "xterm-256color"
        os.environ["MELLY_CODEX_BIN"] = str(codex)
        os.execv(binary, [str(binary), "--model", "gpt-6-luna"])

    try:
        expect(master, b"Ready", 30)
        prompt = (
            b"Create hello.txt containing melly-smoke-ok and a newline. "
            b"Use a shell command to print its contents, then reply with MELLY_SMOKE_OK."
            if args.live else b"Test the terminal connection"
        )
        os.write(master, prompt + b"\r")
        if args.live:
            # Ratatui can leave the unchanged "R" on screen when Running becomes Ready.
            ready_header = re.compile(rb"\x1b\[1;\d+H(?:\x1b\[[;\d]*m)*R?eady")
            output = expect(master, ready_header, 120)
            path = Path(directory, "hello.txt")
            assert path.exists(), f"Codex did not create the file. Output tail: {output[-2000:]!r}"
            assert path.read_text() == "melly-smoke-ok\n"
            assert b"Exited" in output, "No shell exit status was visible in the terminal"
        else:
            expect(master, b"MELLY_SMOKE_OK", 10)
        os.write(master, b"\x11")  # Ctrl+Q
        expect(master, b"\x1b[?1049l", 5)  # Drain the PTY while the terminal is restored.
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
