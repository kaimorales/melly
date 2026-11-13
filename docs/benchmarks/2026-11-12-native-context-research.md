# Native context and completion evidence

The target is stock Codex, not the previous Melly release. The latest pre-experiment matched Taskboard session took 355.449 seconds in stock and 549.381 in Melly. Both passed. One pair is descriptive, not a reliable universal advantage.

## What the traces say

The 193.932-second gap separates into 57.183 seconds before workers start, 112.795 in worker execution, 16.887 between worker reports and corrections, and 7.067 after the last worker report. Worker patch-producing responses were equal (13 each); Melly requested 23 command rounds versus stock's 9 and spent substantially more output on final worker reports.

Eight final-check response boundaries could be grouped while retaining every separate command and exit code. Expected failing regression tests are useful work, not wasted calls. Two later failures came from flawed test fixtures/assertions; meaningful conductor review caught further defects. Dropping checks or review would be a false improvement.

Workers share one App Server connection. The observed 61-second outer tool yields overlap useful worker execution. They do not explain a three-minute gap. Stock also created a fresh worker for each task in this pair; it did not reuse one worker across the three tasks.

## Exact Codex source audit

Inspected [Codex rust-v0.167.0](https://github.com/openai/codex/tree/rust-v0.167.0), commit `086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a`, Apache-2.0. No Codex implementation was copied into Melly.

- Public `thread/fork` can fork the live conductor's persisted history while its dynamic spawn call is pending. The child receives an interrupted boundary and an aborted result for the unfinished call; the parent remains live.
- Native V2 agents preserve the full prefix by default, including tool observations. Native child setup also establishes child identity, role instructions, and accounting boundaries. Public fork is not a drop-in equivalent.
- A fork's `developerInstructions` override alone does not replace inherited developer history. Melly's existing per-turn application context becomes a later developer message and establishes the worker role.
- Native child permissions inherit the parent's effective permissions. Simply enabling native agents beneath Melly's read-only conductor would produce read-only workers and bypass Melly's current delegation controls.
- Public fork inherits cumulative usage. With `excludeTurns=true`, the server deliberately omits the restored-usage replay. With full turn hydration, baseline replay precedes `thread/started`; a product integration would have to persist that baseline and worker-owned turn boundaries. The first own usage event is not a safe baseline. Current live experiment accounting instead uses existing per-response owner IDs, excluding inherited records.
- Native workers share the root session's cache routing. Persistent public forks receive a new routing identity. Codex's existing ephemeral-fork path shares the parent's request-body cache key and Responses session header while keeping a distinct actual thread identity, but it has no durable rollout writer. There is no supported public promotion to persistent storage; testing ephemeral workers with the current collector would silently omit their costs. A disposable source experiment extends only the existing cache-routing guard to persistent public forks, retaining storage and owned usage records. The minimal patch does not retain shared routing after a cold child resume.

In the first public-fork experiment, the first worker calls account for the uncached-input regression: 40,390 more input tokens and 54,656 fewer cached tokens than stock, partly offset by lower conductor uncached input. Source confirms a routing difference; it does not establish the amount of latency caused by cache misses. The runtime experiment must measure that.

Relevant source: `app-server/src/request_processors/thread_processor.rs`, `core/src/agent/history.rs`, `core/src/agent/child_config.rs`, `core/src/thread_manager.rs`, `core/src/state/additional_context.rs`, and `thread-store/src/local/paginated_fork.rs` under `codex-rs`.

## Research reviewed November 12, 2026

These are mechanism and limitation checks, not transferable speed guarantees.

| Primary paper | Useful mechanism | Limitation for Melly / decision |
|---|---|---|
| [SquidAgent, Oct 6](https://arxiv.org/html/2610.08647v1) | Front-load shared decisions, fork investigation context, schedule only worthwhile parallel work. | Small authored task set and judge-based quality; serial fallback lets the orchestrator edit. Its code has different context transfer paths. Test context inheritance while retaining our Luna-only editing rule. |
| [CodeGrep, Aug 6](https://arxiv.org/html/2608.05886v1) | Deliver focused code evidence before implementation; assess complete workflows. | Uses a trained retriever, one coding-agent family, and selected jointly solved tasks for some efficiency comparisons. Generic retrieval hints can increase tokens. Keep deterministic source delivery; do not add an uncounted retrieval model. |
| [When Agents Coordinate, Aug 17](https://arxiv.org/html/2608.16801v1) | Explicit shared artifacts help when information is genuinely distributed. | Two synthetic tasks, one model; benefits depend on existing coordination. Sealed replication matters. Use a second independent task family and explicit implementation/evidence contracts. |
| [TomasuLLM, Oct 1 revision](https://arxiv.org/html/2609.38201v2) | Speculate future tools in private overlays and commit only after dependency/effect validation. | Extra draft work and setup cost; larger benefits with long-running tools. Our checks are mostly subsecond, so speculative execution is not the first bottleneck to address. |
| [SpecAgent, Oct 15](https://arxiv.org/html/2610.21463v1) | Batch a cheap agent's trajectory and verify meaningful boundaries. | Hosted evaluation loses quality in several pairings; verification/model-effort choices differ from ours. Keep an actual conductor review and full High/High accounting. |
| [SmoothAgent, June 30](https://arxiv.org/html/2607.00151v1) | Prepare predictable context transformations ahead of use. | Requires serving/KV-cache control; transformation-point TTFT is not full coding-task latency. Melly can prepare bounded evidence, not claim hosted KV-cache prewarming. |
| [ServerBench, Nov 4](https://arxiv.org/html/2611.06841v1) | Replay dependent, stateful workloads; verify optimization-specific execution paths independently. | Evaluates serving systems under controlled GPU deployments, not this client harness. Apply its evaluation discipline, not its reported speedups. |
| [Not All AI Agents Are Equal, Sep 17](https://arxiv.org/html/2609.19947v1) | Separate model waiting, tool time, and resource contention before optimizing. | CPU-sensitive workloads can benefit from tool admission controls; Melly's observed gap is mostly model rounds. Do not spend this iteration on Rust rendering or more local cores. |

## Candidate decisions

1. **Full live fork:** retain existing brief/source packet, change only worker creation through a disposable RPC adapter. All three tasks and holdouts pass: 451.568 seconds, 53 model calls, 257,211 uncached input and 17,092 output tokens. Faster than older Melly, still slower and more uncached input than latest stock. Do not ship it.
2. **Fork without duplicate source:** isolate whether carrying both inherited investigation and the source packet is wasteful. Retain the exact implementation brief, permissions, scope, and acceptance checks. Missing evidence remains readable. All checks pass, but this run takes 559.187 seconds with 76 calls, 275,776 uncached input and 21,353 output tokens. Reject this variant; removing supplied evidence did not help this sample.
3. **Selective completion evidence:** keep fresh scoped workers, concise decision-rich briefs, grouped independent tool invocations, and a bounded actual diff returned with the report. The conductor still reads and judges the implementation and observed checks. Any omitted or ambiguous evidence requires direct inspection. No automatic acceptance and no additional LLM summarizer.

Fresh matched result for candidate `67a7942` (0.9.4-dev.1): stock **478.787 seconds**, Melly **554.060 seconds**. All six stages, independent holdouts, settings, caps, accounting and resume checks pass. Both final workspaces additionally pass the 36-check holdout under Python 3.11. Melly uses 65 model responses versus 77 and 175,302 uncached input tokens versus 268,629, but emits 23,325 output tokens versus 19,937. This is **15.7% slower**, despite 34.7% less uncached input. The candidate is not a speed win and is not installed as a release.

The completion packet was delivered with complete tracked/new-file evidence in this run. Its implementation also covers separate staged and unstaged views, including opposing changes whose net HEAD diff is empty. The mechanism reduces conductor responses; improving that local metric alone does not establish better end-to-end performance. Preserve this loss in the experiment record.

### Output audit and next candidate

The remaining gap is in initial worker execution (+95.591 seconds), not corrections (-3.068) or conductor-exclusive work (-17.250). Initial command-only responses emit 1,769 more non-reasoning tokens than stock, patch/write responses 984 more, and final reports 361 fewer. Finished production code plus README is 7,714 characters in both arms. Repeated whole-file rewrites or final implementation size do not explain the difference.

Melly emits unchanged long absolute working directories, unnecessary default arguments, and repeated JSON serialization wrappers. Codex's `unified_exec` resolves omitted workdir to the current turn environment; relative patch paths use that same directory. Candidate 0.9.4-dev.2 asks for concise calls with complete results and unchanged checks. It also replaces the soft brief word target with concrete acceptance fixtures/expected outcomes. These are general execution rules, not task answers. Accepted-review bookkeeping costs only 9.380 seconds here; changing final-response streaming/persistence to save that round is deferred until the larger worker overhead is addressed.

The benchmark project prefix is 150 characters. Absolute-path repetition therefore has a larger cost here than in a short real project directory. Both arms receive equivalent paths; results still need confirmation across workloads and path lengths. Do not turn the wrapper-token opportunity into a universal speed claim.

Results, including losses, must remain visible. Compare frozen tasks, actual model/effort records, contract integrity, external holdouts, resume behavior, caps, and owned usage before calculating speed or token differences. Cached input is part of total input; reasoning output is part of output. These counters do not directly measure Codex subscription allowance.
