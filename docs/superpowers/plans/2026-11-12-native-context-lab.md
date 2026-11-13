# Beat stock Codex on verified work

Target: lower end-to-end task time and token use than stock Codex with Sol high and Luna high, without weakening correctness, review, permission boundaries, or caps. Improvements over older Melly are diagnostic, not the success criterion.

## Evidence and experiments

1. Audit the exact installed Codex 0.167.0 source and owned response traces. Separate worker execution, brief construction, correction cycles, and final review. Read current agent-efficiency research, including limitations and implementation details.
2. Disposable experiment: fork the live conductor for each Luna worker, retaining the existing brief and source packet. Keep this outside the installed product because public forks inherit history and cumulative accounting. Result: all checks pass, 451.568 seconds, 257,211 uncached input tokens. Still behind the latest stock run (355.449 seconds, 168,120 uncached). Do not ship this candidate.
3. Test a selective handoff candidate: fresh scoped workers; concise evidence-backed briefs; batch independent tool calls while preserving every command's exit status; bounded completion evidence containing the actual owned-path diff, new-file contents, and observed check outcomes. The conductor still reads and assesses the evidence. Omitted/stale evidence requires direct inspection. No automatic acceptance.
4. Run the same frozen Taskboard contracts and independent holdouts through the actual TUI. Preserve every experiment, including losses. Then compare against a fresh stock run and a second independent cache/concurrency task family. Do not tune the second task family to a candidate's failures.
5. Ship only changes supported by tests and matched evidence. Report sample size, uncertainty, failures, cached/uncached input, output, and model calls. Subscription quota savings cannot be inferred directly from token counts.

## Guardrails

- Conductor diagnoses and reviews; Luna edits/tests. No conductor editing shortcut.
- Same Sol/Luna models, high effort, service tier, caps, tasks, contracts, and independent grading in both arms.
- A completion packet contains observed data, not an invented review or proof that tests cover everything.
- Preserve concurrent changes, resume state, exact brief visibility, and check failures.
- Context packets and diff packets are bounded; incomplete data is explicit. Non-Git workspaces and capture failures fall back to normal inspection.
- No custom model serving, KV-cache claims, hidden grading information, or extra uncounted model calls.
