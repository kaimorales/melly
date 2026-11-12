# Codex vs Melly: Sol high + Luna high

**Stock Codex was faster in all three matched sessions. Melly did not show a meaningful total-input saving: input was 0.5% lower, while uncached input was 24.8% higher and output was 45.7% higher.** This longer test does not support a blanket claim that Melly is faster or more usage-efficient.

All 18 requested coding turns passed the unchanged contract tests and independent stage checks. Every final project also passed the 36-check holdout. The experiment contains three repeated, three-task sessions per harness: nine coding turns each.

## Average complete session

| Metric | Stock Codex | Melly | Melly change |
|---|---:|---:|---:|
| Agent workflow time | 7:49 | 13:13 | +69.2% |
| Input tokens, including cached input | 4,193,399 | 4,170,425 | -0.5% |
| Cached input tokens | 3,976,405 | 3,899,605 | -1.9% |
| Uncached input tokens | 216,994 | 270,820 | +24.8% |
| Output tokens, including reasoning | 21,226 | 30,927 | +45.7% |
| Model calls, all agents | 70.3 | 99.3 | +41.2% |

Times are minutes:seconds. Cached input is part of input; reasoning is part of output. Those subsets are not added twice. Token counts do not establish subscription-quota or dollar savings.

## Individual sessions

| Repeat | Order | Stock time | Melly time | Stock uncached input | Melly uncached input | Quality |
|---|---|---:|---:|---:|---:|---|
| 1 | Stock, Melly | 9:30 | 16:07 | 191,472 | 329,502 | Both pass |
| 2 | Melly, Stock | 6:44 | 9:55 | 274,028 | 252,009 | Both pass |
| 3 | Stock, Melly | 7:13 | 13:37 | 185,482 | 230,949 | Both pass |

Median session time: stock **7:13**, Melly **13:37**. There is substantial run-to-run variation; these are descriptive results from a small sample.

## What the traces show

| Across all three sessions | Stock Codex | Melly |
|---|---:|---:|
| Conductor model calls | 104 | 110 |
| Luna model calls | 107 | 188 |
| Mean Luna input per model call | 66,604 | 29,959 |
| Mean conductor input per model call | 52,438 | 62,536 |

Melly gave Luna about 55% less input per call, but Luna made about 76% more calls. Its conductor also carried more input per call. The net total-input difference was almost zero. In inspected CLI traces, Melly workers reread files and repaired newly written tests across multiple rounds; one stock worker completed the comparable edit with three model calls.

Native stock workers inherited conductor context; stock reused one worker across all three requests in repeat 1 and created separate workers in repeats 2 and 3. Melly created a fresh worker for each request. Context policy is a plausible contributor, but this experiment does not isolate it from instructions, implementation choices, review thoroughness, model variability, or backend load.

Melly reached its initial ready state in 0.17–0.71s and resumed in 0.28–0.91s. These are TUI readiness measurements, not model response times. The workflow slowdown was measured over minutes, far larger than the measured launch times.

## Controls and validation

- Installed versions: Codex CLI 0.167.0; Melly 0.9.2, with checkout `551fa7b` unchanged during the experiment.
- Both used GPT-6.1 Sol high and GPT-6 Luna high, default service tier, personal memory disabled, and a maximum of two worker turns per request. Effective settings and caps were verified from the recorded calls.
- Production Melly otherwise defaults new workers to low effort. A logged, benchmark-only App Server wrapper pinned thread/turn effort to high; it did not rewrite prompts or tool results. Normal account and app defaults were unchanged.
- Same initial file hash and three identical requests: stable cursor pagination; CLI/error handling; then paginated JSON/JSONL export. Later requests used each arm’s own preceding implementation.
- Stock used `codex exec`/`exec resume` with the conductor profile. Melly used the installed TUI in a PTY, closed and reopened before task three. All sessions retained the same conductor thread ID; Melly restored its earlier workers and continued delegating.
- Main timing is the conductor’s task-start to task-complete event, including worker execution and review. Startup and independent post-run grading are excluded. Usage includes the conductor and every worker, excluding experiment setup/reporting.
- Per-response usage was deduplicated by owning thread and response ID. Inherited fork records were excluded. Per-task totals reconciled exactly to complete session logs in all six sessions.
- Review traces showed no conductor patch calls. Seventeen stages showed a full diff read after the last worker completion. In the remaining stock stage, the conductor read the full diff after the first pass, then inspected the targeted test changes and reran checks after the correction.
- The benchmark runner and grader still matched their recorded source hashes. Contract test contents remained intact. All final projects passed the independent holdout.

## Retained pilot and limits

An initial pilot used a five-minute cutoff. Stock completed its three tasks; Melly reached that cutoff on task one before finishing the conductor review, although its tests passed. That is an incomplete run, not a success. Its logs are retained separately and it is excluded from the fresh main batch, which used a ten-minute cutoff per task on both sides. No main-batch task timed out.

This is one small synthetic Python project on one machine/account, with three repetitions. Cache state and backend load were not fully controlled. These configured workflows retained their own orchestration instructions, context policies, skill budgets, and sandbox behavior. The result is not a universal estimate, a comparison against every possible tuned stock configuration, or a parallel-agent/long-context-compaction stress test.

**Next optimization target:** give Luna sufficient reusable task context to avoid repeated discovery, improve the brief’s acceptance criteria, and reduce unnecessary test-repair rounds. Measure those changes with this same frozen suite before claiming an efficiency gain. These are proposed experiments, not fixes proven by this run.

[Full protocol and rerun instructions](../../benchmarks/README.md) · [Exact stock conductor instructions](../../benchmarks/stock-conductor.md) · [Sanitized measurements](../../benchmarks/results/2026-11-12-sol-high-luna-high.json)

Raw model/session traces remain local and are not included in the public repository.
