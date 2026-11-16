# Shared interfaces

The native executable also supports user-invoked `--help`, `--version`, `--open`, `--update` and `--upgrade` before application/profile initialization. An invocation through the `melly` launcher without arguments opens the app. Update aliases require the GUI to be closed and an installed `kaimorales/tap/melly` cask; they delegate to Homebrew and propagate failure. No IPC command exposes this management path. See updates.md.

Rust types in crates/melly-core/src/models.rs are the source of truth. JSON uses camelCase. Rust modules use snake_case.

## Provider ownership contract
`providers::catalog() -> Vec<ProviderSpec>` includes mock, chatgpt, muse-api, claude, gemini, grok, openrouter, deepseek, qwen-standard, qwen-token-plan, moonshot, ollama, ollama-local, custom. Qwen routes are distinct immutable entries grouped under QwenCloud. `ollama` means its cloud API; `ollama-local` is the separate fixed-loopback connection. Meta Model API uses an independent endpoint-bound Keychain credential, and its selection is explicit.
`providers::validate_connection(&Connection) -> anyhow::Result<()>` validates fixed endpoints, including the explicit local route, or public HTTPS custom. `providers::credential_id(&Connection) -> String` binds each secret to provider AND exact normalized base URL.
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
- `start_chat(input:SendInput, attachmentId?: string, attachmentIds?: string[])` -> Message (assistant placeholder). The optional legacy attachmentId is Meta-only. The current composer uses attachmentIds for all providers; Retry uses files on the original user message.
- `stop_chat(chatId)` -> void
- `open_source(source:Source)` -> void (web public HTTPS only; chat navigation handled UI)
- `demo_seed()` -> Bootstrap (idempotent synthetic Job search + other folder examples)
- `enter_search_key()` -> bool
- `remove_search_key()` -> void
- `frontend_ready()` -> void records one content-free process-to-render timing after bootstrap and two animation frames

Events `chat-stream` -> StreamUpdate: kind delta (answer text), reasoning (display-only thinking text), sources, tool (text short activity), done, stopped, error. Done/error/stopped require refresh messages. Events carry chatId/messageId so background streams cannot spill into selected chat. Rust persists final/partial replies. The UI reveals bursty answer deltas over at most 120 ms at roughly 30 paints per second. Reduced motion, background replies, Stop, and errors flush immediately.

Denied model tool calls return a bounded denial with no data. Operational external-search failures terminate the reply with a sanitized visible error; they cannot be silently converted into a completed searched answer. Automatic folder permission recognizes only a direct opening request without quoted/multiline/ambiguous text; the explicit composer toggle handles other wording. Retry is unavailable while viewing older history pages.

Frontend browser-only preview/test mode uses src/mock-backend.ts implementing same command/event contract and clearly labeled Mock; never claims Rust integration tests cover browser shim. Real app always invokes Rust. All configured/live states truthful; Key saved is not Live verified. No keys/tokens in React state/DOM/fixtures.

## Sidebar organization and code display

Sidebar pointer gestures, row menus and the header menu all use the existing Rust `move_chat` and `delete_chat` commands. Dragging is internal to the app; no chat title or ID is written to the system drag clipboard, and external drops cannot move chats. Folder destinations spring open after 450 ms; dropping on Recent removes the folder assignment. The selected chat/draft stays in place, and moving it revokes its per-turn folder permission. Rust rejects mutation of an active chat. The UI changes location only after persistence succeeds.

Delete captures the intended chat, requires confirmation and keeps errors in the dialog. Removing another chat preserves the selected chat/draft. SQLite removes the deleted messages and search-index entries; old citations report an unavailable source.

Fenced-code coloring is presentation only. A lazily loaded worker returns text/span tokens for explicit supported languages; source code is never executed or sent to another service. Streaming text does not wait for coloring. Bounds, watchdog behavior and dependency notices are recorded in [code-block design](design/code-blocks.md).

## Managed project files
Existing folder IDs identify projects. `project_files(folderId)` lists up to 500 imported files. `import_project_file(folderId)` uses a native picker and returns file metadata after copying up to 20 MiB into `projects/<folder UUID>/<file UUID>/<filename>` beneath the profile. `show_project_directory(folderId)` opens the managed directory in Finder. `open_project_file(fileId)` resolves the path from Rust-owned metadata. Imported files retain their original names and have unique IDs, including when names collide. The frontend receives metadata only. Expensive import work runs off the UI thread.

Project import extracts UTF-8 text (up to 2 MiB) or selectable PDF text (20 MiB, 500 pages, 2 MiB extracted text). SQLite FTS5 indexes bounded passages with PDF page or text line locations. Enabling Project context retrieves a combined maximum of five file and sibling-chat excerpts from the active chat's project for every provider. Tool-capable providers can additionally refine sibling-chat searches. `file_source(sourceId)` resolves an opaque stored passage ID. `open_pdf_source(sourceId)` opens a native PDFKit viewer at the cited page. Locked PDFs report an actionable import error. Empty-text pages attempt on-device Apple Vision OCR, bounded to 20 scanned pages. PNG/JPEG input also uses OCR, limited to 40 megapixels and 2 MiB recognized text. Images supply text only. Unavailable Vision services produce a visible error. Native image and scanned-PDF import were verified with synthetic fixtures and a live Ollama reply; the shell-only Vision test is ignored where system services are restricted.

`archive_project(folderId,archived)` retains all content and hides archived projects from normal lists/search. `archived_projects()` lists archived and pending-deletion projects. Project mutations reject active replies. `delete_project(folderId)` first persists a pending-deletion marker, moves only the managed UUID directory to macOS Trash, then transactionally deletes chats, messages, indexes and project metadata. Failed/interrupted deletion stays visible in Archived projects for an idempotent retry. Archived/deleting projects cannot receive new chats or turns. Deleting chats is permanent; imported files can be recovered from Finder Trash.

`set_project_context(folderId,enabled)` saves the visible project preference. `Folder.contextEnabled` restores it across chats and restarts. The composer shows the selected provider as the recipient of enabled project context. `SendInput.folderContext` captures the effective permission for that turn, including an explicit override after moving a chat. Rust derives scope from the current chat and includes at most five initial file/chat excerpts, with a combined maximum of 12,000 bytes. Past-chat excerpts retain their exact message citations. Model tools can refine searches within the same project when supported.

## Linked folders
`link_project_directory(folderId)` uses a native directory picker and records one canonical root and filesystem identity per project. `project_link(folderId)` returns its name/status. `refresh_linked_project(folderId)` scans supported text/code/PDF files in a blocking worker, bounded to 20,000 entries, depth 16, 400 indexed files and 50 MiB read. `cancel_project_scan(folderId)` cancels between entries. Hidden files, credential/secret filenames and dependency/build trees are excluded. A project holds at most 500 imported and linked files combined.

`unlink_project(folderId)` removes the linked index and pointer. External files are edited only through the scoped file-edit flow. Reads open each relative component with `openat` and `O_NOFOLLOW`, verify the selected root's identity, require regular files and enforce byte limits. SHA-256 revisions are checked before references are sent and when citations open. Chat skips unavailable or changed excerpts and continues with the remaining context. Refresh files updates the index and makes current passages eligible again. Unchanged indexed files retain citation IDs across refreshes. Changed/deleted files invalidate their old citations. Index replacements are atomic. Project deletion trashes only the managed UUID directory.

## Local Ollama
`ollama-local` is a separate explicit connection fixed to `http://127.0.0.1:11434`. Saving its model enables it; `disconnect_local()` removes that setup. Startup checks only saved metadata. The native local HTTP client has no credentials, redirects, or environment proxies. Catalog discovery calls `/api/tags` on demand. Before `/api/chat`, `/api/show` verifies local model metadata and rejects remote models. The NDJSON decoder requires `done`, bounds chunks/output, preserves partial text on failure, and uses the existing cancellation path. Melly never starts Ollama or downloads models.

The local request uses at most 8,192 context tokens, capped by the model's reported capacity, and reserves 512–2,048 tokens for generation, depending on model capacity. Recent history uses a conservative byte budget with room for templates, and at most two 400-character project excerpts. Oversized current messages report an actionable error. Switching providers retains stored history and provider/model metadata per reply. Local availability is checked only on user actions; saved setup does not claim a live-tested server.

## Thinking display

`Message.reasoning` is a separate, default-empty SQLite column. Rust emits `ProviderEvent::Reasoning` and IPC `kind: reasoning` deltas for readable provider thinking or summaries. Native Ollama's `message.thinking`, compatible reasoning fields and readable native summaries stay separate from answer deltas. A bounded prefix splitter recognizes leading `<think>` and `<thinking>` blocks even when tags cross stream chunks. Inline examples and fenced code remain ordinary answer text. Opaque signatures and encrypted continuation data stay internal to their existing protocol path.

Reasoning and answer text share the existing 1 MiB output limit. Stop/error persists partial reasoning separately; context replay, FTS search, retrieval and Copy response use only the answer. Previously stored replies are not rewritten. The frontend batches both channels per animation frame. A muted, keyboard-accessible Thinking disclosure opens during reasoning and automatically closes when the answer starts unless the user has explicitly chosen its state. The nine-cell mark disappears as soon as reasoning ends, including when the answer begins streaming or the response stops/fails; completed thoughts remain accessible through the text disclosure. Waiting/tool work uses the same nine-cell activity mark. CSS animates the clockwise perimeter and honors the app and system reduced-motion preferences. No interval or elapsed-time claim is added.


## Project file lifecycle and conversation branches

`import_project_file(folderId, replaceFileId?: string)` creates the new managed copy and atomically replaces its index before moving the older managed copy to Trash. A partial Trash failure retains the new file and reports the remaining cleanup. `remove_project_file(folderId,fileId)` moves only a validated managed copy to Trash before removing its index. Linked files cannot use these commands. Project operations serialize and reject active project turns.

`branch_chat(chatId,messageId,includeMessage)` copies up to 2,000 messages / 16 MiB through a scoped message into a new chat in the same project. Excluding the selected user message supports Edit. Original history is unchanged; message IDs are new, attachments retain shared ownership, and unfinished copied messages become interrupted. Active chats cannot branch.

## Chat files

`AttachmentInfo {id,name,bytes,kind}` is metadata only; `Message.attachments` defaults to an empty list. Kinds are text, pdf, or image-text. `pick_chat_files()` uses AppKit selection. OS drag/drop emits `file-drag: boolean` and `file-drop: ticket`; native paths never cross IPC. `accept_file_drop(ticket,accept)` consumes the ticket once and imports up to eight regular non-symlink files (20 MiB raw / 2 MiB extracted each). Failure removes every draft created by that batch. `discard_chat_file(id)` removes only an unowned draft; `copy_message_files(messageId)` makes fresh drafts for Edit with rollback on failure.

Selected files are not sent until a new `start_chat` accepts their unowned IDs. Rust validates scope and the context budget before inserting the user message; attachment ownership then commits in its own transaction. Files persist across Retry, branches and restart; the final owner’s deletion collects their bytes. Unowned attachments are pruned on restart; saved composer draft attachments are retained. References are explicitly untrusted and selected within 12,000 text bytes total, or 1,200 for local models, plus bounded metadata. Changed chat/view or cancelled attachment operations discard late results. Model APIs receive extracted text, not raw file bytes or filesystem paths.

## Context and accounting

The composer context meter shows the estimated prompt size, configured or
reported capacity, and any clipped history or selected references. The
estimate covers the next request and varies with provider tokenization.
Settings shows recorded usage for each supported connection and model.

`context_preview(chatId?,providerId,modelId,draft,attachmentIds,folderContext)` returns approximate tokens, reported capacity if known, included/omitted messages, selected Sources, this chat’s recorded input/output totals and an explanatory note. This is local work, debounced in the view and suspended during active generation. Earlier references are limited to the same chat, outside the recent 60-message window. Optional existing English sentence embeddings rank at most 2,048 same-project vectors; at most 128 chunks per file are embedded, with no asset download or remote embedding service.

`ProviderEvent::Usage(Usage)` normalizes optional input/output/cache/reasoning counters and provider-reported cost. Counters merge cumulatively within one request; provider rounds remain distinct. Admission is recorded before network execution, then updated idempotently on completion/error/Stop. Unknown values stay null. `ProviderEvent::ContextCapacity(usize)` and optional `ModelInfo.contextWindow` store reported model limits. Neither establishes plan allowance.

`provider_usage(providerId,modelId)` returns current UTC-month connection totals, missing-usage/unpriced-request counts, reported and estimated charges, and selected model rates. `save_usage_rates(providerId,modelId,rates)` stores validated optional USD-per-million input/output rates. Estimates exclude cache discounts, tool charges and taxes. `account_usage(providerId)` currently supports only the explicit OpenRouter route and calls its fixed `/api/v1/key` endpoint on demand. It returns reported key usage and optional budget/remaining values; no cross-provider credential use or fallback exists. Other subscription limits remain unavailable.

## Portable data

`export_data(chatId?,folderId?)` writes Markdown for one chat or version-1 JSON for a project/full backup through a native save panel. Export rejects active replies. `prepare_restore()` reads and validates a native-selected backup and returns a one-use ticket plus counts; `finish_restore(ticket,confirm)` cancels or adds the prepared content. Restore never replaces existing identities. Payloads are bounded to 96 MiB JSON / 48 MiB decoded content, 500 projects, 5,000 chats/files, 20,000 messages and 2,000 attachments. UUIDs, timestamps, references and filenames are validated, new UUIDs/citations assigned, and new private files cleaned up on failure. Keys, settings, external directory contents/paths and usage history are excluded. Restored project permissions default off.

## File changes in linked chats

- `review_file_edit(chatId, messageId)` resolves a proposal from a completed assistant reply against its linked project. It returns a compact diff and a short-lived review ticket.
- `apply_file_edit(messageId, ticket)` revalidates the project and the reviewed file, applies the change atomically, and records the applied state. A proposal can edit or create one UTF-8 file of up to 1 MiB. Creation uses an unused name in an existing project directory.

- `file_edit_mode(chatId)` returns null without a linked folder, false for Ask, or true for Auto-apply. `set_file_edit_mode(chatId, automatic)` changes the current chat's mode for the app session. A grant must exist at the start and end of a successful turn. Moving the chat or re-linking the project revokes it.
- `applied_file_edit(chatId, messageId)` loads the saved diff for manually or automatically applied changes.

## Coding tools

Enable Project context for a chat in a linked directory to give the selected model project search and file reads. The app handles calls sequentially, with at most 16 operations and 16 model rounds per reply. Newly created or edited files are read directly from disk in later rounds. Text files are limited to 1 MiB for writes and results are bounded. Edits and deletes require a current-turn read and a matching file revision.

Ask pauses in the conversation before creating, editing, deleting, or running tests. The existing Auto-apply grant applies those operations to the same chat and link for the current app session. The conversation shows each mutation's diff and the test status and output.

The test tool discovers `npm run test` (or Node's built-in runner), `cargo test --offline`, and Python pytest/unittest. The optional `check` argument selects an existing test, build, check, typecheck, or lint script; it cannot supply shell text. Ask reviews the exact command before execution. The app copies eligible project files into a private temporary workspace, mounts existing local dependencies read-only, and allows generated output only in scratch storage. The linked project and files outside the sandbox cannot be modified by checks. The process receives a clean environment, no network access, bounded output, and a five-minute timeout; Stop kills its process group. Check commands are revalidated after approval. Dependencies must already be installed, and suites requiring network access or excluded files report a failure instead of bypassing isolation. The snapshot is bounded to 20,000 files and 200 MiB. Command success is reported separately from the model's explanation.

## Undo

File mutations persist before/after snapshots in a local journal before touching disk. `file_changes(chatId,messageId)` returns only path/status metadata; `undo_file_changes(chatId,messageId,changeId?)` reverses one change or the turn. Undo revalidates the original linked directory, all affected file contents, and newer changes before mutation. It cannot overwrite subsequent user edits or run during active replies. A partial filesystem failure preserves per-file recovery state for a retry. Deleted file contents and POSIX permissions can be restored; deleted-file ACLs and extended attributes are not retained. Snapshots are local, excluded from model requests and portable exports, and removed with the owning conversation.

## Saved composer drafts

`composer_draft(chatId?)` restores the unsent text, selected attachment metadata, and edit target for that chat (or the new-chat composer). `save_composer_draft(chatId?,draft)` persists bounded content locally as input changes, with serialized writes and no idle polling. Switching chats preserves each draft. Send flushes pending saves and consumes only the matching accepted draft; a failed send retains it. Draft-owned attachment bytes survive restart, remain local until Send, and are removed when discarded. Drafts and undo snapshots are excluded from portable exports.

## Continuing interrupted coding tasks

Coding replies expose Continue instead of destructive Retry. A continuation preserves the earlier reply, its undo journal, and recorded checks, and appends a new turn tied to the original task. Bounded action/check results are replayed as untrusted data; file snapshots stay private. Each mutation is associated with a durable operation receipt before writing. Repeated identical operations in the same task reuse the verified on-disk result, including recovery after interruption between the write and its completion record. If the file changed, the model must read it again; a new user task has independent receipts. Pending approvals are not restored as permissions, and Ask remains the restart default.

## Larger project files and paged search

Coding tools read, edit, create, and undo UTF-8 files up to 1 MiB. Search traverses up to 20,000 eligible entries and pages content scans in batches of 500 files / 16 MiB with up to 20 matches. `search_repo` accepts an optional project-relative `path` and a returned `cursor`; it reports skipped files and never treats a truncated scan as a complete search. Large trees require a narrower path. `read_file` returns `next_line` and `next_column` so long Unicode lines can be read without missing text. Writes still require a fresh file revision, and project-context excerpts keep their smaller prompt budget. Diff previews are bounded and explicitly mark omitted lines/text.

## Native coding functions

Tool-capable provider routes receive six named project functions. Rust validates arguments, derives scope from the chat, and executes calls serially through the same Ask/Auto-apply, revision, undo, and sandbox boundaries. Up to 16 operations/rounds are allowed per coding reply. Native IDs and signed provider continuation data remain intact; tool output is returned as untrusted data. Named compatible connections default to native functions, with an explicit **Compatibility → Tool calls → Text fallback** setting for servers without function calling. Text-only routes continue to use the existing bounded proposal parser. Errors never change endpoints or billing routes.

## Explicit context compaction

The context ring offers **Compact context**. It uses only the currently selected provider/model, with tools and web disabled, to summarize older conversation into bounded task notes. At most eight sequential model requests are admitted and recorded as usage; Stop or failure leaves the prior context unchanged. The newest four messages stay verbatim. Summaries retain a durable prefix boundary; ordinary context selection omits only that prefix and includes its notes. All original messages, attachments, and undo records remain untouched. **Task notes** exposes the actual summary and **Undo compaction** restores normal history selection. Summaries are references, not instructions, and are excluded from exports; originals are exported normally.

## Fresh linked references

Before a turn sends selected references, a blocking worker checks at most five relevant linked files. Changed UTF-8 text up to 2 MiB is reindexed locally, then the references are selected and revision-checked again. Missing, binary, replaced-root, or symlinked files are omitted without interrupting the reply. Imported copies and PDFs retain their explicit refresh/reimport flow. This performs no startup/background scan and no remote embedding call.

## Compact response details

Each reply exposes its sources in one closed, keyboard-accessible disclosure; inline citations remain direct links. Completed file operations abbreviate long paths in the summary and retain the full path beside the expanded diff. Interrupted coding replies retain their changes and expose Continue. Narrow layouts and both themes keep permissions, diffs, and source controls inside the message column.
