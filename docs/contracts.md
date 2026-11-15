# Shared interfaces

Rust types in crates/melly-core/src/models.rs are the source of truth. JSON uses camelCase. Rust modules use snake_case.

## Provider ownership contract
`providers::catalog() -> Vec<ProviderSpec>` includes mock, chatgpt, muse-api, claude, gemini, grok, openrouter, deepseek, qwen-standard, qwen-token-plan, moonshot, ollama, custom. Qwen routes are distinct immutable entries grouped under QwenCloud. Ollama means its cloud API. Meta Model API uses an independent endpoint-bound Keychain credential, and its selection is explicit.
`providers::validate_connection(&Connection) -> anyhow::Result<()>` validates fixed endpoints or public HTTPS custom. `providers::credential_id(&Connection) -> String` binds each secret to provider AND exact normalized base URL.
`providers::list_models(connection: &Connection, credential: Option<&str>, cancel: CancellationToken) -> Result<Vec<ModelInfo>>`.
`providers::stream(request: ProviderRequest, credential: Option<String>, sink: ProviderSink, cancel: CancellationToken) -> Result<()>`. Never Debug/log credentials. Done required for complete. Preserve tool-call IDs and arguments across streaming fragments. Native search citations become Sources; compatible providers use explicit external search tool if configured. Return sanitized errors with provider/status, not raw response/request bodies.
Transport normally requires `text/event-stream`. The fixed ChatGPT Responses route also accepts an absent Content-Type after a bounded SSE-prefix check; explicit incompatible types and other providers retain the strict header requirement. Headerless responses use the same bounded decoder and cancellation path, and still require the native completion event.
`providers::web_search(query:&str, key:&str, cancel:CancellationToken) -> Result<Vec<Source>>` calls only fixed documented search endpoint; no URL fetch.
`providers::public_url(&str) -> Result<url::Url>` rejects non-HTTPS, userinfo, localhost, private/reserved literal IPs, redirects. No cross-origin credential redirects. Root must enforce this also for opening citations.

## Secret/auth ownership contract
`credentials::SecretStore: Send+Sync { get(&self,id:&str)->Result<Option<String>>; contains(&self,id:&str)->Result<bool>; set(&self,id:&str,value:&str)->Result<()>; delete(&self,id:&str)->Result<()> }`. Production `contains` checks attributes without decrypting a credential or requesting authentication. Startup must use it instead of `get`.
`credentials::KeychainStore::new() -> Self` uses macOS Keychain, never SQLite/files. `credentials::MemorySecrets::default()` for tests only.
`auth::sign_in(vault:Arc<dyn SecretStore>, on_url:Arc<dyn Fn(String)+Send+Sync>, cancel:CancellationToken)->Result<()>` handles official new app-owned OAuth/loopback, DCR, PKCE/state/nonce, consented plan scope, no existing credential import. `auth::access_token(vault:Arc<dyn SecretStore>)->Result<String>` refreshes as needed. `auth::has_saved_plan(&dyn SecretStore)->bool` is startup-safe metadata; `auth::is_connected(&dyn SecretStore)->bool` reads credentials and must not run on startup. `auth::disconnect(&dyn SecretStore)->Result<()>`.
`auth_namespace::AuthNamespaces::{open,begin,activate,detach,selected_vault}` owns UUID-only selection metadata. `AuthAttempt::sign_in` produces an opaque `AuthorizedPlan` only after verified authorization. Native activation holds the login-generation lock; Engine::{plan_vault,select_plan_vault} snapshots immutable credential namespaces for plan operations. Base-vault API keys never move into a plan namespace.
`auth::begin_shutdown()` closes admission to new credential work before chats and login are cancelled. `auth::drain_pending(timeout)` waits for tracked sign-in and detached renewal writes. Native Quit uses `EXIT_DRAIN_TIMEOUT` (65 seconds), with no delay while idle. Damaged selector metadata disables only ChatGPT and is preserved on explicit recovery; it cannot block history or Demo startup.
Native secret input: `src-tauri/src/inline_secret.rs` positions an AppKit secure field in provider settings. Its generation and provider identity prevent stale UI sessions from saving to another provider. The frontend sends only field geometry, appearance, provider ID and lifecycle events; the key remains native and is saved directly to the Rust vault. The search-key flow still uses `secure_input.rs::prompt_secret` on the main thread.

## Frontend / Tauri commands
- `bootstrap()` -> Bootstrap
- `create_chat(folderId: string|null, preferredId?)` → Chat. Reuses an empty chat in the requested folder, preferring the currently selected chat.
- `chat(chatId)` -> Chat (one ID lookup for source navigation, independent of title/recent-list limits)
- `rename_chat(chatId,title)`, `delete_chat(chatId)`, `move_chat(chatId,folderId)` -> void
- `create_folder(title)` -> Folder; `rename_folder(folderId,title)` -> void
- `messages(chatId,before: number|null)` -> Message[] (last60 ordered chronologically)
- `search_chats(query)` -> Chat[]
- `save_settings(settings)` -> void
- `save_connection(connection)` -> void; immutable native endpoints; custom changes don't reuse old key
- `open_provider_key_field(generation, providerId, rect, theme, configured)`, `update_provider_key_field(generation, rect, theme)`, `focus_provider_key_field(generation)`, `close_provider_key_field(generation)` -> void; no key bytes cross IPC
- `save_provider_key_field(generation, providerId)` -> bool; saves the native field to the current provider/endpoint credential identity
- `remove_provider_key(providerId)` -> void
- `connect_chatgpt()` -> void starts async sign-in, emits `connection-update` on state changes; `disconnect_chatgpt()` -> void
- `models(providerId)` -> ModelInfo[]; errors visible, custom model ID always supported
- `pick_muse_attachment()` -> {id, name} or null. AppKit supplies the only accepted path. Rust holds one selected UTF-8 file up to 24 KiB in memory. `remove_muse_attachment(id)` discards it.
- `start_chat(input:SendInput, attachmentId?: string)` -> Message (assistant placeholder). Rust consumes the matching attachment on a new Meta Model API turn. Retrying a reply does not reshare the file.
- `stop_chat(chatId)` -> void
- `open_source(source:Source)` -> void (web public HTTPS only; chat navigation handled UI)
- `demo_seed()` -> Bootstrap (idempotent synthetic Job search + other folder examples)
- `enter_search_key()` -> bool
- `remove_search_key()` -> void
- `frontend_ready()` -> void records one content-free process-to-render timing after bootstrap and two animation frames

Events `chat-stream` -> StreamUpdate: kind delta (text is new chunk), sources, tool (text short activity), done, stopped, error. Done/error/stopped require refresh messages. Events carry chatId/messageId so background streams cannot spill into selected chat. Rust persists final/partial replies. The UI reveals bursty answer deltas over at most 120 ms at roughly 30 paints per second. Reduced motion, background replies, Stop, and errors flush immediately.

Denied model tool calls return a bounded denial with no data. Operational external-search failures terminate the reply with a sanitized visible error; they cannot be silently converted into a completed searched answer. Automatic folder permission recognizes only a direct opening request without quoted/multiline/ambiguous text; the explicit composer toggle handles other wording. Retry is unavailable while viewing older history pages.

Frontend browser-only preview/test mode uses src/mock-backend.ts implementing same command/event contract and clearly labeled Mock; never claims Rust integration tests cover browser shim. Real app always invokes Rust. All configured/live states truthful; Key saved is not Live verified. No keys/tokens in React state/DOM/fixtures.

## Sidebar organization and code display

Sidebar pointer gestures, row menus and the header menu all use the existing Rust `move_chat` and `delete_chat` commands. Dragging is internal to the app; no chat title or ID is written to the system drag clipboard, and external drops cannot move chats. Folder destinations spring open after 450 ms; dropping on Recent removes the folder assignment. The selected chat/draft stays in place, and moving it revokes its per-turn folder permission. Rust rejects mutation of an active chat. The UI changes location only after persistence succeeds.

Delete captures the intended chat, requires confirmation and keeps errors in the dialog. Removing another chat preserves the selected chat/draft. SQLite removes the deleted messages and search-index entries; old citations report an unavailable source.

Fenced-code coloring is presentation only. A lazily loaded worker returns text/span tokens for explicit supported languages; source code is never executed or sent to another service. Streaming text does not wait for coloring. Bounds, watchdog behavior and dependency notices are recorded in [code-block design](design/code-blocks.md).

## Context meter

The composer shows an approximate token count for the next prompt and the
selected model's context limit when available. It flags clipped history and
drafts that exceed the connection's configured limit. The estimate uses
characters and can differ from the server's tokenizer, chat template and tool
overhead. A context limit reserves space for generation and constrains the
history Melly sends. It describes the connected model and does not increase a
server's own context window.

## File edits in linked chats

Use a folder’s menu to link a local directory. Enabling Project context shares bounded contents of explicitly named paths with the selected provider. Rust binds a file proposal to the linked directory of the originating chat and allows one UTF-8 file per reply.

- `project_link(folderId)`, `link_project_directory(folderId)`, and `unlink_project(folderId)` manage the link. AppKit supplies the path.
- `file_edit_mode(chatId)` returns null without a linked folder, false for Ask, or true for Auto-apply. `set_file_edit_mode(chatId, automatic)` changes the current chat's mode for the app session.
- `review_file_edit(chatId, messageId)` returns a compact diff and a ticket. `apply_file_edit(messageId, ticket)` revalidates the workspace and file content before writing.
- `applied_file_edit(chatId, messageId)` returns the saved diff. Auto-apply performs the same scoped checks on a completed reply before recording its diff.

## Coding tools

Enable Project context for a chat in a linked directory to give the selected model project search and file reads. The app handles up to one tool call per model round and 16 rounds per reply. Newly created or edited files are read directly from disk in later rounds. Text files are limited to 24 KiB for writes and results are bounded. Deletes require a current-turn read and a matching file revision.

Ask pauses in the conversation before creating, editing, deleting, or running tests. The existing Auto-apply grant applies those operations to the same chat and link for the current app session. The conversation shows each mutation's diff and the test status and output.

The test tool selects Python unittest discovery or Node's built-in test runner. The macOS test process runs with a clean environment, networking denied, and writes limited to temporary scratch storage. It receives the linked project as read-only input. It times out after 90 seconds and does not provide arbitrary commands.

## Undo

File mutations persist before/after snapshots in a local journal before touching disk. `file_changes(chatId,messageId)` returns only path/status metadata; `undo_file_changes(chatId,messageId,changeId?)` reverses one change or the turn. Undo revalidates the original linked directory, all affected file contents, and newer changes before mutation. It cannot overwrite subsequent user edits or run during active replies. A partial filesystem failure preserves per-file recovery state for a retry. Deleted file contents and POSIX permissions can be restored; deleted-file ACLs and extended attributes are not retained. Snapshots are local, excluded from model requests and portable exports, and removed with the owning conversation.

## Saved composer drafts

`composer_draft(chatId?)` restores the unsent text, selected attachment metadata, and edit target for that chat (or the new-chat composer). `save_composer_draft(chatId?,draft)` persists bounded content locally as input changes, with serialized writes and no idle polling. Switching chats preserves each draft. Send flushes pending saves and consumes only the matching accepted draft; a failed send retains it. Draft-owned attachment bytes survive restart, remain local until Send, and are removed when discarded. Drafts and undo snapshots are excluded from portable exports.
