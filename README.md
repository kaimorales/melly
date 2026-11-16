# Melly

A quiet, fast Mac app for local and cloud conversations. Tauri uses the system WebKit view; Rust owns streaming, cancellation, SQLite history/search, provider protocols, tool permissions, and Keychain credentials. Local inference runs in an explicitly configured local server, including OpenAI-compatible engines and Ollama. Melly does not bundle Chromium, a Node server, a model runtime, a coding host, or model downloads.

## Run

For a single installation that subsequent updates replace, quit Melly and run `npm run install:local` from this checkout. This builds and validates the app, replaces `/Applications/Melly.app`, and removes the temporary build copy after success. The existing profile and Keychain stay in place. Open the installed app:

```sh
open /Applications/Melly.app
```

From a fresh checkout, install Node 22.12+, stable Rust and Xcode Command Line Tools, then `npm ci`. Project scripts find the installed Rust toolchain without changing your shell.

```sh
npm run dev       # native Mac window + frontend hot reload
npm run build     # optimized, locally signed .app
npm run install:local # build and replace the one installed app
npm run launch    # open installed app, or build artifact before first install
npm run check     # TypeScript, Rust formatting and Clippy
npm test          # Rust + browser UI suites
```

Install the UI test browser once with `npx playwright install chromium`. `npm run web` is an explicitly labeled browser mock preview; it is not the desktop backend. `npm run test:core` runs the credential-free Rust suite alone. `npm run perf` runs a synthetic 10,000-message SQLite probe.

The bundled CLI supports `melly`, `--help`, `--version`, `--update`, and `--upgrade`. Both update aliases delegate to Homebrew for a Homebrew-managed install; source builds give the local update instructions. The Homebrew cask template and checksum generator are prepared, but the tap and releases are not published. See [installation and updates](docs/updates.md). The local installer links `~/.local/bin/melly`; Homebrew will link the command in its own prefix.

## Projects and local models

Create a project from the sidebar. Its menu opens **Project files**, where **Add file** imports a managed copy of text, Markdown or a PDF with selectable text. **Replace** updates an imported file and its index; **Remove** moves its managed copy to Trash. The original selected file stays in place. **Show in Finder** opens the real project directory. **Link folder** instead reads an existing folder without copying or changing it; selected text files refresh when used; **Refresh files** rescans the linked tree.

Enable **Project context** in the composer to remember permission across that project's chats. Matching file and past-chat excerpts can then accompany messages to the selected provider. The composer identifies the recipient, and source links open the cited passage or PDF page. Text extraction and retrieval run locally. On-device OCR supports PNG/JPEG images and up to 20 scanned PDF pages; native tests verified both a synthetic image and a scanned PDF through a live Ollama reply. Images currently contribute recognized text, not pixels sent to a vision model. Optional on-device sentence embeddings improve file ranking when Apple’s English language asset is already available; keyword retrieval remains available without that asset.

**Archive project** hides its chats and files and can be undone from **Archived projects**. **Delete project** requires confirmation, permanently removes its chats, and moves only Melly-managed project files to Finder Trash. Linked directories stay untouched. Interrupted deletion can be retried from Archived projects.

For local replies, run Ollama separately, install a text model there, and configure **Ollama Local** in Settings. Refresh models or enter an installed model's name. No API key is used. Switching providers in a chat preserves the conversation; the local request includes bounded recent history. Saved configuration is not a live verification. Ollama Cloud remains a separate connection.

## Files, conversation continuity and usage

Attach text, Markdown, PDFs, or recognized image text with **+**, or drop up to eight files into an idle chat. Melly reads files locally; selected excerpts go to the chosen provider only when you send. Files stay attached to the saved message, including after Retry or branching. Native picker imports and OCR were verified in the app; the Finder-to-chat drop gesture still needs a direct native check. Drop tickets, import batches, ownership and cleanup have automated coverage.

**Branch** copies a conversation through the selected message. **Edit** prepares a revised message in a new branch, preserving the original. Context is bounded for the selected model, with reply space reserved; the same chat can move between local and cloud connections. Relevant excerpts from earlier messages in that same chat can be retrieved beyond the recent history window. This does not create permission to search other projects.

The small ring beside the model selector opens **Context**: approximate input size, capacity when reported, included/omitted messages, selected references, and provider-reported token totals for that chat. Unknown capacity has no fabricated percentage.

Each connection’s Settings panel has **Usage & costs**. It shows this month’s usage recorded in Melly, reported charges where available, and clearly separated estimates from optional per-model rates. Incomplete usage and unpriced requests remain marked unknown. OpenRouter has an explicit refresh for the current API key’s usage and configured budget. Subscription allowance is unavailable unless exposed by the provider; token totals cannot stand in for a subscription percentage.

Use a chat’s menu to export Markdown, **Project files → Export project** for a portable project, and **Settings → Your data** for backup/restore. Backups contain readable chats, managed files and attachments, but exclude credentials, linked directories and billing records. Restore adds new identities alongside existing data, with project context off.

## Coding and continuity

In a linked project chat, enable Project context and ask for a change. Search, read, create, edit, delete and discovered test/build/lint commands stay in chat. **Ask** reviews mutations and checks; **Auto-apply** grants the current chat and linked folder permission for the app session. Completed operations have a compact diff and **Undo**. Undo refuses to overwrite later edits. Checks use a bounded private copy with installed dependencies, no network and a five-minute timeout.

Drafts save locally per chat, including attachment selections. **Continue** resumes interrupted coding without deleting earlier work. The context ring offers **Compact context**, retained task notes and **Undo compaction**; original messages remain available. Sources and detailed check output expand only when wanted.

## Browser preview without credentials

The browser preview starts empty and selects **Demo · Mock**. The native desktop starts with provider setup. It makes no network requests. Click **Try an example** to add synthetic Job search chats, or create the project and chats yourself:

1. Create **Job search**. In a chat named **My experience**, send “My transit project improved forecast accuracy by 18%.”
2. Create **Interview prep** in the same project. Send “Look in my other Job search chats for the project result.” The composer visibly enables project context for this turn. The reply receives only matching snippets; click **My experience** beneath it to open the exact source message.
3. Send **Demonstrate streaming and Stop**, then click Stop. That exact phrase (or **Write a longer answer**) selects a deliberately paced synthetic fixture so cancellation can be demonstrated. Ordinary Mock and every live route have no artificial token delay. Ordinary Retry replaces the latest assistant attempt. Interrupted coding exposes Continue and retains earlier actions and undo records.
4. Enable **Search the web** and ask about transit planning. Mock returns a clearly synthetic public source. Live search requires a supported native provider tool or a separately configured Tavily key.
5. Open Settings. Try System, Light, Dark, text size, Enter/⌘ Enter, reduced motion, and your system instructions. Provider connections and model IDs are configured in their own small panels.

Shortcuts: ⌘N new chat, ⌘K chat search, ⌘, settings. Shift-Enter inserts a newline. The sidebar collapses; long histories load in pages.

Drag chats onto a project to move them, hover to open a collapsed project, or drop onto Recent to unfile them. Every chat has an ellipsis/right-click menu for rename and confirmed deletion. The header's chat menu retains Move to project for keyboard access. Fenced code has theme-aware syntax colors, a language label and exact-copy action; its highlighter loads only when needed and runs off the UI thread.

Replies render inline and display LaTeX with `$…$`, `$$…$$`, `\(...\)` and `\[…\]`. Equations inherit Light/Dark appearance and include accessible MathML. The renderer and fonts load locally only when needed, with bounded work off the UI thread. Code, ordinary prices and the original copied response stay intact; unsupported or incomplete equations remain readable as source. See [math rendering notes](docs/design/math.md).

See the [native screenshot walkthrough](docs/demo.md) and [verification record](docs/verification.md). An earlier build measured 533 ms from process entry to rendered UI on this Mac. This single warm-cache observation excludes cloud latency and the OS launch request; the new project features have not been benchmarked. See the verification record for the current build and test limits.

## Accounts and billing

**ChatGPT plan replies and Ollama Local have been verified live; other cloud routes remain fixture-tested.** The app ships with no credentials and never imports tokens or history from another application. A saved key is configuration, not proof of live access.

- **ChatGPT:** Continue with ChatGPT opens official app-owned authorization and requests plan-use consent. Native verification passed for sign-in, account model discovery, multi-turn replies, Stop, folder-tool continuation and use after relaunch. Built-in web search with a citation also passed on `gpt-6.1-sol`, without an external search key. Renewal, revocation, usage-limit failures and other account/model combinations remain pending. No API-key fallback exists on this route.
- **Claude, Gemini, Grok, OpenRouter, DeepSeek, Kimi/Moonshot:** API-key routes. Kimi uses Moonshot's general API, not Kimi Code membership.
- **QwenCloud:** Standard API and Token Plan are separate Singapore endpoints, separate credentials, and explicit billing routes. A plan error cannot switch to metered Standard API.
- **Custom connections:** Public HTTPS or loopback OpenAI-compatible servers, endpoint-bound credentials, and discovered or manual model IDs. Native function calling is the default; **Compatibility → Tool calls → Text fallback** supports servers without it.
- **Meta Model API:** An explicitly metered API-key connection. It uses text chat and optional separately billed web search; no Muse Code host is started.

Provider keys are entered in native secure fields; the web-search key uses a native secure dialog. Keys are saved in macOS Keychain. The webview receives only success/cancel and connection state. OAuth refresh tokens also stay in Keychain. Melly never asks for your Mac password: inaccessible credentials fail without opening authentication UI, and fresh ChatGPT sign-in avoids protected records from earlier development builds. Provider credentials never go to web search, another provider, or an HTTP redirect.

## Data and boundaries

Chats and preferences live at `~/Library/Application Support/com.melly.desktop/melly.sqlite3`. The containing directory is mode 700 and database mode 600; transcripts are local SQLite, not separately encrypted. Credentials use Keychain service `app.melly.desktop.credentials`.

Project context is opt-in and remembered per project; the composer captures the effective permission for each turn. Rust derives scope from the current chat and rejects model-supplied project IDs. Project retrieval excludes other projects, returns bounded file and sibling-chat excerpts, and retains source citations. Earlier-message retrieval is separately confined to the current conversation. Changing or deleting an active chat or project is blocked until Stop completes. Retrieved files, chats and web results are untrusted references. Linked-project coding uses scoped search, reads, writes, deletes, and isolated checks with per-chat Ask or Auto-apply. It never exposes arbitrary shell execution.

Startup does no network discovery. SQLite uses WAL, FTS5, and indexed timestamps. The UI batches streamed text per animation frame and bounds rendered history. HTTP connection setup and reads are cancellable; an interrupted stream is never recorded as completed. Token renewal already started is allowed to save its rotated result even when the chat is stopped.

## For future agents

- [Architecture](docs/architecture.md), [shared IPC/types](docs/contracts.md), [data/tool schemas](docs/data-and-tools.md)
- [Provider endpoints and protocols](docs/providers.md), [authentication and credentials](docs/auth.md)
- [Verification and live checks](docs/verification.md), [source and artwork notices](docs/NOTICE.md)
- Fixtures: `tests/fixtures/providers/`; Rust tests: `crates/melly-core/tests/`; UI tests: `tests/ui/`.

No real account data or conversations belong in fixtures, screenshots, logs, or bug reports. Never add credential fallback, arbitrary URL-fetch tools, shell tools, or inferred cross-folder memory.

## Development checks

macOS CI runs type/format/lint checks, synthetic Rust and browser suites, an app build, and bundle verification. Live provider fixtures are opt-in and use disposable data. `npm run verify:app -- --distribution` additionally requires Developer ID, a stapled notarization ticket and Gatekeeper acceptance. See [updates](docs/updates.md) for release prerequisites.
