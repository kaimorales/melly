# Optimization outcome: stock remains the target

This investigation has not demonstrated that Melly beats stock Codex. Keep the installed 0.9.3 executable unchanged. The experimental branch contains protocol improvements, reproducible runtime experiments, and measurements of the losses; it is not a faster release.

## Complete coding comparisons

All rows use Sol High / Luna High and include conductor, worker, and review work. Each row is a distinct candidate/pair, not a pooled sample of one implementation.

| Experiment | Stock task seconds | Melly task seconds | Outcome |
| --- | ---: | ---: | --- |
| Taskboard, dev.1 completion evidence | 478.787 | 554.060 | Both pass; Melly 15.7% slower despite 34.7% less uncached input. |
| Independent cache family, dev.2 | 412.744 | 569.441 | Both pass; Melly 38.0% slower and 22.3% more uncached input. |
| Taskboard, dev.3 aggregate brief budget | 428.612 | 709.948 | Melly fails independent quality audit despite green automated checks; no quality-qualified ratio. |

Details: [Taskboard experiments](2026-11-12-native-context-research.md), [independent cache family](2026-11-13-independent-cache-family.md), and [dev.3 quality failure](2026-11-13-taskboard-dev3.md). These are small, local comparisons, not universal rankings. Tokens do not directly measure subscription quota.

## What changed in Melly

- Worker completion supplies bounded actual diff evidence and observed check output, reducing repeated fetching while preserving conductor review. Separate staged and unstaged changes remain visible even when their net diff cancels.
- Acceptance details can borrow the unchanged combined 10,000-character brief budget. Valid briefs no longer fail only because their acceptance field exceeds 1,000 characters; scope checks and caps remain intact.
- Guidance removes redundant command arguments and encourages batching independent checks while retaining individual results. The measured complete workflow still loses; concise calls alone are insufficient.
- An external quality audit can veto benchmark success. Green visible tests and holdouts cannot override a reproduced defect; malformed audits fail closed.

The frozen Rust candidate passes 134 tests, formatting, Clippy with warnings denied, and its release build; one existing manual render benchmark is ignored. The later benchmark analyzer passes eight offline tests. Those checks establish implementation behavior, not speed superiority.

## Public-fork runtime experiment: rejected

We built the exact open-source Codex 0.167.0 tag in an isolated lab. Its native children already share cache routing. A small patch extends the existing public ephemeral-fork routing to persistent forks without reusing actual thread identity or hiding worker usage. Controlled lookup probes demonstrated a cache mechanism, but not a reliable end-to-end advantage: a warmed control sometimes cached more overall. See the [controlled probe](2026-11-13-codex-cache-runtime.md).

The first coding attempt could not execute code mode because a companion executable was missing. The second exposed a debug assertion on an unfinished custom call copied into the public fork. Both failures and their recorded costs remain in the results directory. We reproduced and repaired the history bug offline with seven targeted regressions, retained the assertion, and built matched control/patched bundles with identical helpers. Seven cache tests and zero-model helper/code-mode smokes also passed.

The corrected runtime executed Luna's edits without that panic. It nevertheless failed the actual coding contract:

1. Equal-priority timestamps `01:00:00.0000002+01` and `01:00:00.0000001+01` were sorted by ID instead of their distinct instants. The conductor disclosed the unresolved defect after its permitted revision.
2. A deeply nested JSON cursor raised `RecursionError` instead of the required `ValueError` on supported Python 3.11.15.

Both failures were independently reproduced without changing the task or tests. A worker also attempted recursive delegation; Melly rejected it, then the worker edited its own assigned files. The actual conductor did not edit. The fork retained parent history and delegation tool definitions despite correct worker instructions. That is a concrete role-confusion observation and an integration risk, not proof that inheritance alone caused the mistake.

We stopped the run during stage three and did not spend on its stock control. The first two stages used **544.495 task seconds**. Across completed and interrupted work, **56 recorded responses**, **154,899 uncached input tokens**, and **20,345 output tokens** were retained. An interrupted model request may have additional server-side usage without a completed local record; these are recorded lower bounds. This partial, incorrect run cannot be compared with a successful full session. [Sanitized failure metrics](../../benchmarks/results/2026-11-13-cached-fork-quality-failure.json) include the partial-stage costs and limitations.

The source patches, adapter, exact source pin, build/control procedure, and upstream Apache attribution are preserved in the [experimental reproduction package](../../benchmarks/experiments/codex-0.167.0/README.md). No binaries or raw account/session logs are published. Public-fork UI/accounting boundaries and cold-resume routing remain unintegrated, so this package is not production-ready.

## Architectural conclusion

The useful next direction is to reuse Codex's native child lifecycle beneath Melly's UI and admission controls, then test focused changes above that baseline. Public full-history forking is not a drop-in replacement. Native setup additionally establishes child identity, inherited-history/accounting boundaries, ownership, permissions, and resume semantics. Making that available through a narrow host API requires actual runtime work: owner-mediated spawn/revision, persisted worker permission grants that leave the conductor read-only, and child event/usage projection. Merely enabling native tools would bypass Melly's caps and inherit the conductor's read-only permissions.

This remains an architectural hypothesis, not a measured speed improvement. Establish parity first with unchanged High/High task contracts and complete accounting; only then evaluate selective skill discovery, evidence transfer, and scheduling. Do not cut review, lower model effort, omit failed work, or increase caps to manufacture a win.

The [research record](2026-11-12-native-context-research.md) covers primary papers and applicability limits. Further reading on [TokenEconomizer, SkillReducer, and repository instructions](2026-11-13-independent-cache-family.md#research-interpretation) informed experiments, but their reported savings are not Melly's results.
