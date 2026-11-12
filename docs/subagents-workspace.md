# Agent workspace

Builds on the approved Melly plan: two automatic Luna worker turns per user-started conductor turn, three active workers, and a 15% remaining-account-quota reserve for automatic work. Spawns and revisions both count toward the turn cap. Counters are enforced by the client before work starts, with fresh bounded briefs and event-driven collection.

One Codex App Server connection carries all sessions. The harness routes client replies and server events to their originating agent. Only the lead receives Melly delegation tools; native collaboration is disabled for this process. Read workers use a read-only sandbox. Editing workers declare disjoint owned paths and retain the configured Codex permissions; file ownership is a coordination rule, not a new OS permission boundary.

The UI keeps one conductor draft and separate observation scroll positions. `/agents` and keyboard shortcuts open workers for observation; user chat never goes to Luna. The conductor owns spawning, revision and worker compaction. Interruption and result collection operate on the same state as its tools. Tokens and context stay per-thread; account quota remains shared. An approval always identifies its originating agent.

## OpenCode reference

Studied the official MIT-licensed repository at `7bac5bad02f68b95fc5c2c022252c0635ea61abe`, including `packages/tui/src/routes/home.tsx`, `routes/session/message-parts.tsx`, `routes/session/group-view.tsx`, and `feature-plugins/sidebar/context.tsx`.

- Center an original Melly block wordmark over a composer capped near 75 columns on the initial screen.
- Keep the home screen free of an empty activity sidebar.
- Present thinking as a compact amber disclosure row, expanded text muted and indented; keep it distinct from assistant messages.
- Collapse tool output by default. Full command output, including skill instructions, remains available explicitly.
- Use compact textual context and token information, with details on demand.
- Keep active agent identity, task and running state in a compact keyboard-accessible roster. Collapse completed workers to a count; the full picker retains every agent. Show selected worker tokens separately.
- Group consecutive progress, thoughts and tools inline behind disclosures; keep final replies prominent with one footer. Use neutral dark panels, accent-edged user messages and a shaded composer with active/queued model metadata.

The implementation is native Rust/Ratatui; no OpenCode runtime dependencies or source components are copied. Reference: https://github.com/anomalyco/opencode

## Reasoning boundary

Codex may emit readable summaries, reasoning-text events, or neither. Render only what the backend provides, prefer summaries, and show a live waiting indicator otherwise. No decrypted, inferred, or fabricated internal thinking. The installed 0.167 protocol's raw-events debug option is internal-only and is not used.

## Verification

Version 0.3.0 passes 76 Rust tests (one manual rendering benchmark is ignored), formatting, all-target Clippy with warnings denied, and both terminal smoke scripts against the release binary. Coverage includes interleaved worker events, request ID collisions, cap exhaustion, slot release, duplicate tool calls, nonblocking waits, interrupted starts, rejected lead starts, report delivery, quota freshness, compact tool presentation, home layout and per-agent keyboard state.

A signed-in Codex integration run completed with one Sol lead, two automatic Luna workers and one manual Luna, checking three disposable files. The lead collected both automatic workers' actual results and each agent exposed its own token usage. This verifies integration; it is not an efficiency benchmark. Model base context can outweigh the benefit of delegating tiny tasks.

Melly 0.4 stores a local manifest keyed by the lead thread ID. The manifest tracks worker thread IDs, lifecycle and report state, drafts, and the current selection. Codex reloads each transcript and restores the tools registered on the lead. New lead chats register a compaction tool in addition to the existing delegation tools. A model-requested lead compaction starts once the active turn finishes successfully. Idle worker compaction uses a worker slot and preserves the previous result.

## Version 0.6 verification

81 Rust tests passed, with one manual rendering benchmark ignored. Formatting, all-target Clippy, release build and both PTY suites passed. The UI review includes narrow terminal activity expansion and active-versus-queued model metadata.

A real Sol conductor received an ordinary request to fix a disposable Python fixture, with no instruction from the user to delegate. It inspected the source and failing check, passed Luna the exact edit and invariant, waited for its result, and reviewed the actual diff. Luna changed the file and ran the check successfully. This is an integration check, not a general model-quality or usage-efficiency benchmark.

Role instructions now travel on thread start/resume and every turn. The conductor’s read-only sandbox is independent of editing workers’ configured permissions. A worker follows its own server-provided policy even if a brief inaccurately describes the conductor’s restrictions as workspace-wide. Actual permission denials are still respected.
