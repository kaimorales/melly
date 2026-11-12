# Brief and review view

Approved direction: make conductor delegation inspectable, with a minimal view showing exactly what Luna was told and whether review was recorded.

## Contract

- Keep the chat and compact sidebar. Ctrl+R opens a worker's handoff sheet. Brief and Review tabs switch with Tab or left/right. Esc closes; arrows/Page Up/Down scroll; Ctrl+N/P switches worker. `/review` opens the latest worker from conductor chat. User input never goes to workers.
- Brief contains the exact submitted worker prompt, including the conductor fields and harness wrapper. Preserve subsequent correction instructions verbatim. No model call is needed to inspect this.
- Review distinguishes task completion, report delivery, actual command exit codes, and an explicit conductor verdict. Completion/delivery never implies acceptance. Do not infer review from prose or command-name keywords.
- Add a conductor-only `review` tool with worker ID, verdict (`accepted` or `changes_requested`) and concise summary. Capture observed conductor commands after report delivery as supporting evidence, labeled by source. This is a conductor assertion, not independent proof of correctness. It must only apply to the current finished worker attempt after its report was delivered.
- Persist handoffs, corrections, review verdicts, result and bounded command evidence across restart; defaults allow old manifests to load. Older sessions without a stored brief/verdict show unavailable/not recorded. Existing Codex threads retain their registered tool schemas; new chats expose `review`.
- Display a small lifecycle rail and animate only active work, respecting motion preference. No fabricated testing phase or progress percentage. Neutral existing palette, accent for selected tab/current stage, muted labels.

## Tasks

1. Add durable handoff/review data and tool with protocol tests covering exact prompts, no automatic acceptance, revision invalidation, stale/worker calls, duplicate calls, resume and old-manifest defaults. Own `src/delegation.rs`, a small `src/handoff.rs` if useful, `src/lib.rs`, `tests/delegation.rs`, `tests/fake_agents.py`.
2. Add a focused `src/handoff_ui.rs` renderer and integrate keyboard/palette in `agent_ui.rs`, `ui.rs`, `commands.rs`, `main.rs`. Test narrow screens, scroll, tab switching, observation-only behavior and approval preemption. Capture actual Ratatui renders for visual review.
3. Run fmt/test/clippy/release and both PTY suites, extend team smoke for handoff view/resume, live-test explicit review tool once in a disposable repo, review diff, document/version/install/publish.

## Verification and retention decisions

The original assignment and prompt are retained verbatim. Keep the most recent 32 exact follow-up prompts, with an explicit omitted count for older history. Retain the latest 12 command outcomes per source, command labels capped at 240 characters and actual output excerpts at 4,000 characters with explicit ellipsis. Output stays out of model-facing status/wait responses. Legacy missing metadata is unknown. Restoring a session never reconstructs fresh review evidence from old transcript chronology.

Implemented and checked with the Rust suite, both PTY suites, captured Ratatui screens, and a real ordinary coding request. The live conductor inspected the actual diff and recorded acceptance through the new tool. The view showed the saved verdict and diff output. This is an integration check, not proof of correctness on arbitrary tasks.
