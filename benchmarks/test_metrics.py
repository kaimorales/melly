"""Offline regression checks for benchmark accounting; no model calls."""
import io
import json
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
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


class QualityAudit(unittest.TestCase):
    def analyze_pair(self, audit=None, automated_passed=True):
        usage = {"input_tokens": 100, "cached_input_tokens": 60,
                 "uncached_input_tokens": 40, "output_tokens": 10,
                 "reasoning_output_tokens": 4, "total_tokens": 110}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "manifest.json").write_text(json.dumps({"repeats": 1}))
            for arm in ("stock", "melly"):
                directory = root / "repeat-1" / arm
                (directory / "project").mkdir(parents=True)
                for stage in range(1, 4):
                    step = directory / f"stage-{stage}"
                    step.mkdir()
                    passed = arm == "stock" or automated_passed
                    row = {"repeat": 1, "arm": arm, "stage": stage,
                           "primary_id": arm + "-primary", "completed": True,
                           "wall_seconds": 10, "turn_seconds": 9,
                           "usage_delta": usage, "settings_valid": True,
                           "calls": [{"turn_id": f"{arm}-worker-{stage}", "role": "worker"}],
                           "conductor_calls": 1, "worker_calls": 1,
                           "grade": {"passed": passed, "exit_code": 0 if passed else 1,
                                     "contracts_intact": True}}
                    (step / "result.json").write_text(json.dumps(row))
                if arm == "melly" and audit is not None:
                    (directory / "quality_audit.json").write_text(audit)
            with (patch.object(analyze.sys, "argv", ["analyze.py", str(root)]),
                  patch.object(analyze.subprocess, "run", return_value=SimpleNamespace(
                      returncode=0, stdout="holdout passed\n", stderr="")),
                  patch.object(analyze, "snapshot", return_value={
                      "totals": {key: value * 3 for key, value in usage.items()}}),
                  redirect_stdout(io.StringIO())):
                analyze.main()
            return (json.loads((root / "summary.json").read_text()),
                    (root / "report.md").read_text())

    def test_known_unresolved_defect_vetoes_green_checks_and_reductions(self):
        reason = "Python 3.11 cursor decoding still raises an uncaught RecursionError."
        summary, report = self.analyze_pair(json.dumps({"passed": False, "reasons": [reason]}))
        melly = next(row for row in summary["sessions"] if row["arm"] == "melly")
        self.assertTrue(melly["automated_qualified"])
        self.assertFalse(melly["qualified"])
        self.assertFalse(melly["quality_audit"]["passed"])
        self.assertEqual(melly["quality_audit"]["reasons"], [reason])
        self.assertFalse(summary["comparable"])
        self.assertNotIn("reductions_pct", summary)
        self.assertIn(reason, report)

    def test_positive_audit_cannot_rescue_failed_automated_checks(self):
        summary, _ = self.analyze_pair(json.dumps({"passed": True, "reasons": []}),
                                       automated_passed=False)
        melly = next(row for row in summary["sessions"] if row["arm"] == "melly")
        self.assertFalse(melly["automated_qualified"])
        self.assertTrue(melly["quality_audit"]["passed"])
        self.assertFalse(melly["qualified"])
        self.assertFalse(summary["comparable"])
        self.assertNotIn("reductions_pct", summary)

    def test_malformed_audit_fails_closed(self):
        for audit in ("{broken", "[]", "{}", '{"passed": "false"}', '{"passed": 1}',
                      '{"passed": true, "reasons": "all good"}',
                      '{"passed": true, "reasons": [null]}',
                      '{"passed": true, "reasons": [" "]}', "[" * 2000 + "]" * 2000):
            with self.subTest(audit=audit):
                summary, report = self.analyze_pair(audit)
                melly = next(row for row in summary["sessions"] if row["arm"] == "melly")
                self.assertEqual(melly["quality_audit"]["status"], "invalid")
                self.assertFalse(melly["qualified"])
                self.assertFalse(summary["comparable"])
                self.assertNotIn("reductions_pct", summary)
                self.assertIn("Invalid quality audit", report)

    def test_failed_audit_without_details_still_reports_a_reason(self):
        summary, report = self.analyze_pair('{"passed": false}')
        melly = next(row for row in summary["sessions"] if row["arm"] == "melly")
        self.assertFalse(melly["qualified"])
        self.assertTrue(melly["quality_audit"]["reasons"])
        self.assertIn("without supplying a reason", report)

    def test_unreadable_audit_is_invalid(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / "quality_audit.json").mkdir()
            audit = analyze.quality_audit(directory)
        self.assertEqual(audit["status"], "invalid")
        self.assertFalse(audit["passed"])

    def test_absent_audit_preserves_automated_qualification_with_coverage_label(self):
        summary, report = self.analyze_pair()
        self.assertTrue(summary["comparable"])
        self.assertIn("reductions_pct", summary)
        for row in summary["sessions"]:
            self.assertTrue(row["qualified"])
            self.assertTrue(row["automated_qualified"])
            self.assertEqual(row["quality_audit"]["status"], "absent")
        self.assertIn("Automated coverage", report)
        self.assertIn("not complete correctness", report)


if __name__ == "__main__":
    unittest.main()
