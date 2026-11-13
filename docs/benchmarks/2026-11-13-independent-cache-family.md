# Independent cache workload: dev.2 versus stock

The independent workload is a loss for Melly. Both harnesses pass, but fewer model calls and lower total input do not translate into better time or uncached usage.

| Three-task session | Stock Codex | Melly 0.9.4-dev.2 |
|---|---:|---:|
| Recorded task seconds | 412.744 | 569.441 |
| Model responses | 60 | 54 |
| Input tokens | 3,166,659 | 2,445,281 |
| Cached input | 2,966,144 | 2,200,064 |
| Uncached input | 200,515 | 245,217 |
| Output tokens | 17,259 | 22,758 |
| Contract stages | 3 / 3 | 3 / 3 |
| Independent holdout assertions | 2,276 passed | 2,276 passed |

Melly is **38.0% slower**, uses **22.3% more uncached input**, and emits **31.9% more output** in this pair. Total input is 22.8% lower. This is one descriptive pair, not a statistically established effect. Tokens do not directly establish subscription-quota savings.

## Protocol

Both use installed Codex 0.167.0, Sol High plus Luna High, the same task contracts, default service tier, no personal memory, a two-worker-turn request cap, and a 600-second task deadline. Melly uses its actual TUI; stock uses exec/resume. Each session resumes before stage three. Melly runs first, then stock, with equivalent short project paths. Task time includes delegation, worker execution, and review; startup and independent grading are excluded. All owning-thread model responses are counted, and usage reconciles with the independent ledger. Settings, launch limits, contracts, and resume checks pass in both arms.

The workload was constructed and sealed before its first live model run. It implements a Python standard-library TTL/LRU cache, then per-key single-flight loading, then invalidation during active loads. Injected clocks and controlled thread events replace timing-dependent sleeps. Sixteen deliberate broken implementations were rejected; two passed visible contracts but failed the holdout. The reference passed twenty repeated concurrency runs. Both live implementations exercised the optional joined-waiter checks: zero skipped observations. Both final projects also pass under Python 3.11.15: Melly 37 unit tests, stock 33, plus all 2,276 holdout assertions per arm with zero skipped waiter observations.

Candidate source is `649f31065a72c9c00a4300f54b2fee329bd3bf8d`. Binary, fixture, holdout, and baseline hashes are in [the sanitized results](../../benchmarks/results/2026-11-13-cache-family.json). The sealed reference, grading implementation, raw model traces, and account data remain outside the public repository.

## What failed and what changed

The leaner calls were adopted: worker traces contain no repeated working-directory fields, absolute project prefixes, or JSON serialization wrappers. However, the first brief was rejected because `expected_output` contained 1,016 characters against its 1,000-character field limit. Its complete four-field narrative was only 2,501 characters. Re-emitting the brief added a 15.483-second model-response interval and 544 output tokens.

The subsequent dev.3 fix lets acceptance details borrow unused narrative space while preserving the original 10,000-character maximum across the four fields. It does not truncate instructions, increase the total budget, change permissions, remove checks, or spend a launch on an invalid request. Boundary, Unicode, preservation, and schema tests cover the change.

Patch failures also consume time, but attribution matters. Two malformed README attempts span 37.306 seconds and 1,974 output tokens; one response also successfully repairs a test, so that whole interval is not waste. The latter identical malformed attempt alone consumes 13.487 seconds and 715 output tokens. Stock also encountered README patch-context failures. A tool-level recovery strategy would need to account for partial success and changed file contents, not blindly retry or suppress errors.

Stage one's initial implementation patch takes nearly the same model-response interval in both arms. Its remaining worker gap includes additional regression construction, fixing a mistaken eviction fixture, and command-only responses. Removing legitimate test coverage to improve the score would not satisfy the optimization goal.

## Research interpretation

[TokenEconomizer](https://arxiv.org/html/2610.22732v1) motivates environment-specific rules validated against actual traces, with recovery for incorrect pruning or intervention. Its Codex experiments use an older runtime and restart interrupted invocations; that is not a drop-in implementation for Melly's live agent UI. Its reported savings cannot be assigned to this candidate.

[SkillReducer](https://arxiv.org/html/2603.29919v1) separates compact routing metadata from skill content loaded on demand and evaluates whether compression preserves behavior. This supports investigating selective worker context, not deleting user skills or assuming arbitrary truncation is safe. The [AGENTS.md efficiency study](https://arxiv.org/html/2601.20404v1) also cautions against treating all instructions as overhead: useful repository guidance can reduce work. These papers provide hypotheses; the matched coding results determine what ships.
