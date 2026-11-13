# Data and read-only tools

`crates/melly-core/src/models.rs` is the JSON/IPC source of truth. JSON field names are camelCase; provider wire formats are converted only inside Rust adapters. No credential appears in these models.

## SQLite

| Table | Contents |
| --- | --- |
| folders | UUID id, title |
| chats | UUID id, title, nullable folder foreign key, updated_at |
| messages | UUID id, chat FK, user/assistant role, text, status, source JSON, unique increasing millisecond timestamp, provider/model/billing metadata |
| messages_fts | FTS5 content index maintained by insert/update/delete triggers |
| settings | Validated JSON preferences and synthetic demo seed marker |
| connections | Provider ID, endpoint, selected model; never tokens/keys |

WAL, foreign keys, synchronous NORMAL, and a bounded cache are enabled. Chat deletion cascades messages/FTS rows. Message states: streaming, complete, stopped, error, interrupted. Startup marks abandoned streaming rows interrupted. Final replies and stopped partial text persist together with source metadata. An abrupt process kill may lose the current uncommitted partial text; the user's message is already durable. No background export, cloud history sync, telemetry or transcript logger exists.

History reads return 60 messages before an exclusive timestamp cursor. The UI displays at most 240 at a time and pages backwards for old source targets. Model context uses at most 60 completed messages / 60,000 UTF-8 bytes in the current chat. A user input is capped at 32,000 bytes; system instructions at 8,000 characters. Retry removes only the latest assistant attempt. Initial sidebar lists are capped at 500 chats/folders; local search returns at most 100 hits.

## Tools

Both tools use JSON Schema objects with `additionalProperties:false`. The coordinator validates arguments again, independent of the provider. A tool must be in the exact offered set for this turn. A model may not select a separate search billing route by emitting an unadvertised function.

```json
{"name":"search_folder","parameters":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":5}},"required":["query"],"additionalProperties":false}}
```

`folderId`, `chatId`, SQL, filenames and scopes are not arguments. Rust loads the active chat's folder, searches only other completed messages in that folder, and returns at most five relevant snippets. Queries are literalized as bounded quoted FTS terms; snippets are capped at 600 characters. A chat outside a folder has no folder retrieval authority.

```json
{"name":"search_web","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}
```

The engine bounds queries to 500 UTF-8 bytes. The adapter calls only the fixed Tavily search endpoint with the dedicated search key; native provider search is a separate explicitly selected capability. There is no arbitrary fetch, shell, file access, or action tool. Mock emits synthetic references instead of making network requests.

Tool responses are objects with `untrusted_reference_data:true`, a descriptive scope, and sources. A source has id, kind (`chat`/`web`), title, bounded snippet, optional HTTPS URL, optional chatId/messageId. Only local folder retrieval can originate chat sources; cloud-supplied sources are restricted to validated web URLs and have chat/message IDs removed. The UI resolves `melly-source:` links only against the reply's actual source records. Clicking a chat source navigates locally. Clicking a web source invokes Rust HTTPS/public-host validation before opening the system browser.

A turn permits at most four request rounds / eight client searches, twenty collected sources, and 1 MiB visible text. The network decoder has independent byte/event/tool bounds. Reaching the engine output limit cancels transport promptly and saves an error, distinct from a user Stop.

## Adapter interface

`ProviderRequest` contains a fixed `Connection`, system prompt, bounded `WireMessage` history, offered function schemas, and native web-search flag. `ProviderSink` emits `Delta`, `Sources`, `ToolCall`, `Activity`, and terminal `Done`; `CancellationToken` controls both request setup and stream reads. The coordinator also drops the active request on Stop.

Function-call IDs and native continuation metadata survive only for active tool rounds. `ToolCall.provider_data` holds native signatures/reasoning continuity when required; it is not transcript content and is never sent to the frontend or stored in SQLite. Protocol success must precede client tool dispatch. Missing completion, malformed arguments, native tool errors, and rate limits stay visible errors.

Every provider connection binds credential identity to both provider ID and normalized full base URL. Validation runs before using a credential. There is no retry to a second endpoint or billing route. To add an adapter, extend the catalog, wire serializer/decoder, synthetic stream fixtures, error/cancel tests, and source-backed route documentation; live success requires a separate account test.
