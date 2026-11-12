# Chat and conductor implementation plan

**Goal:** Follow the approved OpenCode chat layout while making the conductor's diagnosis-to-Luna handoff reliable.

**Architecture:** Keep native Ratatui and the existing Codex transport. UI changes concern presentation only; conductor and worker role instructions travel through official thread/turn fields without changing configured worker permissions.

**Constraints:** User chats only with conductor. Preserve caps, resume, compaction, approval handling, usage truthfulness and observation controls. No portfolio edits. No new UI dependencies.

## Task 1: Conductor handoff

Files: `src/delegation.rs`, `tests/delegation.rs`, `tests/fake_agents.py`.

- [x] Add failing protocol regressions for explicit conductor/worker roles on start/resume/turn, including worker-owned permissions independent of conductor read-only. Test real outgoing messages.
- [x] Give conductor developer instructions: inspect relevant code, identify evidence and chosen approach, delegate implementation with paths, concrete plan, invariants and acceptance checks; don't dump a diagnosis into chat and stop. Don't transfer its read-only restrictions to Luna. Wait and inspect actual final diff and observed checks, revise only concrete defects.
- [x] Give edit workers authoritative role instructions on start/resume/turn: use their own server-provided sandbox/approval rules, never infer permissions from conductor context; execute edits/checks within scope. Never bypass genuine restrictions.
- [x] Keep existing tool fields compatible with saved threads; improve schema descriptions to solicit findings/plan/checks, retain bounded contexts and caps.
- [x] Run delegation tests and review changed protocol shapes; live test later must ask for an ordinary fix without explicitly telling the model to delegate.

## Task 2: OpenCode-style conversation

Files: `src/ui.rs`, `src/agent_ui.rs`.

- [x] Add rendering regression for grouped thought/tool activity, single final-response footer, Markdown thinking, compact sidebar without raw shell logs, observation navigation.
- [x] Use neutral near-black chat with shaded user prompts and accent left edge. Small header, shaded composer with left accent and metadata below prompt. Remove top composer rule and redundant permanent shortcuts.
- [x] Group consecutive progress, thoughts and tools into collapsible per-turn activity. Keep assistant final text prominent, single footer, tool details available via existing toggles. Render Markdown in expanded thoughts. Preserve literal shell commands in details.
- [x] Sidebar: context and quota once, active task rows, selected worker usage, completed count with selected-completed row still visible. Full roster remains in picker. No raw shell feed by default.
- [x] Verify small terminal sizes and capture representative conversation/team screenshots.

## Task 3: Integration and release

- [x] Update PTY assertions for intentional UI changes and run both terminal suites.
- [x] Run fmt, tests, clippy and release build. Live-test ordinary coding request in disposable repository and inspect conductor brief, worker edits/tests and final review.
- [x] Review combined diff, update documentation/version, and install verified binary.
- Release: publish the verified commit to `main` through a fast-forward update.
