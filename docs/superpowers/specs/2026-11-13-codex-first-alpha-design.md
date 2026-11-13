# Melly: Codex-first alpha product and release plan

Date: November 13, 2026. Status: proposed plan for review; product implementation has not started under this plan.

## Product decision

Melly is a beautiful terminal workspace where a developer talks to one lead, watches delegated coding work, understands the changes and checks, and resumes tomorrow without reconstructing the task. The first audience is individual developers already using Codex on macOS. Existing Codex sign-in is the first connection; OpenRouter/BYOK follows a usable alpha.

The first release must be worth choosing for its workflow, clarity, and responsiveness. Speed and efficiency remain engineering goals, with measured claims tied to specific workloads. Universal superiority to stock Codex is not a launch requirement. Correctness, truthful usage, dependable recovery, and no hidden model calls are requirements.

The public pitch: **A clearer, calmer way to code with an agent team.** Show the product doing a real task; do not sell an unproven percentage improvement.

## What the foundation already gives us

Codex already supports subagent orchestration, model/effort configuration, and activity inspection. Current documentation distinguishes explicit/project-instructed local delegation from proactive behavior in other settings; a hidden team is not necessarily running on every prompt. This means merely adding agents is not a sufficient differentiator. [Official subagent documentation](https://learn.chatgpt.com/docs/agent-configuration/subagents).

App Server is the integration surface intended for applications with persistent conversations, streaming, tools, and approval handling. Keep Melly's existing Rust client on it. [Official platform guidance](https://developers.openai.com/blog/codex-as-a-platform), [App Server protocol](https://learn.chatgpt.com/docs/app-server).

The inspected release source is Codex `rust-v0.167.0`, commit `086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a`. Documentation can move ahead of this pin: generate/check the actual supported protocol and test compatibility before upgrading. Do not silently accept arbitrary installed versions.

Melly already has the command palette, model/effort controls, skills, compaction, project history, team restore, observation views, exact briefs, recorded reviews, usage/context views, smooth scrollback, and reduced-motion settings. Keep and harden these. Existing fake-server and terminal fixtures cover much of this without model calls. Audit the original behavior before rewriting it.

## Three architectural options

| Approach | Benefit | Cost / decision |
| --- | --- | --- |
| Keep Melly's Rust UI on Codex App Server; selectively reuse native agents | Preserves working product and upstream agent machinery; small integration boundary | Recommended. Native integration gets a bounded feasibility gate, not an open-ended rewrite. |
| Maintain a broad fork of the whole Codex application | Maximum internal access | Substantial upstream, binary, and platform maintenance. Only a narrow, tested runtime bridge is eligible now. |
| Port OpenCode or build a universal agent runtime | Provider breadth or full control | Replaces working foundations and enlarges scope. Defer; OpenCode remains a design reference. |

Melly owns presentation, task/brief/review state, delegation admission, user preferences, and its session index. Codex owns model execution, its tool loop, configured sandboxing/approvals, context machinery, and durable thread history. There must be one authoritative execution state: reconnect reconciles persisted Melly task metadata against server state, not another independent agent loop.

Introduce only the backend boundary needed by real changes: session operations, normalized events, capabilities, and accounting semantics. Keep Codex-specific payloads in the adapter. Do not build a generic plugin framework before a second backend needs one.

## The signature experience

A developer asks for a feature. The lead explains the approach briefly, creates a precise assignment, and Luna implements and tests. The developer sees activity without interviewing the worker. At completion, the same task opens into changed files, check results, and the lead's review. Closing and reopening restores the task and team.

The main view remains a readable chat with one composer and a compact sidebar. A focused task inspector contains Brief, Activity, Changes, and Review. Reuse the existing agent and handoff views; do not create four permanent panels. The task state is explicit: planning, implementing, checking, awaiting review, changes requested, accepted, interrupted, or blocked. Test success never silently becomes acceptance.

The first 45–60 second demo should show: open Melly, request a small real change, inspect an exact brief and live worker activity, see a test outcome and diff, see the lead's review, then resume the saved session. Label time cuts in the recording. A local fixture-driven demo mode can support design work, but must be visibly labeled simulated.

Visual work should improve that sequence: consistent gutters and spacing, restrained accent color, legible Markdown/diffs, smooth anchored scrolling, and responsive menus. The logo gets one short reveal that never blocks typing. Motion indicates actual activity, respects reduced motion, stops when idle, and never invents progress percentages. Essential status cannot depend on color alone. Preserve the current 20 fps animation ceiling; use the existing bounded scroll animation separately.

## Workflow and controls

- The user talks only to the lead. Worker views remain observation views; there is no manual worker chat or spawn control in the initial release.
- Lead questions appear in the main conversation/composer. Worker clarification requests pause that worker and return a blocker to the lead; they do not open a second user conversation. Actual tool approval requests remain visible for the user's explicit decision, including requests originating from workers.
- Discover models from the signed-in account. Prefer Sol for the lead and Luna for implementation when available; let the user select another available worker model when they are not. Do not hardcode Astra, require access to one model, or silently replace a selected model. Keep model and effort choices explicit.
- The lead answers ordinary questions directly. For repository edits, it investigates, chooses an approach, and gives Luna a concrete brief: objective, evidence, owned paths, implementation direction, invariants, and checks. Luna edits and runs the repair/test loop. The lead checks the diff and observed results afterward and requests a focused correction only when warranted.
- Preserve a genuinely read-only conductor. Native children must not quietly inherit that restriction and become unable to edit; nor may the conductor become writable to make native spawning work.
- Start with one implementation worker at a time. Expose parallel implementation later, once disjoint ownership and integration checks are verified. Preserve the existing limit of two worker assignments/revisions per user-started task and the 15% quota reserve. Steering, reconnecting, and restarting must not reset the task's spent budget. If that durability is missing, fixing it is a release gate.
- Enforce worker admission and recursion rules in the harness, not only in prompts. Owned paths are currently coordination boundaries, not an OS-level per-file sandbox; the interface must not imply otherwise.
- When a limit is reached with unfinished work, show the defect and remaining work. The lead asks the user to extend the task budget; it does not claim completion or secretly retry.
- Live activity comes from server events. Opening sessions, selecting agents, viewing diffs, changing visual settings, and inspecting usage cause zero model calls. Compaction is an explicit operation that can consume usage.
- Keep account quota, per-agent token counts, and estimated context occupancy separate. Unknown values stay unknown. Token totals are not a claim about exact subscription consumption. Reported reasoning summaries stay distinguishable from tool output and ordinary chat.

The strict lead/worker split has a handoff cost even for tiny edits. Keep the user's chosen workflow; measure that cost instead of promising that every request will be faster.

## Milestones and stop conditions

These are focused engineering timeboxes, not calendar promises. The first technical spike determines whether native integration fits the alpha. Do not run all phases in parallel.

| Milestone | Timebox | Deliverable and exit condition |
| --- | --- | --- |
| 0. Prove the execution boundary | 1–2 working days | One child can start, edit/test, return evidence, be revised/interrupted, and resume with its parent. Permissions, caps, identity, and usage remain correct. Otherwise keep the existing fresh-thread backend for alpha and park the native bridge. |
| 1. Complete the everyday task loop | 2–3 days | Interactive agent questions work; approvals and failures are clear; interrupted/reconnected work stays truthful; task/brief/diff/review state survives restart; no silent message loss or duplicate execution. |
| 2. Make the task loop feel excellent | 2–3 days | Polish the existing chat and inspector, not a redesign from scratch. Replay deterministic sessions at small and wide sizes; typing, scrollback, selection, and reduced motion work while output streams. |
| 3. Package a private alpha | 1–2 days | Tested macOS ARM64 binary with checksums, version requirements, quickstart, troubleshooting, release notes, and a clean-machine install test. No Rust toolchain required for users. |
| 4. Validate with five developers | About one week of usage | Observe first install and real tasks; fix the top recurring friction. At least three voluntarily return for a second session before expanding the roadmap. |
| 5. Publish the first public alpha | After the above gates | Real task demo, concise README, feature/limitation matrix, reproducible release artifacts, and a feedback template. Explain that Melly is an independent Codex-powered client. |

Milestones 1–3 are roughly one to two focused engineering weeks if the execution spike is bounded. Calendar time depends on available working time and what the install/recovery checks reveal. Keep a milestone incomplete when its exit condition fails.

### Milestone 0: exact first engineering task

Read the pinned schemas and existing native integration findings first. Do not start another paper search or reproduce the failed full-history public-fork approach. Public child input and permissions are known gaps: native V2 children need owner-mediated input; their permissions normally inherit the parent's. A narrow host bridge may be necessary. Its build/helper distribution and upstream maintenance costs count toward the decision.

Produce an isolated proof with these assertions:

1. A child is authorized from an active conductor invocation; unrelated callers are rejected.
2. The conductor remains read-only; a scoped worker can edit under the configured user-authorized policy. Permission grants survive cold resume and are revalidated.
3. The task budget is enforced for spawn and revision; recursive or alternate native-tool paths cannot bypass it. Persist budget consumption across restart.
4. Activity, completion, interruption, and errors map into Melly's existing views. Usage counts only child-owned responses, not inherited parent history.
5. Close/reopen restores roles, child relationships, briefs, reviews, and drafts. An interrupted task stays interrupted until explicitly continued.

Use source-level tests and fake model/event streams first. If they pass, allow at most two tiny live scenarios: edit/test/review, then resume/revision. Each has a two-minute timeout and a local stop threshold of 20 observed model responses; an in-flight request can still incur unreported usage after interruption. Both scenarios use the configured models and count all agents. These checks establish feasibility, not a speed claim.

Stop if the bridge requires broad core refactoring, permissions or caps become advisory, owned usage/resume remains ambiguous, or the timebox expires. Save the evidence and retain the supported fresh-thread backend. Do not let a native rewrite delay user testing indefinitely.

### Milestone 1 file ownership map

This is a planning map, not a request to rewrite these modules wholesale:

| Area | Existing code | Work |
| --- | --- | --- |
| Protocol and connection | `src/rpc.rs`, `src/main.rs` | Compatibility checks, connection lifecycle, truthful startup/reconnect states. |
| Server interactions | `src/session.rs`, `src/ui.rs` | Interactive question forms and pending-request recovery alongside existing approvals. |
| Delegation and persistence | `src/delegation.rs`, `src/state.rs` | Durable task budgets, interruption/restart reconciliation, role/cap enforcement. |
| Inspection | `src/agent_ui.rs`, `src/handoff_ui.rs`, `src/review_evidence.rs` | Coherent task inspector; preserve exact briefs, real diffs, and separate review evidence. |
| Visual system | `src/visual.rs`, `src/session_ui.rs`, `DESIGN.md` | Consistent geometry, status hierarchy, minimal motion and responsive layouts. |
| Verification and delivery | Existing `tests/` fixtures; new release workflow under `.github/workflows/` | Offline interaction/recovery coverage, packaged install test, narrowly scoped live checks. |

Extract a small module when the touched behavior needs a clean boundary. Do not schedule an unrelated refactor of all large UI/session files.

## Token and experiment budget

Default development uses unit tests, event replay, fake App Server fixtures, terminal captures, and real human inspection. Never spend live model tokens to check spacing or a menu. Freeze fixtures and the selected model/effort before comparing runtime changes.

The entire pre-alpha engineering path gets at most four tiny live scenarios: two for Milestone 0 and two final integration checks for the packaged build (fresh install/task completion, then resume/revision). Each uses the same two-minute timeout and 20-observed-response stop threshold. Do not reset that allowance for another candidate; additional live testing needs a newly agreed budget. Visual/recovery development uses deterministic fixtures and existing recordings. Tester-initiated real tasks are their explicit normal product use, not hidden benchmarks.

Performance research is a separate later milestone. Its initial allowance is one candidate, one written hypothesis, and one comparison pack after offline gates pass: three representative task pairs, at most six live task executions, at most five minutes per execution, the normal worker-turn cap, and the existing quota reserve. Another candidate requires a newly agreed total budget. This is screening, not enough evidence for a broad speed claim. Preserve failure costs, quality vetoes, settings, run order, and all recorded agent usage. Stop on repeated infrastructure failure instead of burning the remaining timeout waiting for a dead worker.

Do not rerun performance benchmarks for presentation-only changes. A candidate that passes screening needs a separately budgeted, repeated evaluation before any public savings claim. Benchmark work resumes only for a specific observed bottleneck, not because another paper suggests a large headline percentage.

For alpha, track successful task completion, lost/duplicated work, unexpected model calls, time to first useful activity, and whether users choose to return. Local responsiveness targets: p95 key-to-frame work under 50 ms on the recorded reference Mac; no scheduled animation frames while idle. These are acceptance targets to measure, not existing benchmark claims. If model work is noticeably more expensive or slower, show the evidence and fix or disclose the tradeoff before promotion.

## OpenRouter and broader providers

OpenRouter already documents Codex integration through a custom Responses-compatible provider. That gives us a concrete compatibility route to test, not proof that every model supports Melly's workflow. [OpenRouter's Codex guide](https://openrouter.ai/blog/tutorials/codex-cli-openrouter/).

After repeat use in the Codex alpha, test one OpenRouter configuration with one OpenAI model and one non-OpenAI model. Verify streamed text, tool argument/result fidelity, cancellation, resume, context metadata, approvals, and reported usage. Start with one provider per workspace; mixed-provider teams wait. Model choices must come from available capabilities, and missing reasoning/usage features must be shown honestly.

Reuse Codex provider configuration if it passes those checks. If it does not, decide whether a second backend is justified by actual user demand. Keep authentication separate from UI state; do not save keys in session transcripts or change the user's global Codex setup silently. Provider costs, when available, are distinct from Codex-plan quota. Supporting a Claude model through OpenRouter would not mean embedding the Claude Code product.

## Explicitly deferred

An autonomous swarm, speculative agent execution, automatic model tournaments, a plugin marketplace, background daemons that keep working after exit, cloud session sync, cross-provider agents in one task, custom image-based terminal graphics, and broad platform support are outside this alpha. Full Codex command parity is also outside scope: publish the supported subset and handle unsupported requests visibly.

The next action after plan review is Milestone 0. Its deliverable is a pass/fail architecture decision with evidence, not another performance promise. The parallel-free product path is then the everyday task loop, visual polish, packaging, and five real users.
