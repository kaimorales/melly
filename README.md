# Melly

Melly is a Rust terminal workspace for Codex. The first milestone opens a real coding session in the current directory with a lead chat and a live activity pane for commands, file changes, and reported tokens.

## Run it

Install Rust and the [Codex CLI](https://learn.chatgpt.com/docs/codex/cli), then sign in with `codex login`.

~~~sh
cargo install --path . --locked
cd /path/to/your/project
melly
~~~

Melly asks Codex for available models and prefers GPT-6.1 Sol when it is offered. Set the model at launch with `melly --model gpt-6-luna`. Use the thread ID from the activity pane to reopen a saved chat with `melly --resume THREAD_ID`. Codex keeps the original working directory when resuming.

| Key | Action |
| --- | --- |
| Enter | Send a prompt or steer an active turn |
| Tab | Focus the activity pane or switch views in a narrow terminal |
| Page Up / Page Down | Scroll the focused pane |
| Esc | Return to the lead chat |
| Ctrl+C | Interrupt a running turn, or quit when idle |
| Ctrl+Q | Quit |

You can paste multiline prompts. When a supported command or patch needs approval, read the action details, press `y` to select allow, then press Enter to confirm. Press `n` to decline. Melly uses the sandbox and approval policy configured in Codex.

## Development

~~~sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
python3 tests/terminal_smoke.py target/debug/melly
~~~

The terminal smoke test uses a local fixture by default. Add `--live` to run one signed-in Luna turn against a disposable Git repository. Set `MELLY_CODEX_BIN` to point to a different Codex executable.

The first milestone targets macOS and Codex CLI 0.167.0. The activity pane shows reported total tokens and context capacity when available. The next milestones add context occupancy and compaction, the command and skill palette, account quota, and multiple live workers.
