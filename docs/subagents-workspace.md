# Agent workspace

Builds on the approved Melly plan: two automatic Luna launches per user-started lead turn, three active workers including manual launches, and a 15% remaining-account-quota reserve for automatic work. Counters are enforced by the client before a launch, with fresh bounded briefs and event-driven collection.

One Codex App Server connection carries all sessions. The harness routes client replies and server events to their originating agent. Only the lead receives Melly delegation tools; native collaboration is disabled for this process. Read workers use a read-only sandbox. Editing workers declare disjoint owned paths and retain the configured Codex permissions; file ownership is a coordination rule, not a new OS permission boundary.

The UI keeps separate drafts and scroll positions for each agent. `/spawn`, `/agents`, direct agent shortcuts, targeted interruption, and compact result collection operate on the same state as the lead's tools. Tokens and context stay per-thread; account quota remains shared. An approval always identifies its originating agent.

## OpenCode reference

Studied the official MIT-licensed repository at `7bac5bad02f68b95fc5c2c022252c0635ea61abe`, including `packages/tui/src/routes/home.tsx`, `routes/session/message-parts.tsx`, `routes/session/group-view.tsx`, and `feature-plugins/sidebar/context.tsx`.

- Center an original Melly block wordmark over a composer capped near 75 columns on the initial screen.
- Keep the home screen free of an empty activity sidebar.
- Present thinking as a compact amber disclosure row, expanded text muted and indented; keep it distinct from assistant messages.
- Collapse tool output by default. Full command output, including skill instructions, remains available explicitly.
- Use compact textual context and token information, with details on demand.
- Keep agent identity, task, running state and tokens visible in a keyboard-accessible roster.

The implementation is native Rust/Ratatui; no OpenCode runtime dependencies or source components are copied. Reference: https://github.com/anomalyco/opencode

## Reasoning boundary

Codex may emit readable summaries, reasoning-text events, or neither. Render only what the backend provides, prefer summaries, and show a live waiting indicator otherwise. No decrypted, inferred, or fabricated internal thinking. The installed 0.167 protocol's raw-events debug option is internal-only and is not used.

## Verification

Version 0.3.0 passes 76 Rust tests (one manual rendering benchmark is ignored), formatting, all-target Clippy with warnings denied, and both terminal smoke scripts against the release binary. Coverage includes interleaved worker events, request ID collisions, cap exhaustion, slot release, duplicate tool calls, nonblocking waits, interrupted starts, rejected lead starts, report delivery, quota freshness, compact tool presentation, home layout and per-agent keyboard state.

A signed-in Codex integration run completed with one Sol lead, two automatic Luna workers and one manual Luna, checking three disposable files. The lead collected both automatic workers' actual results and each agent exposed its own token usage. This verifies integration; it is not an efficiency benchmark. Model base context can outweigh the benefit of delegating tiny tasks.

Worker compaction and persisted agent-workspace restoration are deferred. Lead compaction remains available, and individual Codex chats can be resumed; automatic delegation currently requires a fresh chat.
