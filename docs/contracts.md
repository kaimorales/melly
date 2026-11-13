# Shared interfaces

Rust types in crates/melly-core/src/models.rs are the source of truth. JSON uses camelCase. Rust modules use snake_case.

## Provider ownership contract
`providers::catalog() -> Vec<ProviderSpec>` includes mock, chatgpt, muse (disabled), claude, gemini, grok, openrouter, deepseek, qwen-standard, qwen-token-plan, moonshot, custom. Qwen routes distinct immutable entries; UI groups both beneath QwenCloud. No OAuth except ChatGPT; no coding hosts.
`providers::validate_connection(&Connection) -> anyhow::Result<()>` validates fixed endpoints or public HTTPS custom. `providers::credential_id(&Connection) -> String` binds each secret to provider AND exact normalized base URL.
`providers::list_models(connection: &Connection, credential: Option<&str>, cancel: CancellationToken) -> Result<Vec<ModelInfo>>`.
`providers::stream(request: ProviderRequest, credential: Option<String>, sink: ProviderSink, cancel: CancellationToken) -> Result<()>`. Never Debug/log credentials. Done required for complete. Preserve tool-call IDs and arguments across streaming fragments. Native search citations become Sources; compatible providers use explicit external search tool if configured. Return sanitized errors with provider/status, not raw response/request bodies.
`providers::web_search(query:&str, key:&str, cancel:CancellationToken) -> Result<Vec<Source>>` calls only fixed documented search endpoint; no URL fetch.
`providers::public_url(&str) -> Result<url::Url>` rejects non-HTTPS, userinfo, localhost, private/reserved literal IPs, redirects. No cross-origin credential redirects. Root must enforce this also for opening citations.

## Secret/auth ownership contract
`credentials::SecretStore: Send+Sync { get(&self,id:&str)->Result<Option<String>>; contains(&self,id:&str)->Result<bool>; set(&self,id:&str,value:&str)->Result<()>; delete(&self,id:&str)->Result<()> }`. Production `contains` checks attributes without decrypting a credential or requesting authentication. Startup must use it instead of `get`.
`credentials::KeychainStore::new() -> Self` uses macOS Keychain, never SQLite/files. `credentials::MemorySecrets::default()` for tests only.
`auth::sign_in(vault:Arc<dyn SecretStore>, on_url:Arc<dyn Fn(String)+Send+Sync>, cancel:CancellationToken)->Result<()>` handles official new app-owned OAuth/loopback, DCR, PKCE/state/nonce, consented plan scope, no existing credential import. `auth::access_token(vault:Arc<dyn SecretStore>)->Result<String>` refreshes as needed. `auth::has_saved_plan(&dyn SecretStore)->bool` is startup-safe metadata; `auth::is_connected(&dyn SecretStore)->bool` reads credentials and must not run on startup. `auth::disconnect(&dyn SecretStore)->Result<()>`.
`auth_namespace::AuthNamespaces::{open,begin,activate,detach,selected_vault}` owns UUID-only selection metadata. `AuthAttempt::sign_in` produces an opaque `AuthorizedPlan` only after verified authorization. Native activation holds the login-generation lock; Engine::{plan_vault,select_plan_vault} snapshots immutable credential namespaces for plan operations. Base-vault API keys never move into a plan namespace.
`auth::begin_shutdown()` closes admission to new credential work before chats and login are cancelled. `auth::drain_pending(timeout)` waits for tracked sign-in and detached renewal writes. Native Quit uses `EXIT_DRAIN_TIMEOUT` (65 seconds), with no delay while idle. Damaged selector metadata disables only ChatGPT and is preserved on explicit recovery; it cannot block history or Demo startup.
Native secret input: `src-tauri/src/secure_input.rs::prompt_secret(app:tauri::AppHandle,label:String)->Result<Option<String>,String>` async; native secure dialog on main thread, secret never reaches frontend. Save via Rust vault. UI only invokes enter_provider_key(providerId).

## Frontend / Tauri commands
- `bootstrap()` -> Bootstrap
- `create_chat(folderId: string|null)` -> Chat
- `chat(chatId)` -> Chat (one ID lookup for source navigation, independent of title/recent-list limits)
- `rename_chat(chatId,title)`, `delete_chat(chatId)`, `move_chat(chatId,folderId)` -> void
- `create_folder(title)` -> Folder; `rename_folder(folderId,title)` -> void
- `messages(chatId,before: number|null)` -> Message[] (last60 ordered chronologically)
- `search_chats(query)` -> Chat[]
- `save_settings(settings)` -> void
- `save_connection(connection)` -> void; immutable native endpoints; custom changes don't reuse old key
- `enter_provider_key(providerId)` -> bool (native secure input)
- `remove_provider_key(providerId)` -> void
- `connect_chatgpt()` -> void starts async sign-in, emits `connection-update` on state changes; `disconnect_chatgpt()` -> void
- `models(providerId)` -> ModelInfo[]; errors visible, custom model ID always supported
- `start_chat(input:SendInput)` -> Message (assistant placeholder; existing run rejected)
- `stop_chat(chatId)` -> void
- `open_source(source:Source)` -> void (web public HTTPS only; chat navigation handled UI)
- `demo_seed()` -> Bootstrap (idempotent synthetic Job search + other folder examples)
- `enter_search_key()` -> bool
- `remove_search_key()` -> void
- `frontend_ready()` -> void records one content-free process-to-render timing after bootstrap and two animation frames

Events `chat-stream` -> StreamUpdate: kind delta (text is new chunk), sources, tool (text short activity), done, stopped, error. Done/error/stopped require refresh messages. Events carry chatId/messageId so background streams cannot spill into selected chat. Rust persists final/partial replies. UI batching ≤one update per animation frame; no artificial token delay.

Denied model tool calls return a bounded denial with no data. Operational external-search failures terminate the reply with a sanitized visible error; they cannot be silently converted into a completed searched answer. Automatic folder permission recognizes only a direct opening request without quoted/multiline/ambiguous text; the explicit composer toggle handles other wording. Retry is unavailable while viewing older history pages.

Frontend browser-only preview/test mode uses src/mock-backend.ts implementing same command/event contract and clearly labeled Mock; never claims Rust integration tests cover browser shim. Real app always invokes Rust. All configured/live states truthful; Key saved is not Live verified. No keys/tokens in React state/DOM/fixtures.
