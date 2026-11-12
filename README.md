# Melly

A fast Rust terminal workspace for Codex. Talk to one conductor. It assigns edits and checks to Luna when useful, then reviews the result. Switch views to observe each agent’s activity and usage. Melly uses the official Codex App Server and your existing Codex sign-in.

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
| `/agents` | Open the agent switcher; Up/Down selects and Enter opens an observation view |
| `/model` | Pick from the models available to your account |
| `/effort` or `/strength` | Pick a reasoning effort supported by the selected model |
| `/skills` | Browse enabled Codex skills and attach one to the next message |
| `/compact` | Ask Codex to compact the conductor while idle |
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

Model and effort changes apply to the next turn. New chats start at the selected model's advertised default effort; resumed chats preserve their settings. The composer distinguishes active settings from queued settings while a turn runs. Sending a message during a turn steers it with its current settings. Skill selection attaches a typed skill input; opening a menu or inspecting usage does not make a model call.

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

The inspector sits beside the chat in wide terminals, with active agent tasks, selected-agent context and account quota. Completed agents collapse to a count; `/agents` keeps every agent accessible. Worker token totals remain visible when observing them. Narrow terminals switch between panes. The conductor keeps your draft. Luna views preserve their scroll position. Esc returns from a Luna view to the conductor. Typing brings the composer back into view.

When a supported command or patch needs approval, inspect the action, press `y` to select allow, then Enter to confirm. Press `n` to decline. Melly preserves the sandbox and approval policy configured in Codex.

## Delegation

The conductor is the only conversation you type into. It answers simple questions directly. For coding changes, it inspects relevant code and tests, identifies the evidence, and chooses an approach before giving Luna the findings, concrete implementation steps, scoped paths, behavior to preserve, and acceptance checks. User-facing progress stays brief. Luna owns implementation and the test loop. When Luna finishes, the conductor reviews the working-tree diff and the command exit codes captured by Melly. It can ask the same worker for a focused correction, then review that correction. The conductor runs in Codex's read-only sandbox. Editing workers retain their own configured sandbox and approval policy, and ownership is a coordination rule. The conductor’s read-only role does not make the workspace read-only for Luna. Explicit worker role instructions apply to new and resumed threads and override misleading permission descriptions in a conductor brief; actual server restrictions remain authoritative.

The conductor can spawn, wait for, inspect, cancel, and compact workers. New 0.5 chats also register the `revise` tool. Older Melly chats retain their original tools, so the conductor can use a fresh bounded follow-up if needed. Worker views are for observation and any configured approval prompts. Ctrl+C stops the conductor and its active delegated workers.

- **Two worker turns per user-started conductor turn.** Each spawn or revision spends one slot. Steering does not reset the budget.
- **Three active workers total**, including workers starting, stopping or compacting.
- **A 15% remaining quota reserve.** Delegation waits briefly for a quota refresh and stays blocked if quota is unavailable.
- Luna must be offered by the signed-in account. It receives a bounded brief with relevant context and keeps its context for a revision.

The conductor receives results and up to 12 observed command exit codes through the wait or status tool. These describe the commands that ran. The conductor judges whether those commands covered the requested behavior. Waiting is event-driven and starts no hidden conductor turns.

## Thinking, context and usage

User messages have an accent edge and a tinted background. Final assistant replies put one quiet agent and model footer below the text. Consecutive progress, thinking and tools share a collapsed activity row instead of repeating role labels. A spinner shows the selected agent’s live state. Thinking appears as an amber disclosure row. Melly prefers the reasoning summaries Codex supplies and falls back to readable reasoning-text events when available. Some turns or models expose neither; Melly then shows the live state; turn timing is available in `/status`. It cannot force the server to reveal hidden reasoning. Ctrl+T expands the supplied text.

Commands and skill-file reads stay inside the chat’s collapsed activity by default. The sidebar never becomes a shell transcript. A skill read may contain instructions because Codex chose to read that file; it is ordinary tool output. Ctrl+O or `/activity` reveals the full output inline. Ctrl+T expands reasoning summaries with inline Markdown formatting.

Context is a compact **estimate from the last model-call token snapshot**, divided by the reported capacity. It is not a live tokenizer. It clears after compaction until a new snapshot arrives. Each agent's totals accumulate across calls; input, cached input, output and reasoning details are separate in `/usage`. Account percentages are shared across agents and come directly from Codex, with each available quota bucket retained. Melly does not infer plan consumption or prices from tokens.

The terminal draws on changes, ignores unrelated notifications, caches formatting and wrapping measurements per message or activity group, and renders only visible entries. The local UI cannot shorten server inference time. `/status` separates startup time, first-event latency and turn elapsed time.

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
