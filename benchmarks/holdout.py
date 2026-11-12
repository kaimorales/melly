"""Independent acceptance cases, run outside the coding agents' workspaces."""
from datetime import datetime, timezone
import copy
import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile

project, stage = Path(sys.argv[1]).resolve(), int(sys.argv[2])
sys.path.insert(0, str(project))
from taskboard.query import page

checks = 0

def check(condition, message):
    global checks
    checks += 1
    if not condition: raise AssertionError(message)

def key(row):
    return (-row["priority"], datetime.fromisoformat(row["created_at"].replace("Z", "+00:00")).astimezone(timezone.utc), row["id"])

def records(seed, size=137):
    rng = random.Random(seed)
    offsets = ["2026-02-01T01:00:00+01:00", "2026-01-31T18:00:00-06:00", "2026-02-01T01:00:00Z"]
    rows = [{"id": i, "title": f"Task λ {i}", "priority": rng.randrange(6),
             "created_at": rng.choice(offsets), "status": rng.choice(["open", "done"])} for i in range(size)]
    rng.shuffle(rows)
    return rows

for seed in range(5):
    rows = records(seed)
    saved = copy.deepcopy(rows)
    for status in (None, "open", "done"):
        expected = sorted([r for r in rows if status is None or r["status"] == status], key=key)
        actual, cursor = [], None
        for _ in range(200):
            result = page(rows, limit=7 + seed, cursor=cursor, status=status)
            actual.extend(result["items"])
            cursor = result["next_cursor"]
            if cursor is None: break
        check(actual == expected and cursor is None, f"incorrect traversal seed={seed} filter={status}")
    check(rows == saved, "query mutated caller data")
    first = page(rows, limit=13)
    boundary = first["items"][-1]
    changed = [r for r in rows if r["id"] != boundary["id"]]
    changed.append({"id": 9999, "title": "Inserted before cursor", "priority": 99, "created_at": "2000-01-01T00:00:00Z", "status": "open"})
    expected = sorted([r for r in changed if key(r) > key(boundary)], key=key)[:11]
    check(page(changed, limit=11, cursor=first["next_cursor"])["items"] == expected, "cursor used unstable position")

for bad in ("", "e30=", "W10=", "bnVsbA==", "💥", "not-a-valid-cursor"):
    try: page(rows, cursor=bad)
    except ValueError: checks += 1
    else: raise AssertionError(f"malformed cursor accepted: {bad!r}")

if stage >= 2:
    with tempfile.TemporaryDirectory(prefix="taskboard-grader-") as tmp:
        path = Path(tmp) / "data with spaces.json"
        rows = records(99, 211)
        path.write_text(json.dumps(rows))
        def cli(*arguments):
            return subprocess.run([sys.executable, "-m", "taskboard", "list", str(path), *arguments], cwd=project, capture_output=True, text=True, timeout=10)
        first = page(rows, limit=9, status="done")
        result = cli("--limit", "9", "--status", "done")
        check(result.returncode == 0 and json.loads(result.stdout) == first and not result.stderr, "CLI page differs from query")
        cursor = first["next_cursor"]
        result = cli("--status", "open", "--after", cursor)
        check(result.returncode == 2 and not result.stdout and "Traceback" not in result.stderr, "CLI wrong-filter cursor handling")
        if stage >= 3:
            expected = sorted(rows, key=key)
            result = cli("--all", "--limit", "1")
            check(result.returncode == 0 and json.loads(result.stdout) == {"items": expected, "next_cursor": None}, "all export batch=1")
            result = cli("--all", "--limit", "100", "--format", "jsonl")
            check(result.returncode == 0 and [json.loads(x) for x in result.stdout.splitlines()] == expected, "JSONL all export")
            result = cli("--status", "done", "--after", cursor, "--limit", "3", "--format", "jsonl")
            expected = page(rows, status="done", cursor=cursor, limit=3)["items"]
            check(result.returncode == 0 and [json.loads(x) for x in result.stdout.splitlines()] == expected, "JSONL continuation")

print(json.dumps({"passed": True, "checks": checks}))
