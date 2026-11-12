"""Offline regression checks for benchmark accounting; no model calls."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import compare
import analyze


class Accounting(unittest.TestCase):
    def test_only_matched_successful_sessions_support_a_savings_claim(self):
        rows = [{"repeat": 1, "arm": "stock", "qualified": True},
                {"repeat": 1, "arm": "melly", "qualified": True}]
        self.assertTrue(analyze.comparable(rows))
        self.assertTrue(analyze.comparable(rows, expected_repeats=1))
        self.assertFalse(analyze.comparable(rows, expected_repeats=3))
        self.assertFalse(analyze.comparable([]))
        self.assertFalse(analyze.comparable(rows[:1]))
        self.assertFalse(analyze.comparable(rows + [{"repeat": 2, "arm": "stock", "qualified": True}]))
        rows[1]["qualified"] = False
        self.assertFalse(analyze.comparable(rows))

    def test_fork_history_and_repeated_records_are_not_billed_twice(self):
        def context(model, effort):
            return {"type": "turn_context", "payload": {"model": model, "effort": effort}}

        def usage(thread, response, n, cached):
            return {"type": "token_usage_record", "timestamp": response, "payload": {
                "thread_id": thread, "response_id": response, "turn_id": "turn",
                "usage": {"input_tokens": n, "cached_input_tokens": cached,
                          "output_tokens": 10, "reasoning_output_tokens": 4, "total_tokens": n + 10}}}

        parent = usage("parent", "response-1", 100, 80)
        child = usage("child", "response-2", 50, 10)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            logs = []
            for thread, records in (
                ("parent", [context("gpt-6.1-sol", "high"), parent, parent]),
                ("child", [context("gpt-6.1-sol", "low"), parent,
                           context("gpt-6-luna", "high"), child, child]),
            ):
                path = root / (thread + ".jsonl")
                path.write_text("".join(json.dumps(r) + "\n" for r in records))
                logs.append((path, {"id": thread}))
            with patch.object(compare, "session_logs", return_value=logs):
                result = compare.snapshot(root, "parent")
        self.assertEqual(result["totals"], {
            "input_tokens": 150, "cached_input_tokens": 90, "uncached_input_tokens": 60,
            "output_tokens": 20, "reasoning_output_tokens": 8, "total_tokens": 170,
        })
        self.assertEqual([(c["role"], c["model"], c["effort"]) for c in result["calls"]], [
            ("conductor", "gpt-6.1-sol", "high"), ("worker", "gpt-6-luna", "high"),
        ])


if __name__ == "__main__":
    unittest.main()
