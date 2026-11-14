# Architecture

Melly is a native Tauri Mac application. Rust is the trust boundary and owns all expensive/stateful work. The system WKWebView renders a React view. Model inference runs in the cloud.

`melly-core` is independent of the desktop shell so storage, tool scope, network protocols and cancellation can be tested quickly without launching a window. `models.rs` defines serializable data; `storage.rs` owns SQLite WAL, FTS5 and transactions; `engine.rs` owns turn execution, bounded context, read-only tool dispatch and persistence. The provider modules normalize SSE into typed events; credentials/auth use Keychain only. The Tauri shell exposes the minimal command contract in contracts.md.

Launch reads bounded local state, creates the window, and never waits for network or model discovery. Message history pages are bounded; text search uses a local FTS index. Streamed deltas render in animation-frame batches; final/partial turns are committed once. Cancel tokens drop HTTP requests promptly. There is no polling timer while idle.

The provider HTTP client is reused across turns, keeping origin-scoped connections and TLS sessions warm. Credentials are attached to individual requests, never client-wide defaults. Redirects, environment proxies and cookies remain disabled; new connections still use the public-address-only resolver.

Each connection has one provider identity and one endpoint/billing route. Endpoint changes cannot reuse another connection's key. Subscription errors remain on that route. Provider keys are entered in a native secure field embedded in settings; the search key uses a native secure dialog. Neither passes through JavaScript. OAuth belongs to this app; it does not read other applications' credentials or ChatGPT history.

The desktop exposes only real provider connections. If the saved selection was the internal mock or is no longer connected, it asks the user to choose or connect a provider; it does not select a paid route automatically. The mock and synthetic examples remain available in the browser preview and automated tests.

Folder tools accept a query only, never an authorization scope. Rust derives the folder from the active chat and returns at most five bounded snippets from other chats in that folder. The model sees references with source IDs, not whole transcripts. Tool results are explicitly untrusted, and the only actions are local search and a fixed read-only web search API. Web citation navigation is separate and validates public HTTPS URLs.

For Meta Model API, Rust runs the optional Tavily search first with the typed message as the query. It passes bounded source text and any selected UTF-8 attachment to the chosen provider as untrusted reference data. Draft attachment bytes are kept in Rust memory and omitted from the transcript. A provider reply can quote the shared text, and the reply is saved with the chat. The Meta API key uses the regular endpoint-bound Keychain credential store.

The browser preview has a clearly labeled synthetic in-memory/mock backend for UI tests. The native app always calls Rust. Live provider coverage is recorded separately from fixtures.
