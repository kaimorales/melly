# Taskboard dev.3: known quality failure

The candidate is not release-qualified. Automated contracts and holdouts pass, but an independent audit reproduces a known unresolved defect on supported Python 3.11. The conductor disclosed the defect and recorded `changes_requested` after exhausting its worker cap. No quality-qualified savings ratio is reported.

| Three-task session | Stock Codex | Melly 0.9.4-dev.3 |
|---|---:|---:|
| Recorded task seconds | 428.612 | 709.948 |
| Model responses | 65 | 78 |
| Input tokens | 3,061,350 | 3,228,428 |
| Cached input | 2,877,056 | 3,022,976 |
| Uncached input | 184,294 | 205,452 |
| Output tokens | 17,089 | 29,559 |
| Reasoning output, included above | 3,259 | 7,933 |
| Contract stages | 3 / 3 | 3 / 3 |
| Independent holdout assertions | 36 passed | 36 passed |
| External quality audit | Pass | Fail |

These are observed costs, including the unsuccessful Melly work. Settings, complete usage reconciliation, launch caps, contract integrity, and resume checks pass. Startup and independent grading are excluded from recorded task time; all conductor, worker, and review work is included. Both use unchanged installed Codex 0.167.0, Sol High/Luna High, default service tier, equal contracts, and equivalent short workspace paths. Actual order is Melly then stock. Token counters do not directly establish subscription usage.

On Python 3.11.15, a base64url cursor encoding `"[" * 2000 + "0" + "]" * 2000` raises an uncaught `RecursionError` in Melly's final implementation. The contract requires `ValueError` for malformed cursors and explicitly supports Python 3.11+. Stock returns the required `ValueError`. The defect persists after all three stages even though the final unit suites (Melly 44, stock 37) and the sealed 36-assertion holdout pass on that interpreter. Neither the project nor the sealed tests were edited during this audit.

Both conductors complied with the role split: their visible owned calls inspect code/diffs and run checks; workers perform the edits. The original automated summaries are retained privately. A separate external audit now vetoes Melly's qualification in the analyzer and suppresses percentage calculations. Positive audits cannot rescue failed automated checks; malformed audits fail closed. This addresses a measurement flaw rather than hiding the failed run.

The candidate is frozen at `c8281257519533a8479e91796d27ebca70b098c2`, binary SHA-256 `95b23e00468e4c5c465f29f2de81fdb3eab658f18f84edabfba242ac900d6b0c`. Its changes include the earlier completion evidence, leaner call/brief guidance, and the aggregate brief-budget fix. The run does not isolate the causal effect of any one change. All three initial assignment briefs have acceptance fields above the previous 1,000-character field limit and are admitted without truncation under the unchanged combined budget. That is a demonstrated protocol improvement, not proof of a faster complete workflow.

Validation of the frozen candidate: 134 Rust tests passed, one existing manual render benchmark ignored; formatting, Clippy with warnings denied, release build, and two Python accounting tests passed. The subsequent analyzer fix passes eight offline tests, including the audit veto and positive-audit non-override cases. Independent review found no must-fix issue in the aggregate-budget change. The installed CLI remains unchanged. Complete sanitized metrics and audit reasons are in [the results JSON](../../benchmarks/results/2026-11-13-taskboard-dev3.json).

The separate public-fork/shared-cache coding experiment uses custom, matched-profile Codex builds and is labeled separately. It must not be merged into this installed-runtime comparison or treated as evidence that native Codex lacks cache sharing.
