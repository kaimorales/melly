# Source handoff experiment: Sol high + Luna high

The candidate completed two three-task sessions in 570.042 and 549.381 seconds. All tasks, final external holdouts, and conductor reviews passed. The candidate uses fewer calls than the earlier Melly runs, but stock Codex remains faster in the fresh comparison.

## Change and protocol

Commit `ae5701e` adds bounded source snapshots to initial Luna assignments. Owned files and explicitly selected supporting files arrive with the conductor brief, with no extra model call. The complete prompt remains visible in the handoff view. Source is limited to 12 KiB per file and 32 KiB per serialized packet; omissions are explicit. The behavior is released as 0.9.3; the benchmark binary still reported 0.9.2 before the version-only release bump.

The [original protocol](../../benchmarks/README.md) was unchanged: Sol high, Luna high, default service tier, matching initial project and task text, two worker turns per request, a 600-second cutoff, and resume before task three. Runner, fixture, RPC effort override and holdout source hashes match the published earlier experiment. The candidate canary ran first, followed by one fresh stock-first pair. No account defaults or installed baseline were changed for the experiment.

## Results

Time covers conductor task start through completion, including workers and review. Token counts include all agents. Cached input is part of input; reasoning is part of output.

| Run | Seconds | Model calls | Input | Cached input | Uncached input | Output |
|---|---:|---:|---:|---:|---:|---:|
| Earlier stock (3-session mean) | 468.8 | 70.3 | 4,193,399 | 3,976,405 | 216,994 | 21,226 |
| Earlier Melly 0.9.2 (3-session mean) | 793.1 | 99.3 | 4,170,425 | 3,899,605 | 270,820 | 30,927 |
| Candidate canary | 570.0 | 75 | 2,515,745 | 2,354,816 | 160,929 | 22,028 |
| Fresh stock | 355.4 | 50 | 2,114,232 | 1,946,112 | 168,120 | 15,003 |
| Fresh candidate | 549.4 | 71 | 2,667,369 | 2,470,784 | 196,585 | 21,548 |

The two candidate sessions averaged 559.7 seconds and 73 calls. Relative to the earlier three-session Melly mean, the observed reductions were 29.4% in time, 26.5% in calls, 37.9% in input, 34.0% in uncached input, and 29.6% in output. Both candidate sessions used 41 Luna calls; the earlier Melly mean was 62.7.

The fresh pair is the relevant current comparison: stock took 355.449 seconds and the candidate 549.381 seconds. Melly took 54.6% longer, used 16.9% more uncached input and 43.6% more output. This change does not establish an advantage over stock Codex.

## Verification

- 119 Rust tests passed, one existing manual render test ignored; formatting and Clippy passed. The nine new regression tests failed before implementation and passed afterward.
- Both deterministic terminal smoke suites passed, including delegation, restore, brief/review views, history, drafts and navigation. The two benchmark-accounting tests passed.
- All nine new live tasks passed their contracts; each of the three final session trees passed all 36 independent holdout checks.
- Effective model/effort, usage reconciliation, resume identity and worker-turn caps passed. Candidate worker turns were [2, 2, 2] and [2, 2, 1]; fresh stock was [2, 1, 1].
- All six candidate worker assignments have an accepted conductor review. Fresh stock issued diff inspections after the final worker completion for each task, with no conductor patch calls.
- The resumed candidate sessions attached new source packets normally. Exact prompts, corrections and review evidence persisted.

## Interpretation and next targets

These are small, sequential, nonrandomized samples on one compact Python project. The earlier stock average was 468.8 seconds; fresh stock also ran faster at 355.4 seconds. Model trajectories, caching and backend load vary. The before/after percentages describe these observations; they do not isolate causality or predict other repositories, subscription quota or dollar savings. Failed pilot data from the original experiment remains retained there.

The remaining gap is predominantly model workflow: fresh stock used 26 worker calls and 24 conductor calls versus 41 and 30 in the paired candidate. Native workers retain broader context; the Melly packet still starts a fresh worker. Reusing useful worker context on related assignments is the next experiment. It must preserve per-assignment ownership and inspectable brief/review history. Independent checks can also be batched while preserving actual exit codes. Correction turns should focus on substantive gaps; one canary correction included README style polishing, while other corrections caught real JSON/cursor problems and must remain possible. These follow-ups are not implemented or credited with savings here.

See the [research notes](2026-11-12-optimization-research.md) for the recent papers, their limitations and the hypothesis behind source delivery. [Machine-readable results](../../benchmarks/results/2026-11-12-context-handoff.json) retain every session, accounting category and quality gate. Raw traces remain private because they can contain account metadata and model/tool content.
