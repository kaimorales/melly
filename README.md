# Melly

A fast Rust terminal workspace for Codex. Keep your coding conversation beside live activity, see thinking summaries as they arrive, and change models or reasoning strength from a keyboard command palette. Melly uses the official Codex App Server and your existing Codex sign-in.

## Run it

Install Rust and the [Codex CLI](https://learn.chatgpt.com/docs/codex/cli), then sign in with `codex login`.

```sh
cargo install --path . --locked
cd /path/to/your/project
melly
```

Melly discovers available models from Codex and prefers GPT-6.1 Sol when offered. Start with a specific model using `melly --model gpt-6-astra`, or change it with `/model`. Reopen a saved chat using `melly --resume THREAD_ID`; Codex preserves its original working directory. Find the ID under `/status`.

## Commands

Type `/` to open the palette. Type to filter, use Up/Down to select, Tab to complete, and Enter to apply. Esc closes the menu. For a literal message beginning with `/`, press Esc then Enter.

| Command | Action |
| --- | --- |
| `/model` | Pick from the models available to your account |
| `/effort` or `/strength` | Pick a reasoning effort supported by the selected model |
| `/skills` | Browse enabled Codex skills and attach one to the next message |
| `/compact` | Ask Codex to compact the current conversation while idle |
| `/context` | Inspect the latest context estimate and model capacity |
| `/usage` | Inspect session tokens and refresh account usage limits |
| `/status` | Inspect model settings, connection timing, directory and resume ID |
| `/thinking` | Expand or collapse completed thinking summaries |
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
| Ctrl+U | Clear the draft and attached skills |
| Page Up / Page Down | Scroll the focused pane |
| Shift+Enter | Newline, if supported by the terminal; multiline paste also works |
| Esc | Close a menu or return to the conversation |
| Ctrl+C | Interrupt a running turn, or quit when idle |
| Ctrl+Q | Quit |

The inspector sits beside the chat in wide terminals. Narrow terminals switch between panes. Typing brings the composer back into view.

When a supported command or patch needs approval, inspect the action, press `y` to select allow, then Enter to confirm. Press `n` to decline. Melly preserves the sandbox and approval policy configured in Codex.

## Thinking, context and usage

Thinking blocks display the reasoning summaries Codex supplies. Some turns or models provide no summary text; Melly still shows the live state and elapsed time. Ctrl+T collapses completed summaries without discarding them.

The context meter is an **estimate from the last model-call token snapshot**, divided by the reported capacity. It is not a live tokenizer. It clears after compaction until a new snapshot arrives. Session totals accumulate across calls; input, cached input, output and reasoning details are separate in `/usage`. Account percentages come directly from Codex, with each available quota bucket retained. Melly does not infer plan consumption or prices from tokens.

The terminal draws on changes, ignores unrelated notifications, caches formatting and wrapping measurements per history entry, and renders only visible entries. The local UI cannot shorten server inference time. `/status` separates startup time, first-event latency and turn elapsed time.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 tests/terminal_smoke.py target/release/melly --features
```

The smoke test uses a deterministic local App Server fixture. It exercises keyboard model and effort selection, skill attachment, thinking output and compaction without spending model usage. An opt-in live test runs one coding turn in a disposable repository:

```sh
python3 tests/terminal_smoke.py target/release/melly --live
python3 tests/terminal_smoke.py target/release/melly --live --live-model gpt-6-astra
```

Set `MELLY_CODEX_BIN` to use a different Codex executable. To measure warmed renderer cost on a 400-entry transcript, run `cargo test --release cached_scrollback_render_timing -- --ignored --nocapture`. This measures local test-backend rendering, not model latency or terminal-emulator performance.

Tested on macOS with Codex CLI 0.167.0. This release supports one lead session. Multiple visible workers, automatic delegation budgets, complete stock command parity, and interactive server question forms remain future work. Unsupported server requests produce a visible notice and a protocol error rather than silently accepting them.
