# Melly

A quiet, fast Mac app for cloud conversations. Tauri uses the system WebKit view; Rust owns streaming, cancellation, SQLite history/search, provider protocols, tool permissions, and Keychain credentials. No bundled Chromium, Node server, local inference, coding host, or model downloads run in the app.

## Run

On this Mac, open the local build:

```sh
open target/release/bundle/macos/Melly.app
```

From a fresh checkout, install Node 22.12+, stable Rust and Xcode Command Line Tools, then `npm ci`. Project scripts find the installed Rust toolchain without changing your shell.

```sh
npm run dev       # native Mac window + frontend hot reload
npm run build     # optimized, locally signed .app
npm run launch    # open the built app
npm run check     # TypeScript, Rust formatting and Clippy
npm test          # Rust + browser UI suites
```

Install the UI test browser once with `npx playwright install chromium`. `npm run web` is an explicitly labeled browser mock preview; it is not the desktop backend. `npm run test:core` runs the credential-free Rust suite alone. `npm run perf` runs a synthetic 10,000-message SQLite probe.

## Try it without credentials

The first launch is empty and selects **Demo · Mock**. It makes no network requests. Click **Try an example** to add synthetic Job search chats, or create the folder and chats yourself:

1. Create **Job search**. In a chat named **My experience**, send “My transit project improved forecast accuracy by 18%.”
2. Create **Interview prep** in the same folder. Send “Look in my other Job search chats for the project result.” The composer visibly enables folder search for this turn. The reply receives only matching snippets; click **My experience** beneath it to open the exact source message.
3. Send **Demonstrate streaming and Stop**, then click Stop. That exact phrase (or **Write a longer answer**) selects a deliberately paced synthetic fixture so cancellation can be demonstrated. Ordinary Mock and every live route have no artificial token delay. Retry replaces the last assistant reply without duplicating the user message.
4. Enable **Search the web** and ask about transit planning. Mock returns a clearly synthetic public source. Live search requires a supported native provider tool or a separately configured Tavily key.
5. Open Settings. Try System, Light, Dark, text size, Enter/⌘ Enter, reduced motion, and your system instructions. Provider connections and model IDs are configured in their own small panels.

Shortcuts: ⌘N new chat, ⌘K chat search, ⌘, settings. Shift-Enter inserts a newline. The sidebar collapses; long histories load in pages.

Drag chats onto a folder to move them, hover to open a collapsed folder, or drop onto Recent to unfile them. Every chat has an ellipsis/right-click menu for rename and confirmed deletion. The header's chat menu retains Move to folder for keyboard access. Fenced code has theme-aware syntax colors, a language label and exact-copy action; its highlighter loads only when needed and runs off the UI thread.

Replies render inline and display LaTeX with `$…$`, `$$…$$`, `\(...\)` and `\[…\]`. Equations inherit Light/Dark appearance and include accessible MathML. The renderer and fonts load locally only when needed, with bounded work off the UI thread. Code, ordinary prices and the original copied response stay intact; unsupported or incomplete equations remain readable as source. See [math rendering notes](docs/design/math.md).

See the [native screenshot walkthrough](docs/demo.md) and [verification record](docs/verification.md). The signed local release is 12.78 MiB; its latest measured process-to-render launch on this Mac took 533 ms. This single warm-cache observation excludes cloud latency and the OS launch request.

## Accounts and billing

**ChatGPT plan replies have been verified live; other provider routes remain fixture-tested.** The app ships with no credentials and never imports tokens or history from another application. A saved key is configuration, not proof of live access.

- **ChatGPT:** Continue with ChatGPT opens official app-owned authorization and requests plan-use consent. Native verification passed for sign-in, account model discovery, multi-turn replies, Stop, folder-tool continuation and use after relaunch. Built-in web search with a citation also passed on `gpt-6.1-sol`, without an external search key. Renewal, revocation, usage-limit failures and other account/model combinations remain pending. No API-key fallback exists on this route.
- **Claude, Gemini, Grok, OpenRouter, DeepSeek, Kimi/Moonshot:** API-key routes. Kimi uses Moonshot's general API, not Kimi Code membership.
- **QwenCloud:** Standard API and Token Plan are separate Singapore endpoints, separate credentials, and explicit billing routes. A plan error cannot switch to metered Standard API.
- **Custom:** Public HTTPS OpenAI-compatible base URL, API key, and manual model ID. Text streaming only; unknown tool capabilities are disabled.
- **Meta Muse Code:** Visible but disabled because chat-only host restrictions have not been verified. No coding host is started.

Keys are entered in a native secure dialog and saved in macOS Keychain. The webview receives only success/cancel and connection state. OAuth refresh tokens also stay in Keychain. Melly never asks for your Mac password: inaccessible credentials fail without opening authentication UI, and fresh ChatGPT sign-in avoids protected records from earlier development builds. Provider credentials never go to web search, another provider, or an HTTP redirect.

## Data and boundaries

Chats and preferences live at `~/Library/Application Support/com.melly.desktop/melly.sqlite3`. The containing directory is mode 700 and database mode 600; transcripts are local SQLite, not separately encrypted. Credentials use Keychain service `app.melly.desktop.credentials`.

A folder is organization, not automatic memory. Folder retrieval needs permission on each turn. Rust derives the current folder from the database and rejects model-supplied folder IDs. It excludes the current chat and all other folders, returns at most five 600-character snippets, and identifies their exact messages. Changing or deleting an active chat is blocked until Stop completes. Retrieved chats and web results are untrusted references, never tool authorization.

Startup does no network discovery. SQLite uses WAL, FTS5, and indexed timestamps. The UI batches streamed text per animation frame and bounds rendered history. HTTP connection setup and reads are cancellable; an interrupted stream is never recorded as completed. Token renewal already started is allowed to save its rotated result even when the chat is stopped.

## For future agents

- [Architecture](docs/architecture.md), [shared IPC/types](docs/contracts.md), [data/tool schemas](docs/data-and-tools.md)
- [Provider endpoints and protocols](docs/providers.md), [authentication and credentials](docs/auth.md)
- [Verification and live checks](docs/verification.md), [source and artwork notices](docs/NOTICE.md)
- Fixtures: `tests/fixtures/providers/`; Rust tests: `crates/melly-core/tests/`; UI tests: `tests/ui/`.

No real account data or conversations belong in fixtures, screenshots, logs, or bug reports. Never add credential fallback, arbitrary URL-fetch tools, shell tools, or inferred cross-folder memory.

## Previous CLI

The original CLI checkout and Git history were preserved before the user-authorized empty-main reset. Local recovery artifacts are at `/Users/kaimorales/.codex/backups/melly-before-desktop-2026-11-13/`: a verified all-ref Git bundle, full working-tree archive, status and patch. Main's empty-tree reset is `4e9c5593de0f2046253490c132b29f55782eadd2`. The desktop work is committed and pushed to `main` in feature-sized checkpoints, as requested; no force push or release publication is used.
