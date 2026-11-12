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
| `/sessions` | Search saved sessions; Enter resumes the full team, Tab toggles this project / all projects |
| `/new` | Start a fresh conductor session in the current project when the team is idle |
| `/agents` | Open the agent switcher; Up/Down selects and Enter opens an observation view |
| `/review` | Open the latest worker’s brief and review |
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
| Ctrl+S | Open or close session history from any agent view |
| Ctrl+G | Open or close the agent switcher from any agent view |
| Ctrl+R | Open or close the selected worker’s brief and review; from conductor, open the latest worker |
| Ctrl+T | Expand or collapse completed thinking summaries |
| Ctrl+O | Expand or collapse full tool output |
| Alt+1 … Alt+9 | Jump directly to an agent; Alt+1 opens the lead |
| Ctrl+N / Ctrl+P | Next / previous agent (portable fallback) |
| Ctrl+U | Clear the draft and attached skills |
| Page Up / Page Down | Scroll the focused pane |
| Mouse wheel / trackpad | Scroll the chat or inspector beneath the pointer |
| Ctrl+Home / Ctrl+End | Jump to the oldest loaded content / latest content in the focused pane |
| Shift+Enter | Newline, if supported by the terminal; multiline paste also works |
| Esc | Close a menu or return to the conversation |
| Ctrl+C | Interrupt a running turn, or quit when idle |
| Ctrl+Q | Quit |

The inspector sits beside the chat in wide terminals, with each active agent's task, current command or file edit, elapsed turn time, selected-agent context and account quota. The conductor's waiting state comes from the harness. Workers awaiting review, requesting changes or interrupted stay visible; accepted workers collapse into the remaining-agent count. Short terminals prioritize the selected agent and active work. `/agents` or Ctrl+G keeps every agent accessible, including its review state and measured token total. Worker token totals remain visible when observing them. Narrow terminals switch between panes. The conductor keeps your draft. Luna views preserve their scroll position. Esc returns from a Luna view to the conductor. Typing brings the composer back into view.

Scrolling up anchors the visible message while new output streams or older history loads. Scrolling back to the bottom, Ctrl+End, or sending a message restores following the latest output. Each agent has its own position. The quiet “History · Ctrl+End latest” hint appears only while reading earlier messages. Wheel input does not change the draft, send prompts, or scroll behind an open picker.

Wheel and trackpad gestures ease across terminal rows for at most 100 ms after the last event; reversing direction cancels unfinished movement. Keyboard navigation stays immediate, and disabling Animations in `/settings` also makes wheel scrolling immediate. A faint scrollbar appears in the chat's existing right gutter when the loaded transcript exceeds the viewport. It indicates your position within the loaded messages without changing wrapping.

Replies are plain text. The full model name and reasoning effort appear once in the composer, with active and queued settings distinguished during a turn. The conductor label stays in the agent navigation instead of repeating below every reply. Muted keyboard hints sit outside the composer with a blank row of separation, and narrow terminals omit hints that cannot fit.

## Session history

Press **Ctrl+S** or use `/sessions`. History starts in the current project; Tab includes all projects. Type to search titles, project paths or thread IDs, use arrows to select, and Enter to resume. Each row shows its project, last saved time and Luna count. Ctrl+N inside history starts a new session without discarding the previous draft; `/new` is also available from the conductor. Esc returns to the current view.

Melly names a session locally from its first submitted message and shows the title in the chat header. Older saved sessions without titles appear as “Untitled session” with their project and ID; their next submitted message supplies a title. Empty launches are hidden unless they contain a draft or agents. The picker reads Melly's saved workspaces, not unrelated stock Codex chats.

Switching saves the current session first and restores the destination's full team, selected agent, conductor draft, briefs and recorded reviews. It uses the same Codex resume flow as `--continue`, including automatic delegation. You can browse while a team runs, but switching waits until idle; Esc then Ctrl+C interrupts the current team if needed. Approval prompts take priority. A missing workspace, locked session or immediate connection-launch failure keeps the current workspace open. A closed Melly process does not keep workers running in the background.

Searching and viewing agents make no model calls. History is loaded only when opened, and skips saved command output during metadata parsing. Live activity comes from existing Codex events; it does not ask agents to send extra status reports. Briefs, full output and review evidence remain in Ctrl+R and Ctrl+O rather than filling the sidebar.

When a supported command or patch needs approval, inspect the action, press `y` to select allow, then Enter to confirm. Press `n` to decline. Melly preserves the sandbox and approval policy configured in Codex.

## Delegation

The conductor is the only conversation you type into. It answers simple questions directly. For coding changes, it inspects relevant code and tests, identifies the evidence, and chooses an approach before giving Luna the findings, concrete implementation steps, scoped paths, behavior to preserve, and acceptance checks. User-facing progress stays brief. Luna owns implementation and the test loop. When Luna finishes, the conductor reviews the working-tree diff and the command exit codes captured by Melly. It can ask the same worker for a focused correction, then review that correction. The conductor runs in Codex's read-only sandbox. Editing workers retain their own configured sandbox and approval policy, and ownership is a coordination rule. The conductor’s read-only role does not make the workspace read-only for Luna. Explicit worker role instructions apply to new and resumed threads and override misleading permission descriptions in a conductor brief; actual server restrictions remain authoritative.

The conductor can spawn, wait for, inspect, cancel, and compact workers. New 0.5 chats also register the `revise` tool. Older Melly chats retain their original tools, so the conductor can use a fresh bounded follow-up if needed. Worker views are for observation and any configured approval prompts. Ctrl+C stops the conductor and its active delegated workers.

- **Two worker turns per user-started conductor turn.** Each spawn or revision spends one slot. Steering does not reset the budget.
- **Three active workers total**, including workers starting, stopping or compacting.
- **A 15% remaining quota reserve.** Delegation waits briefly for a quota refresh and stays blocked if quota is unavailable.
- Luna must be offered by the signed-in account. It receives a bounded brief with relevant context and keeps its context for a revision.

The conductor receives results and up to 12 observed command exit codes through the wait or status tool. These describe the commands that ran. The conductor judges whether those commands covered the requested behavior. Waiting is event-driven and starts no hidden conductor turns.

## Brief and review

Select a worker and press **Ctrl+R**, or use `/review` from the conductor. A focused sheet keeps the main chat uncluttered:

- **Brief:** exact objective, findings/approach, owned paths, expected checks and delegation reason. Press `p` for the complete worker prompt, including the harness wrapper. Follow-up prompts are preserved verbatim.
- **Review:** the conductor’s explicit verdict and summary, observed conductor commands after report delivery, Luna’s commands and final report. Press `o` to expand recorded output, including diff excerpts. Exit codes are actual command outcomes, not a guarantee that the checks cover the task.
- **Navigation:** Tab or left/right switches tabs, arrows/Page Up/Down scroll, Ctrl+N/P changes workers, and Esc returns to your chat. The conductor draft is preserved; typing and paste in the sheet never send a message. Approval prompts take precedence.

A small lifecycle rail animates while Luna runs and respects the motion setting. Completion and report delivery never create an acceptance verdict. Only the conductor’s `review` tool records an assessment, and a new worker attempt clears the prior verdict. The verdict is the conductor’s assertion; the observed commands are separate evidence, not an automatic correctness check.

Handoffs and evidence survive reopening Melly. To keep local storage bounded, it retains the original prompt, the latest 32 exact follow-up prompts (with an omitted count for older ones), and the latest 12 recorded command outcomes for each source, newest first. Command labels are capped at 240 characters and output excerpts at 4,000 characters, with an ellipsis when clipped. Output excerpts stay local and are not added to wait/status tool responses. Missing final output is labeled unavailable.

**Start a new Melly chat for explicit review verdicts.** New 0.7 chats register the `review` tool. Existing Codex chats keep their original tool schemas when resumed. Older handoffs without metadata show unknown rather than implying that no review occurred. After restart, new conductor command evidence starts from a fresh report-delivery boundary; an older saved verdict and its evidence remain visible.

## Thinking, context and usage

The native interface uses a shared graphite palette, aligned message/composer gutters and a composer that grows with multiline drafts. A custom lowercase pixel wordmark reveals once on the start screen. A small synchronized pulse shows live work; idle screens stop animating. `/settings` → Animations disables the reveal and uses static indicators. The effect is rendered with ordinary terminal cells, without images, additional dependencies, model calls or delayed input. See [the visual design notes](DESIGN.md).

User messages have an accent edge and a tinted background. Final assistant replies put one quiet agent and model footer below the text. Consecutive progress, thinking and tools share a collapsed activity row instead of repeating role labels. A spinner shows the selected agent’s live state. Thinking appears as an amber disclosure row. Melly prefers the reasoning summaries Codex supplies and falls back to readable reasoning-text events when available. Some turns or models expose neither; Melly then shows the live state; turn timing is available in `/status`. It cannot force the server to reveal hidden reasoning. Ctrl+T expands the supplied text.

Commands and skill-file reads stay inside the chat’s collapsed activity by default. The sidebar never becomes a shell transcript. A skill read may contain instructions because Codex chose to read that file; it is ordinary tool output. Ctrl+O or `/activity` reveals the full output inline. Ctrl+T expands reasoning summaries with inline Markdown formatting.

Context is a compact **estimate from the last model-call token snapshot**, divided by the reported capacity. It is not a live tokenizer. It clears after compaction until a new snapshot arrives. Each agent's totals accumulate across calls; input, cached input, output and reasoning details are separate in `/usage`. Account percentages are shared across agents and come directly from Codex, with each available quota bucket retained. Melly does not infer plan consumption or prices from tokens.

Melly limits the automatic skill catalog to 3,000 context tokens for the conductor and 1,200 for each worker. Workers skip personal memory injection because their scoped brief supplies the task context. Skills remain available through `/skills`, and the conductor retains its normal memory setting. These limits apply to new and resumed Melly threads; they do not change your global Codex configuration. See [token-efficiency notes](docs/token-efficiency.md) for a measured example and its limits.

The terminal draws on changes, ignores unrelated notifications, caches formatting and wrapping measurements per message or activity group, and renders only visible entries. Active visual animation is capped at 20 frames per second and stops after the home reveal or when work becomes idle. The local UI cannot shorten server inference time. `/status` separates startup time, first-event latency and turn elapsed time.

For a single assignment, the conductor can request the finished report directly from `spawn` with `wait_for_result: true`. Melly holds the tool reply while activity and approvals continue streaming, avoiding a separate conductor call just to wait. Parallel work can still launch immediately and wait later. The held call returns status after 120 seconds if work continues. Start a new Melly chat to expose this optional field; resumed older chats retain their original tools and use the existing launch/wait flow. See [speed measurements](docs/speed.md).

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
python3 tests/terminal_smoke.py target/release/melly --features
python3 tests/team_terminal_smoke.py target/release/melly
```

The smoke tests use deterministic local App Server fixtures. They exercise model and effort selection, skills, thinking, compaction, interleaved agent activity, launch caps, reopening a workspace with settings and drafts, resumed delegation and compaction, searchable history, switching full teams, busy protection and fresh sessions without spending model usage. Opt-in live tests spend Codex usage in disposable repositories:

```sh
python3 tests/terminal_smoke.py target/release/melly --live
python3 tests/terminal_smoke.py target/release/melly --live --live-model gpt-6-astra
# See examples/agent_smoke.rs for the three-file multi-agent fixture.
cargo run --release --example agent_smoke -- /path/to/disposable/fixture
```

Set `MELLY_CODEX_BIN` to use a different Codex executable. To measure warmed renderer cost on a 400-entry transcript, run `cargo test --release cached_scrollback_render_timing -- --ignored --nocapture`. This measures local test-backend rendering, not model latency or terminal-emulator performance.

Tested on macOS with Codex CLI 0.167.0. Codex threads created outside Melly only expose the tools originally registered for that thread. Complete stock command parity and interactive server question forms remain future work. Unsupported server requests produce a visible notice and a protocol error rather than silently accepting them.

The native Rust interface takes inspiration from [OpenCode](https://github.com/anomalyco/opencode)'s centered start screen, highlighted prompts, assistant footers, and compact thinking and context presentation. See [the design notes](docs/subagents-workspace.md).
