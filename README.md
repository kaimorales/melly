# Melly

A fast Rust terminal workspace for Codex. Start from a centered composer, delegate bounded work to Luna, and jump between agents with separate conversations, activity and token counts. Melly uses the official Codex App Server and your existing Codex sign-in.

## Run it

Install a current Rust toolchain and the [Codex CLI](https://learn.chatgpt.com/docs/codex/cli), then sign in with `codex login`.

```sh
cargo install --path . --locked
cd /path/to/your/project
melly
```

Melly discovers available models from Codex and prefers GPT-6.1 Sol when offered. Start with a specific model using `melly --model gpt-6-astra`, or change it with `/model`. Run `melly --continue` from the same directory to reopen the most recently saved workspace, or `melly --resume THREAD_ID` with the lead ID from `/status`. Melly restores the lead, its saved agents and drafts, and the selected agent. Codex supplies their chat history. A turn interrupted by closing Melly does not restart automatically. If you only used workers and Codex never saved the empty lead, Melly opens a new lead chat and keeps the same workers.

Local workspace files and preferences live in `~/.local/state/melly` by default. Melly also honors `XDG_STATE_HOME` and `MELLY_DATA_DIR`. Workspaces created before 0.4 have no saved agent roster. Their lead chat can still be resumed by ID.

## Commands

Type `/` to open the palette. Type to filter, use Up/Down to select, Tab to complete, and Enter to apply. Esc closes the menu. For a literal message beginning with `/`, press Esc then Enter.

| Command | Action |
| --- | --- |
| `/agents` | Open the agent switcher; Up/Down selects, Enter opens, Ctrl+D stops |
| `/spawn task` | Start a read-only Luna worker with a fresh brief |
| `/spawn --edit src/a.rs,tests/a.rs -- task` | Start an editing Luna with explicit owned paths |
| `/model` | Pick from the models available to your account |
| `/effort` or `/strength` | Pick a reasoning effort supported by the selected model |
| `/skills` | Browse enabled Codex skills and attach one to the next message |
| `/compact` | Ask Codex to compact the selected agent while idle |
| `/settings` | Save preferences for accent, animation, thinking and tool detail, and automatic delegation |
| `/context` | Inspect the latest context estimate and model capacity |
| `/usage` | Inspect session tokens and refresh account usage limits |
| `/status` | Inspect model settings, connection timing, directory and resume ID |
| `/thinking` | Expand or collapse completed thinking summaries |
| `/tools` | Expand or collapse full tool output |
| `/collect` | Return to the lead; unseen results accompany your next lead message |
| `/activity` | Focus shell commands, tool output and file changes |
| `/help` | Open the keyboard reference |
| `/stop` | Interrupt the active turn |
| `/quit` | Close Melly |

Model and effort changes apply to the next turn. New chats start at the selected model's advertised default effort; resumed chats preserve their settings. The header distinguishes active settings from queued settings while a turn runs. Sending a message during a turn steers it with its current settings. Skill selection attaches a typed skill input; opening a menu or inspecting usage does not make a model call.

## Keyboard

| Key | Action |
| --- | --- |
| Enter | Send a message, steer a running turn, or apply a menu choice |
| Up / Down | Navigate a menu |
| Tab | Complete a menu choice; otherwise switch panes |
| Ctrl+B | Switch panes |
| Ctrl+T | Expand or collapse completed thinking summaries |
| Ctrl+O | Expand or collapse full tool output |
| Alt+1 … Alt+9 | Jump directly to an agent; Alt+1 opens the lead |
| Ctrl+N / Ctrl+P | Next / previous agent (portable fallback) |
| Ctrl+U | Clear the draft and attached skills |
| Page Up / Page Down | Scroll the focused pane |
| Shift+Enter | Newline, if supported by the terminal; multiline paste also works |
| Esc | Close a menu or return to the conversation |
| Ctrl+C | Interrupt a running turn, or quit when idle |
| Ctrl+Q | Quit |

The inspector sits beside the chat in wide terminals, with an agent roster, selected-agent context and tokens, and live activity. Narrow terminals switch between panes. Each agent keeps its own draft and scroll position. Typing brings the composer back into view.

When a supported command or patch needs approval, inspect the action, press `y` to select allow, then Enter to confirm. Press `n` to decline. Melly preserves the sandbox and approval policy configured in Codex.

## Delegation

The lead can call Melly's spawn, wait, status and cancel tools, including after resuming a Melly chat. Chats created in 0.4 also give the lead a compact tool. It can compact an idle worker or schedule its own compaction after the turn, once per lead turn. Workers use an active slot while compacting and retain their last report. It is instructed to delegate substantial independent work only when the saved lead effort or parallel progress justifies another model's context cost; trivial greetings and tiny edits should stay with the lead. This judgment is model-driven. The harness enforces the limits:

- **Two automatic launches per user-started lead turn**, even if an earlier worker finishes. Steering does not reset the count.
- **Three active workers total**, including manual workers, workers starting, and workers still stopping.
- **A 15% remaining quota reserve** for automatic launches. Automatic launches wait briefly for a quota refresh and stay blocked if quota is still unavailable. Manual spawning remains available within the active-worker cap.
- Luna must be offered by the signed-in account. Workers receive bounded task briefs, not a copy of the lead conversation. Codex's own base instructions and tools still contribute context.
- Read-only workers cannot approve writes or network access. Editing workers declare non-overlapping owned paths; ownership coordinates agents, while Codex's configured sandbox remains the actual permission boundary.

Waiting is event-driven and does not require workers to send progress reports to the lead. The UI reads activity and usage directly from Codex events. Finished results arrive through a tool response, or accompany your next lead message if not yet collected; Melly does not start hidden lead turns. Stopping the lead also stops its automatic workers. Manual workers remain independently controllable.

These defaults cap launches and concurrency, not exact plan consumption. A running model can still consume usage until it completes or is stopped. Native Codex delegation is disabled only for Melly's server process so it cannot bypass the harness cap.

## Thinking, context and usage

User messages have an accent edge and a tinted background. Assistant replies put agent and model details below the text, with progress messages labeled separately. A spinner shows the selected agent’s live state. Thinking appears as an amber disclosure row. Melly prefers the reasoning summaries Codex supplies and falls back to readable reasoning-text events when available. Some turns or models expose neither; Melly then shows the live state and elapsed time. It cannot force the server to reveal hidden reasoning. Ctrl+T expands the supplied text.

Commands and skill-file reads are compact by default. A skill read may contain instructions because Codex chose to read that file; it is ordinary tool output. Ctrl+O reveals the full output.

Context is a compact **estimate from the last model-call token snapshot**, divided by the reported capacity. It is not a live tokenizer. It clears after compaction until a new snapshot arrives. Each agent's totals accumulate across calls; input, cached input, output and reasoning details are separate in `/usage`. Account percentages are shared across agents and come directly from Codex, with each available quota bucket retained. Melly does not infer plan consumption or prices from tokens.

The terminal draws on changes, ignores unrelated notifications, caches formatting and wrapping measurements per history entry, and renders only visible entries. The local UI cannot shorten server inference time. `/status` separates startup time, first-event latency and turn elapsed time.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 tests/terminal_smoke.py target/release/melly --features
python3 tests/team_terminal_smoke.py target/release/melly
```

The smoke tests use deterministic local App Server fixtures. They exercise model and effort selection, skills, thinking, compaction, interleaved agent activity, launch caps, reopening a workspace with settings and drafts, and resumed delegation and compaction without spending model usage. Opt-in live tests spend Codex usage in disposable repositories:

```sh
python3 tests/terminal_smoke.py target/release/melly --live
python3 tests/terminal_smoke.py target/release/melly --live --live-model gpt-6-astra
# See examples/agent_smoke.rs for the three-file multi-agent fixture.
cargo run --release --example agent_smoke -- /path/to/disposable/fixture
```

Set `MELLY_CODEX_BIN` to use a different Codex executable. To measure warmed renderer cost on a 400-entry transcript, run `cargo test --release cached_scrollback_render_timing -- --ignored --nocapture`. This measures local test-backend rendering, not model latency or terminal-emulator performance.

Tested on macOS with Codex CLI 0.167.0. Codex threads created outside Melly only expose the tools originally registered for that thread. Complete stock command parity and interactive server question forms remain future work. Unsupported server requests produce a visible notice and a protocol error rather than silently accepting them.

The native Rust interface takes inspiration from [OpenCode](https://github.com/anomalyco/opencode)'s centered start screen, highlighted prompts, assistant footers, and compact thinking and context presentation. See [the design notes](docs/subagents-workspace.md).
