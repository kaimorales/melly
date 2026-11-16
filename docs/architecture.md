# Architecture

Melly is a native Tauri Mac application. Rust is the trust boundary and owns all expensive/stateful work. The system WKWebView renders a React view. Model inference uses an explicitly selected cloud connection or an external Ollama server on this Mac.

`melly-core` is independent of the desktop shell so storage, tool scope, network protocols and cancellation can be tested quickly without launching a window. `models.rs` defines serializable data; `storage.rs` owns SQLite WAL, FTS5 and transactions; `engine.rs` owns turn execution, bounded context, read-only tool dispatch and persistence. The provider modules normalize SSE into typed events; credentials/auth use Keychain only. The Tauri shell exposes the minimal command contract in contracts.md.

Launch reads bounded local state, creates the window, and never waits for network or model discovery. Message history pages are bounded; text search uses a local FTS index. Streamed deltas render in animation-frame batches; final/partial turns are committed once. Cancel tokens drop HTTP requests promptly. There is no polling timer while idle.

The provider HTTP client is reused across turns, keeping origin-scoped connections and TLS sessions warm. Credentials are attached to individual requests, never client-wide defaults. Redirects, environment proxies and cookies remain disabled; Cloud connections use the public-address-only resolver. A separate fixed-loopback client handles Ollama Local.

Each connection has one provider identity and one endpoint/billing route. Endpoint changes cannot reuse another connection's key. Subscription errors remain on that route. Provider keys are entered in a native secure field embedded in settings; the search key uses a native secure dialog. Neither passes through JavaScript. OAuth belongs to this app; it does not read other applications' credentials or ChatGPT history.

The desktop exposes only real provider connections. If the saved selection was the internal mock or is no longer connected, it asks the user to choose or connect a provider; it does not select a paid route automatically. The mock and synthetic examples remain available in the browser preview and automated tests.

Folder tools accept a query only, never an authorization scope. Rust derives the folder from the active chat and returns at most five bounded snippets from other chats in that folder. The model receives bounded references with source IDs. Project context can also supply relevant file and chat excerpts before generation for providers without tool calling. Tool results are explicitly untrusted, and the only actions are local search and a fixed read-only web search API. Web citation navigation is separate and validates public HTTPS URLs.

For Meta Model API, Rust runs the optional Tavily search first with the typed message as the query. It passes bounded source text and any selected UTF-8 attachment to the chosen provider as untrusted reference data. The earlier one-file Meta attachment command is retained for compatibility; the current composer uses persistent, provider-independent attachments described below. The Meta API key uses the regular endpoint-bound Keychain credential store.

The browser preview has a clearly labeled synthetic in-memory/mock backend for UI tests. The native app always calls Rust. Live provider coverage is recorded separately from fixtures.


Project metadata, imported copies and indexes belong to the local profile. Imported files live in UUID-based project directories. Linked directories remain external. Native selection supplies paths, and Rust resolves model-facing references using opaque IDs. Import and linked indexing run on blocking workers with explicit bounds. Linked reads use descriptor-relative traversal and reject symlinks, root replacements and changed revisions. Refresh is explicit. Archiving hides a project, while confirmed deletion journals its state before moving managed files to Trash and transactionally removing its chats and index.

Readable thinking is normalized into its own Rust event and persisted display field. It is not replayed as conversation context or indexed for retrieval. The view keeps its disclosure separate from Markdown answers and uses CSS-only activity motion; encrypted or signed continuation payloads remain inside protocol adapters.


Chat attachments use native picker/drop paths held only in Rust. A one-use opaque drop ticket authorizes a bounded import. SQLite owns raw bytes, extracted text and message ownership; detached drafts are pruned at startup, and deleting the last referencing message collects the file. Branches share ownership, while Edit creates fresh draft copies. Failed batches roll back their drafts. Text/PDF extraction and Apple Vision OCR run on blocking workers. Image attachments share recognized text only.

Conversation preflight chooses bounded recent messages, same-chat earlier excerpts and explicitly enabled project references before committing the new turn. Provider-reported model capacity constrains the budget with space for a reply; conservative byte limits remain when tokenization is unavailable. The UI context preview uses the same selection path and labels its character-based token estimate. Optional Apple NaturalLanguage sentence embeddings stay on device, never trigger asset downloads, and fall back to FTS when unavailable.

The content-free usage ledger records admission before each provider round, then merges cumulative provider counters on completion or Stop/error. An interrupted process leaves an unknown request. Endpoint/model identities scope rates and capacity; monthly totals include only this app’s recorded calls. Reported charges and user-rate estimates remain separate. Subscription percentages are never derived from token counts. Only explicit account refresh can contact a documented usage endpoint; there is no startup/background polling.


Installation retains the stable production bundle and profile identity. The local development installer stages and validates a replacement for `/Applications/Melly.app`, with rollback on failed promotion. The native CLI handles help/version/open/update before Tauri or profile initialization. Update aliases delegate only to the installed Melly Homebrew cask; they are not model tools, startup networking or a second updater. See updates.md for the unpublished cask preparation and distribution prerequisites.

In a linked project chat, any configured model can include one file proposal in its reply. Melly renders the proposed change inline and writes it after the user clicks **Apply change** or enables **Auto-apply** for that chat and linked folder. The applied reply keeps a compact diff. Rust binds the reply to the linked folder's identity, checks the current file before review and again before writing, and saves through the verified directory. Secret paths and other paths excluded from indexing are also excluded from edits.

Portable exports are versioned, bounded JSON or chat Markdown. Native save panels supply destinations and private temporary files are atomically renamed. Restore validates every identity, relationship, file name and payload before a transaction adds new UUIDs and rewrites citations. Credentials, settings, billing records and linked directories are excluded. Restored project context is off; external folders require a new native selection.
