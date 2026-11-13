#!/usr/bin/env python3
"""Summarize recorded comparisons without treating a faster failed task as a win."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import sys
from compare import snapshot

QUALIFICATION_SCOPE = ("Qualification reflects automated coverage plus any supplied external audit vetoes, "
                       "not complete correctness. An absent audit means no external audit was supplied.")


def quality_audit(directory):
    """Load an optional external veto; malformed or unreadable audits fail closed."""
    path = directory / "quality_audit.json"
    try:
        try:
            audit = json.loads(path.read_text(encoding="utf-8"))
        except FileNotFoundError:
            if path.is_symlink():
                raise ValueError("quality_audit.json is a broken symlink")
            return {"status": "absent", "passed": None, "reasons": []}
        if not isinstance(audit, dict) or type(audit.get("passed")) is not bool:
            raise ValueError("expected an object with a boolean passed field")
        reasons = audit.get("reasons", [])
        if not isinstance(reasons, list) or any(not isinstance(r, str) or not r.strip() for r in reasons):
            raise ValueError("reasons must be an array of nonempty strings")
        if not audit["passed"] and not reasons:
            reasons = ["External quality audit reported failure without supplying a reason."]
        return {"status": "passed" if audit["passed"] else "failed",
                "passed": audit["passed"], "reasons": reasons}
    except (OSError, UnicodeError, ValueError, RecursionError) as error:
        return {"status": "invalid", "passed": False,
                "reasons": [f"Invalid quality audit: {error}"]}


def comparable(sessions, expected_repeats=None):
    """Do not turn an incomplete or incorrect workflow into a speed win."""
    stock = {s["repeat"] for s in sessions if s["arm"] == "stock"}
    melly = {s["repeat"] for s in sessions if s["arm"] == "melly"}
    return (bool(stock) and stock == melly and
            (expected_repeats is None or stock == set(range(1, expected_repeats + 1))) and
            all(s["qualified"] for s in sessions))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    manifest = json.loads((output / "manifest.json").read_text())
    stages = [json.loads(p.read_text()) for p in sorted(output.glob("repeat-*/*/stage-*/result.json"))]
    sessions = []
    for directory in sorted(output.glob("repeat-*/*")):
        if not (directory / "project").exists(): continue
        repeat, arm = int(directory.parent.name.split("-")[1]), directory.name
        rows = [r for r in stages if r["repeat"] == repeat and r["arm"] == arm]
        if not rows: continue
        stage = max(r["stage"] for r in rows)
        holdout = subprocess.run([sys.executable, str(Path(__file__).with_name("holdout.py")), str(directory/"project"), str(stage)], capture_output=True, text=True, timeout=60)
        (directory / "holdout.txt").write_text(holdout.stdout + holdout.stderr)
        totals = {k:sum(r["usage_delta"][k] for r in rows) for k in rows[0]["usage_delta"]}
        settings_valid = all(r["settings_valid"] for r in rows)
        worker_turns = [len({c["turn_id"] for c in r["calls"] if c["role"] == "worker"}) for r in rows]
        cap_valid = all(1 <= count <= 2 for count in worker_turns)
        usage_reconciled = totals == snapshot(directory / "project", rows[0]["primary_id"])["totals"]
        resume_valid = len({r["primary_id"] for r in rows}) == 1
        automated_qualified = (len(rows)==3 and all(r["completed"] and r["grade"]["passed"] for r in rows)
                               and settings_valid and cap_valid and usage_reconciled and resume_valid
                               and holdout.returncode==0)
        audit = quality_audit(directory)
        sessions.append({"repeat": repeat, "arm": arm, "stages": len(rows),
            "turn_seconds": sum(r.get("turn_seconds", r["wall_seconds"]) for r in rows),
            "observed_wall_seconds": sum(r["wall_seconds"] for r in rows),
            "usage": totals, "conductor_calls":sum(r["conductor_calls"] for r in rows),
            "worker_calls":sum(r["worker_calls"] for r in rows), "settings_valid":settings_valid,
            "worker_turns":worker_turns, "cap_valid":cap_valid,
            "usage_reconciled":usage_reconciled, "resume_valid":resume_valid,
            "contract_passes":sum(r["grade"]["exit_code"] == 0 and r["grade"]["contracts_intact"] for r in rows),
            "holdout_passed":holdout.returncode == 0, "holdout_output":holdout.stdout + holdout.stderr,
            "automated_qualified": automated_qualified, "quality_audit": audit,
            "qualified": automated_qualified and audit["passed"] is not False})
    arms = {}
    for arm in ("stock", "melly"):
        rows = [s for s in sessions if s["arm"] == arm]
        if not rows: continue
        arms[arm] = {"sessions":len(rows), "qualified_sessions":sum(r["qualified"] for r in rows),
            "mean_session_seconds":statistics.mean(r["turn_seconds"] for r in rows),
            "median_session_seconds":statistics.median(r["turn_seconds"] for r in rows),
            "range_session_seconds":[min(r["turn_seconds"] for r in rows),max(r["turn_seconds"] for r in rows)],
            "mean_usage":{k:statistics.mean(r["usage"][k] for r in rows) for k in rows[0]["usage"]},
            "mean_conductor_calls":statistics.mean(r["conductor_calls"] for r in rows),
            "mean_worker_calls":statistics.mean(r["worker_calls"] for r in rows)}
    summary = {"sessions":sessions, "arms":arms, "qualification_scope":QUALIFICATION_SCOPE,
               "comparable":comparable(sessions, manifest["repeats"])}
    if summary["comparable"]:
        a,b=arms["stock"],arms["melly"]
        summary["reductions_pct"]={"time":100*(1-b["mean_session_seconds"]/a["mean_session_seconds"]),
            **{k:100*(1-b["mean_usage"][k]/a["mean_usage"][k]) if a["mean_usage"][k] else None for k in a["mean_usage"]}}
    (output/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    lines=["# Codex vs Melly: repeated coding sessions", "", "GPT-6.1 Sol high + GPT-6 Luna high. Recorded conductor task time includes worker work and review; startup is excluded. Tokens include all agents, deduplicated per response.", "", "Automated coverage: " + QUALIFICATION_SCOPE, "", "| Repeat | Harness | Completed tasks | Contract checks | Holdout | External audit | Qualified | Seconds | Input tokens | Cached input | Uncached input | Output tokens | Model calls |", "|---|---|---:|---:|---|---|---|---:|---:|---:|---:|---:|---:|"]
    for r in sessions:
        u=r["usage"]
        lines.append(f'| {r["repeat"]} | {r["arm"]} | {r["stages"]}/3 | {r["contract_passes"]}/3 | {"Pass" if r["holdout_passed"] else "Fail"} | {r["quality_audit"]["status"]} | {"Yes" if r["qualified"] else "No"} | {r["turn_seconds"]:.1f} | {u["input_tokens"]:,} | {u["cached_input_tokens"]:,} | {u["uncached_input_tokens"]:,} | {u["output_tokens"]:,} | {r["conductor_calls"]+r["worker_calls"]} |')
    audit_reasons = [f'- Repeat {r["repeat"]}, {r["arm"]} ({r["quality_audit"]["status"]}): {" ".join(reason.split())}'
                     for r in sessions for reason in r["quality_audit"]["reasons"]]
    if audit_reasons:
        lines += ["", "External quality audit details:", "", *audit_reasons]
    if not summary["comparable"]:
        lines += ["", "The comparison is incomplete or contains a failed quality/settings check. No savings percentage is calculated."]
    lines += ["", "## Interpretation limits", "", "This is a small synthetic multi-file project, three tasks per session, on one machine/account. Results are descriptive, not a statistically established universal savings rate. Compare quality before time. Cached input is included in input, and reasoning is included in output. Tokens do not directly establish subscription quota or dollar savings.", "", "Stock uses exec/resume; Melly uses its actual TUI and a disclosed high-effort RPC override. Native parent sandbox inheritance and Melly's read-only conductor enforcement differ. Worker context budgets and orchestration remain each harness's native implementation. See benchmarks/README.md for the complete protocol."]
    (output/"report.md").write_text("\n".join(lines)+"\n")
    print(json.dumps({"arms":arms,"reductions_pct":summary.get("reductions_pct")},indent=2))

if __name__ == "__main__": main()
