"""Small, deterministic, dependency-free multi-step coding workload."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys

WORKFLOW = """Work only in this project; no network, dependency installs, personal memory, or other projects.
The user has authorized implementation and checks. Do not pause for a design approval.
The conductor must inspect the relevant code, choose a specific approach, and delegate edits/tests to GPT-6 Luna with HIGH reasoning. In stock Codex use the project-scoped bench_luna agent (not luna_implementer). In Melly use its edit worker.
Give concrete findings, steps, owned paths and checks. At most two worker assignments/revisions per user turn. Workers must not delegate. The conductor must not edit. Wait for the worker, inspect the real diff and test evidence once, and give a concise final result. Preserve existing tests; add tests as needed. Do not commit. Do not inspect benchmark runner files outside this project.
"""

BASE = {
    "AGENTS.md": WORKFLOW,
    ".gitignore": "__pycache__/\n*.pyc\n",
    ".codex/agents/bench_luna.toml": '''name = "bench_luna"
description = "Implement the conductor's scoped changes and run the test/fix loop."
model = "gpt-6-luna"
model_reasoning_effort = "high"
developer_instructions = "Follow the conductor's concrete approach and file scope. You are not alone in the codebase; preserve others' edits. Implement, test, fix in-scope failures, and report changed files, commands and actual results concisely. Do not delegate."
''',
    "README.md": "# Taskboard\n\nA small Python task-file query CLI. Python 3.11+, standard library only.\n\nRun `python3 -m taskboard list tasks.json`. Check with `python3 -m unittest discover -s tests -v`.\n",
    "taskboard/__init__.py": '"""Query task files without a database."""\n',
    "taskboard/__main__.py": "from .cli import main\nraise SystemExit(main())\n",
    "taskboard/storage.py": '''import json
from pathlib import Path

def load_tasks(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise ValueError("task file must contain a JSON array")
    return data
''',
    "taskboard/query.py": '''def page(tasks, *, limit=20, cursor=None, status=None):
    """Return a page of tasks and its continuation cursor."""
    rows = sorted(tasks, key=lambda task: task["title"])[:limit]
    if status is not None:
        rows = [task for task in rows if task["status"] == status]
    return {"items": rows, "next_cursor": None}
''',
    "taskboard/cli.py": '''import argparse
import json
from .query import page
from .storage import load_tasks

def main(argv=None):
    parser = argparse.ArgumentParser(prog="taskboard")
    sub = parser.add_subparsers(dest="command", required=True)
    listing = sub.add_parser("list")
    listing.add_argument("file")
    args = parser.parse_args(argv)
    print(json.dumps(page(load_tasks(args.file))))
    return 0
''',
    "tests/test_baseline.py": '''import unittest
from taskboard.query import page

class Baseline(unittest.TestCase):
    def test_empty(self):
        self.assertEqual(page([]), {"items": [], "next_cursor": None})
''',
}

SAMPLE = [
    {"id": 8, "title": "Zulu", "priority": 2, "created_at": "2026-01-02T00:00:00Z", "status": "open"},
    {"id": 3, "title": "Beta", "priority": 3, "created_at": "2026-01-01T01:00:00+01:00", "status": "done"},
    {"id": 2, "title": "Alpha", "priority": 3, "created_at": "2026-01-01T00:00:00Z", "status": "open"},
    {"id": 5, "title": "Gamma", "priority": 3, "created_at": "2026-01-02T00:00:00Z", "status": "open"},
    {"id": 9, "title": "Delta", "priority": 1, "created_at": "2025-12-31T00:00:00Z", "status": "done"},
]

PROMPTS = [
    """Fix taskboard.query.page pagination. Sort by priority descending, then created_at chronologically (ISO 8601 offsets represent real instants), then numeric id ascending. Filter status before limiting. Do not mutate the input. limit must be an integer from 1 through 100 (bool is invalid); status must be None, 'open' or 'done'. Return {'items': [...], 'next_cursor': string-or-None}. Generate an opaque URL-safe cursor when more matching rows remain; it must encode a stable ordering position and the status filter. Continuation is exclusive and must still work if the task at the previous page boundary has since been deleted. Reject malformed cursors or cursors from a different status filter with ValueError. No duplicates or skipped rows across pages. Inspect the existing code/tests, have Luna high implement and test, and review its diff and results. Run python3 -m unittest discover -s tests -v. Add regression tests.""",
    """Extend the taskboard list CLI to expose the query API: --limit (default 20), --after CURSOR, and --status open|done. Output exactly one JSON object containing items and next_cursor, followed by a newline. Invalid limits, malformed or wrong-filter cursors, missing files, malformed JSON and non-array task files must exit 2 with a short explanation on stderr, no traceback, and empty stdout. Keep --help successful. Update README with pagination/filter examples and add subprocess-level tests. Preserve the previous pagination behavior. Diagnose, delegate implementation/testing to Luna high, then review the diff and results. Run python3 -m unittest discover -s tests -v.""",
    """Add exports to the existing taskboard list CLI: --all walks all matching pages using the existing page API (--limit is the batch size), and --format json|jsonl defaults to json. With --all and JSON, output {'items': every matching task in stable order, 'next_cursor': None}. JSONL emits only one compact JSON task per line, in the same order, with no metadata; empty matches produce empty stdout. JSONL without --all exports just the selected page. Reject --all combined with --after (exit 2, empty stdout). Keep all existing error handling, filtering, pagination and default JSON behavior. Document exports and add regression tests, including more than 100 tasks. Have Luna high edit/test, then review its diff and results. Run python3 -m unittest discover -s tests -v.""",
]

QUERY_TESTS = '''import copy
import unittest
from taskboard.query import page
TASKS = SAMPLE_LITERAL

class Pagination(unittest.TestCase):
    def test_order_and_no_mutation(self):
        original = copy.deepcopy(TASKS)
        self.assertEqual([x["id"] for x in page(TASKS)["items"]], [2,3,5,8,9])
        self.assertEqual(TASKS, original)
    def test_walk(self):
        result, cursor = [], None
        for _ in range(10):
            p = page(TASKS, limit=2, cursor=cursor)
            result += [x["id"] for x in p["items"]]
            cursor = p["next_cursor"]
            if cursor is None: break
            self.assertIsInstance(cursor, str)
        self.assertEqual(result, [2,3,5,8,9])
        self.assertIsNone(cursor)
    def test_filter_first(self):
        p = page(TASKS, limit=2, status="open")
        self.assertEqual([x["id"] for x in p["items"]], [2,5])
        self.assertEqual([x["id"] for x in page(TASKS, limit=2, status="open", cursor=p["next_cursor"])["items"]], [8])
    def test_deleted_boundary(self):
        p = page(TASKS, limit=2)
        remaining = [x for x in TASKS if x["id"] != 3]
        self.assertEqual([x["id"] for x in page(remaining, limit=2, cursor=p["next_cursor"])["items"]], [5,8])
    def test_bad_arguments(self):
        for limit in (0, 101, -1, True, 1.5, "2"):
            with self.subTest(limit=limit), self.assertRaises(ValueError): page(TASKS, limit=limit)
        with self.assertRaises(ValueError): page(TASKS, status="missing")
        with self.assertRaises(ValueError): page(TASKS, cursor="not!a!cursor")
    def test_cursor_filter(self):
        p = page(TASKS, limit=1, status="open")
        with self.assertRaises(ValueError): page(TASKS, status="done", cursor=p["next_cursor"])
'''.replace("SAMPLE_LITERAL", repr(SAMPLE))

CLI_TESTS = '''import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
TASKS = SAMPLE_LITERAL

class Cli(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name, "tasks.json")
        self.path.write_text(json.dumps(TASKS))
    def run_cli(self, *args):
        return subprocess.run([sys.executable, "-m", "taskboard", "list", str(self.path), *args], capture_output=True, text=True, timeout=5)
    def test_page_and_filter(self):
        r = self.run_cli("--limit", "2", "--status", "open")
        self.assertEqual(r.returncode, 0, r.stderr)
        p = json.loads(r.stdout)
        self.assertEqual([x["id"] for x in p["items"]], [2,5])
        r = self.run_cli("--limit", "2", "--status", "open", "--after", p["next_cursor"])
        self.assertEqual([x["id"] for x in json.loads(r.stdout)["items"]], [8])
    def test_invalid_arguments(self):
        for args in [("--limit", "0"), ("--limit", "many"), ("--after", "invalid!")]:
            r = self.run_cli(*args)
            self.assertEqual(r.returncode, 2)
            self.assertEqual(r.stdout, "")
            self.assertTrue(r.stderr)
            self.assertNotIn("Traceback", r.stderr)
    def test_invalid_files(self):
        for text in ["{", "{}"]:
            self.path.write_text(text)
            r = self.run_cli()
            self.assertEqual(r.returncode, 2)
            self.assertEqual(r.stdout, "")
            self.assertNotIn("Traceback", r.stderr)
        self.path.unlink()
        r = self.run_cli()
        self.assertEqual(r.returncode, 2)
        self.assertEqual(r.stdout, "")
        self.assertNotIn("Traceback", r.stderr)
    def test_help(self):
        self.assertEqual(self.run_cli("--help").returncode, 0)
'''.replace("SAMPLE_LITERAL", repr(SAMPLE))

EXPORT_TESTS = '''import json
from test_cli_contract import Cli

class Export(Cli):
    def test_all_and_jsonl(self):
        rows = [{"id": i, "title": str(i), "priority": i % 4, "created_at": "2026-01-01T00:00:00Z", "status": "open" if i % 2 else "done"} for i in range(207, 0, -1)]
        self.path.write_text(json.dumps(rows))
        expected = sorted([x for x in rows if x["status"] == "open"], key=lambda x: (-x["priority"], x["id"]))
        r = self.run_cli("--all", "--limit", "7", "--status", "open")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(json.loads(r.stdout), {"items": expected, "next_cursor": None})
        r = self.run_cli("--all", "--limit", "11", "--status", "open", "--format", "jsonl")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual([json.loads(line) for line in r.stdout.splitlines()], expected)
    def test_empty_jsonl(self):
        self.path.write_text("[]")
        r = self.run_cli("--all", "--format", "jsonl")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "")
    def test_jsonl_single_page(self):
        r = self.run_cli("--limit", "1", "--format", "jsonl")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual([json.loads(line)["id"] for line in r.stdout.splitlines()], [2])
    def test_invalid_all_after(self):
        cursor = json.loads(self.run_cli("--limit", "1").stdout)["next_cursor"]
        r = self.run_cli("--all", "--after", cursor)
        self.assertEqual(r.returncode, 2)
        self.assertEqual(r.stdout, "")
'''

CONTRACTS = [("test_query_contract.py", QUERY_TESTS), ("test_cli_contract.py", CLI_TESTS), ("test_export_contract.py", EXPORT_TESTS)]

def create(project):
    project.mkdir(parents=True, exist_ok=False)
    for name, content in BASE.items():
        path = project / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    subprocess.run(["git", "init", "-q"], cwd=project, check=True)
    subprocess.run(["git", "add", "."], cwd=project, check=True)
    subprocess.run(["git", "-c", "user.name=Benchmark", "-c", "user.email=benchmark@example.invalid", "commit", "-qm", "Unmodified benchmark baseline"], cwd=project, check=True)
    return hashlib.sha256(json.dumps(BASE, sort_keys=True).encode()).hexdigest()

def add_contract(project, stage):
    name, content = CONTRACTS[stage - 1]
    (project / "tests" / name).write_text(content)

def grade(project, stage):
    intact = all((project / "tests" / name).read_text() == content for name, content in CONTRACTS[:stage])
    result = subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", "tests", "-v"], cwd=project, capture_output=True, text=True, timeout=45)
    holdout = subprocess.run([sys.executable, str(Path(__file__).with_name("holdout.py")), str(project), str(stage)], cwd=project, capture_output=True, text=True, timeout=45)
    return {"passed": result.returncode == 0 and holdout.returncode == 0 and intact,
            "exit_code": result.returncode, "contracts_intact": intact, "output": result.stdout + result.stderr,
            "holdout_exit_code": holdout.returncode, "holdout_output": holdout.stdout + holdout.stderr}
