"""Small VT screen reader for Ratatui smoke assertions, independent of diff chunking."""
import codecs
import re
import unicodedata


class Screen:
    def __init__(self, width=160, height=60):
        self.width, self.height = width, height
        self.cells = [[" "] * width for _ in range(height)]
        self.x = self.y = 0
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")

    def feed(self, chunk):
        self.pending += self.decoder.decode(chunk)
        while self.pending:
            if self.pending.startswith("\x1b["):
                match = re.match(r"\x1b\[([0-9;?]*)([@-~])", self.pending)
                if not match:
                    break
                args, command = match.groups()
                values = [int(s) if s else 0 for s in args.lstrip("?").split(";")]
                if command in "Hf":
                    self.y = max(0, (values[0] or 1) - 1)
                    self.x = max(0, ((values[1] if len(values) > 1 else 1) or 1) - 1)
                elif command == "J" and values[0] in (2, 3):
                    self.cells = [[" "] * self.width for _ in range(self.height)]
                elif command == "K" and self.y < self.height:
                    start = 0 if values[0] in (1, 2) else self.x
                    end = self.x + 1 if values[0] == 1 else self.width
                    for x in range(start, min(end, self.width)):
                        self.cells[self.y][x] = " "
                elif command in "ABCD":
                    delta = values[0] or 1
                    if command == "A": self.y = max(0, self.y-delta)
                    if command == "B": self.y += delta
                    if command == "C": self.x += delta
                    if command == "D": self.x = max(0, self.x-delta)
                elif command == "G": self.x = max(0, (values[0] or 1)-1)
                self.pending = self.pending[match.end():]
                continue
            if self.pending.startswith("\x1b]"):
                match = re.search(r"\x07|\x1b\\", self.pending)
                if not match: break
                self.pending = self.pending[match.end():]
                continue
            if self.pending.startswith("\x1b"):
                if len(self.pending) == 1: break
                self.pending = self.pending[2:]
                continue
            ch, self.pending = self.pending[0], self.pending[1:]
            if ch == "\r": self.x = 0
            elif ch == "\n": self.y += 1
            elif ch == "\t": self.x = (self.x // 8 + 1) * 8
            elif ord(ch) >= 32 and not unicodedata.combining(ch):
                if self.y < self.height and self.x < self.width:
                    self.cells[self.y][self.x] = ch
                self.x += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1

    def text(self):
        return "\n".join("".join(row) for row in self.cells).encode()
