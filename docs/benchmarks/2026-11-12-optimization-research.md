# Context handoff experiment

Melly 0.9.2 took 793 seconds per three-task session versus stock Codex's 469 in the
[High/High comparison](2026-11-12-sol-high-luna-high.md). Across the nine tasks,
Melly's Luna workers made 188 model calls versus 107 for stock. Smaller worker
inputs did not produce a cheaper complete workflow. This makes avoidable search
and repair rounds a better first target than Rust rendering speed.

## Research checked on November 12, 2026

[The Physical Cost of Context, revision November 4](https://arxiv.org/html/2610.34763v2)
separates evidence delivery from history retention. Its eager source delivery
reduced calls in a localization experiment; repeatedly deleting old observations
in a separate patch experiment damaged prefix reuse and increased uncached work.
It measures quality before admitting an efficiency gain. These are controlled
experiments on local serving stacks, not measurements of Melly, hosted Codex, or
subscription quota. Our inference: test useful source delivered before Luna's
first call, retain history, and measure calls plus uncached input and output.

[TokenEconomizer, October 15](https://arxiv.org/html/2610.22732v1)
uses task history to learn operational instructions, prune unused tools and
intervene in persistent failure loops. It includes the controller's own model
calls in cost accounting. Repeated errors alone do not establish a hopeless
loop; successful runs can repeat errors too. Its SWE-Bench streams contain
related independent fixes rather than real evolving user sessions. This suggests
later experiments with evidence-backed test recipes and selective intervention,
not arbitrary turn limits or another always-running model monitor.

These are recent relevant preprints, not an exhaustive survey or promises that
their reported savings transfer to this harness.

## First change

The conductor still diagnoses the problem and writes the implementation brief.
At spawn, the harness adds complete small owned files plus explicitly selected
supporting files. The packet is bounded to 32 KiB, each file to 12 KiB, and missing
or excluded content is marked. No directory walk, retrieval service, extra model
call, history rewriting or permissions change is introduced. The exact packet is
available in the existing full-prompt handoff view and survives session restore.

Canonical path checks reject paths outside the workspace and identity rechecks
reject detected pathname changes during capture. This is a bounded context selector, not an
OS sandbox against adversarial filesystem races. Existing execution permissions
remain responsible for access control.

The intended mechanism is fewer initial read calls and fewer implementation
mistakes caused by missing nearby source. Larger initial prompts are an explicit
tradeoff. This hypothesis must be tested; source supply alone may not explain the
gap, and existing failure loops may persist.

## Acceptance and next experiments

Use the unchanged benchmark and High/High models. Count every conductor and worker
call, include final review, preserve failures, and require contract and external
holdout checks. Keep the installed baseline while evaluating the candidate.
One canary is enough to reject an obviously harmful change, not enough to claim
a general speed win. Repeated paired runs are needed for a stronger conclusion.

If the packet fails to reduce work, inspect the resulting trajectory before
adding another optimization. Candidate follow-ups are scoped source ranges for
large files, retaining relevant worker context across related tasks, combining
revision and wait into one tool result, and reusing verified repository test
commands. Each needs a separate measured comparison. Avoid tuning instructions
to the benchmark's specific project, tests, or expected answers.
