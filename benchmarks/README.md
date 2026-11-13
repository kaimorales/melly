# Live workflow benchmark

[2026-11-12 results: Sol high + Luna high](../docs/benchmarks/2026-11-12-sol-high-luna-high.md)

[Source handoff experiment and fresh comparison](../docs/benchmarks/2026-11-12-context-handoff.md)

This comparison consumes the signed-in Codex plan. It does not call a separately
billed API, predict subscription charges, or convert tokens into plan percentages.
Raw traces can contain account metadata and model/tool output; keep the output
directory outside the public repository.

## Protocol

Three repeated sessions per arm, alternating which arm runs first. Each session
starts from an identical, disposable Python project and receives three successive
requests: repair stable cursor pagination, expose it through a CLI with robust
errors, then implement paginated exports. Standard-library contract tests are
added before each request. Agents may add tests but cannot alter the contracts.
There is no package installation or external service dependency.
Later requests build on that arm's own prior implementation, as in an ongoing
coding session. Compare the complete session totals; the intermediate generated
code can differ even though the initial files and successive requests are identical.
The default cutoff is ten minutes per request. A timeout is incomplete even if
the implementation passes tests; finishing the conductor review is required.

The conductor diagnoses and chooses the implementation approach. Luna edits and
tests; the conductor reviews the diff/results. Two worker assignments or revisions
per request are allowed. Stock Codex enforces that limit through instructions;
Melly additionally enforces it in its harness. Do not equate those mechanisms.

- Conductor: GPT-6.1 Sol, high reasoning.
- Worker: GPT-6 Luna, high reasoning.
- Service tier: default on both.
- Personal memory reads/generation: disabled equally for repeatability.
- Conductor skills context budget: 3,000 tokens on both. Melly retains its 1,200
  token worker skill budget; stock agents retain their native inheritance.
- Sandbox: workspace-write, approval policy never. Melly additionally makes the
  conductor read-only; stock relies on its conductor instructions not to edit,
  since native children inherit the parent's sandbox.
- Stock runs through `codex exec --json`, then `exec resume` with the same thread
  for later tasks. Its conductor profile is overlaid only for this invocation,
  using a project-scoped `bench_luna` role configured high.
  The published run's exact developer instructions are in
  [stock-conductor.md](stock-conductor.md). The runner reads the installed profile;
  changing that profile produces a different baseline and is recorded in each
  private invocation log.
- Melly uses the installed production TUI in a PTY. It remains open for the first
  two requests and is closed/reopened with `--continue` before the third.
- `rpc_tap.py` records Melly's App Server transport and pins thread/turn reasoning
  effort to high. This is a disclosed benchmark-only setting override because
  production Melly 0.9.2 otherwise selects low for newly spawned workers. It does
  not rewrite prompts, tool results, or other behavior. No global defaults change.

## Measurements

The primary time boundary is the conductor's recorded `task_started` to
`task_complete`, including worker execution and review. This excludes process
startup and avoids comparing a warm TUI to a fresh exec subprocess. Observed wall
time and Melly startup/resume time are retained separately. This is an agent
workflow benchmark, not a terminal-emulator rendering benchmark.
The independent grader runs after the conductor finishes and is outside that
timing boundary. Benchmark setup and this experiment's analysis are excluded from
agent usage; the coding conductor and all of its workers are included.

Per-response `token_usage_record` entries are collected for every thread with the
exact project directory. Inherited fork-history records are excluded by matching
the owning thread ID, and response IDs are deduplicated. Input, cached input,
uncached input, output and reasoning are reported separately. Cached tokens are
part of input; reasoning tokens are part of output. Neither is added twice.
Effective model/effort is taken from the context associated with each response,
not from requested settings alone. A missing worker or wrong setting invalidates
the run and halts further spending.

Contract tests are rerun by the benchmark process. Independent holdout checks cover
timezone-equivalent timestamps, stable traversal, deleted/inserted boundaries,
malformed cursors, Unicode, and CLI export behavior. Run the holdout grader against
each final session tree even when reading results from an earlier runner version.
Faster incorrect results do not establish a speed win.

Caching and backend load cannot be fully controlled. Alternating order reduces,
but does not remove, warm-cache effects. Three sessions are a small sample from
one machine/account and a compact synthetic project. Retain individual runs and
failures; do not generalize a percentage to all coding or subscription savings.
This compares the configured conductor workflows, including their prompt and
context policies. It does not establish the best performance attainable by tuning
stock Codex, or isolate Rust UI speed from model/backend latency. The sequential
requests exercise growing context and resume; they are not a long-context
compaction stress test or a benchmark of parallel fan-out across independent tasks.

## Run

Requires Python 3.11+, Git, installed/signed-in Codex, Melly, and the stock conductor
profile used by this project. Defaults locate binaries in `~/.local/bin`.

```sh
python3 benchmarks/compare.py --output /tmp/melly-comparison --repeats 3
python3 benchmarks/analyze.py /tmp/melly-comparison
python3 -m unittest discover -s benchmarks -p 'test_*.py' -v
```

The runner records a manifest, identical prompt files, invocation settings, private
traces, per-stage results and token records. Choose a new output directory for each
experiment; an existing session directory is never silently overwritten.
The analyzer reruns independent checks on every final session tree and withholds
savings percentages if either arm has incomplete, unmatched, or failed sessions.
Offline accounting tests check that inherited and duplicate usage records are
excluded and that failed work cannot become a reported savings win.
