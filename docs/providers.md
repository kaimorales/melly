# Cloud providers

Checked against official documentation on 2026-11-13. These adapters are implemented and tested with synthetic fixtures and local HTTP servers. **The ChatGPT plan route also passed native live authorization, account model discovery, multi-turn inference, Stop, folder-tool continuation and use after relaunch.** Other routes remain unverified with real credentials. Saving a key means configured, not live verified; one working account/model does not establish general entitlement.

## Explicit routes

Each native connection has one immutable base URL. Credentials are keyed by a SHA-256 digest of provider ID plus the normalized exact base URL. Selecting a different provider, Qwen billing route, custom host, or custom base path does not reuse another route's credential. Errors never trigger another connection, endpoint, model, or billing route.

| Connection | Base URL | Wire protocol | Initial model hint |
| --- | --- | --- | --- |
| Mock | No network | Synthetic adapter | `melly-demo` |
| ChatGPT Plan | `https://api.openai.com/v1` | Responses | `gpt-6.1-sol`; replace with account discovery |
| Meta Muse Code Plan | Disabled | No host started | None |
| Claude API key | `https://api.anthropic.com/v1` | Messages | `claude-sonnet-5-5` |
| Gemini API key | `https://generativelanguage.googleapis.com/v1beta` | Interactions | `gemini-4-hydrogen` |
| Grok API key | `https://api.x.ai/v1` | Responses | `grok-4.7` |
| OpenRouter API key | `https://openrouter.ai/api/v1` | Chat Completions | `openai/gpt-6.1-sol` |
| DeepSeek API key | `https://api.deepseek.com` | Chat Completions | `deepseek-flash` |
| QwenCloud Standard API key | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | Chat Completions | `qwen3.8-flash` |
| QwenCloud Token Plan | `https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1` | Chat Completions | `auto` |
| Kimi / Moonshot API key | `https://api.moonshot.ai/v1` | Chat Completions | `kimi-k3.1-pro` |
| Ollama Cloud API key | `https://ollama.com/v1` | Chat Completions | `gemma4:31b` |
| Custom compatible | Explicit public HTTPS base | Chat Completions | User-entered ID |

Both Qwen entries use **Singapore** endpoints. Standard API and Token Plan are different billing identities and require their matching, region-specific credentials. There is no Coding Plan or Kimi Code host integration. Standard's documented shared domain remains supported; workspace-specific domains can be used only as an explicitly separate Custom connection. The Token Plan endpoint is documented for compatible interactive AI tools; account eligibility and acceptance of this client remain unverified. [Official Token Plan setup](https://www.alibabacloud.com/help/en/model-studio/token-plan-personal-quick-start), [Team setup and billing separation](https://www.alibabacloud.com/help/en/model-studio/token-plan-team-quickstart), [Generic integration guide](https://www.alibabacloud.com/help/en/model-studio/more-tools).

Claude in Melly requires a Claude Console API key. A school account or Claude subscription does not automatically grant this app API access; ask the organization's administrator about Console access and billing. Anthropic documents API credits for eligible Max, Ultra and Team plans, subject to linking a Console organization. Those credits are distinct from chat usage limits; school eligibility has not been verified. [Get an API key](https://platform.claude.com/docs/en/get-api-key), [API credits for subscribers](https://platform.claude.com/docs/en/about-claude/api-credits-for-subscribers).

DeepSeek uses the documented general API base and `deepseek-flash`. Moonshot uses its general API with the documented current Kimi model, independently of Kimi's coding product. [DeepSeek quickstart](https://api-docs.deepseek.com/quick_start/agent_integrations/), [Kimi quickstart](https://platform.kimi.ai/docs/get-your-api-key).

Ollama Cloud uses a key created in the user's Ollama account and runs remotely, without an Ollama installation or local inference. Ollama says included plan credits are used first and purchased credits can follow; actual entitlement and billing depend on the account, so Melly labels this as an API-key and credits route rather than promising unlimited subscription use. There is no automatic fallback to another provider. The hosted catalog is `https://ollama.com/api/tags`; entries explicitly lacking completion support are hidden. [Cloud access](https://docs.ollama.com/api/openai-compatibility), [Account pricing](https://ollama.com/pricing).

## Native behavior

- **ChatGPT:** New app-owned OAuth is implemented in `auth`, separately from adapters. Requests use `store:false`, `stream:true`, `instructions`, full local input history, and the public Responses endpoint. Function tools are grouped under the `melly` namespace as required by the plan preview. Unsupported plan fields and `previous_response_id` are omitted. Success requires `response.completed`; a partial answer or `[DONE]` alone is not success. Model discovery preserves the returned ordering of visible account model slugs. [Plan inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference), [Preview limits](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).
- **Claude:** Native Messages content blocks, tool-use/result blocks, and completion handling; `anthropic-version:2026-10-21`. When a response includes both native web search and a client folder search, streamed server-tool arguments are reassembled and retained in the continuation; invalid arguments fail before any client tool runs. A paused native search continues on the same route with its original content blocks, within the existing four-round turn limit; those blocks stay in memory and are not serialized into chat history. Basic `web_search_20250305` is used, capped at three uses. Newer dynamically filtered search versions can implicitly provision code execution, so those versions are not enabled. [Messages streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [Server tool continuation](https://platform.claude.com/docs/en/agents-and-tools/tool-use/server-tools), [Official search versions](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool).
- **Gemini:** Native Interactions, `Api-Revision:2026-05-20`, `store:false`, and typed input/step replay. Tool argument fragments and thought signatures are preserved during an active tool round. No server conversation handle is retained. Success requires `interaction.completed` with completed or actionable tool status. [Text generation](https://aistudio.google.com/docs/text-generation), [Streaming](https://aistudio.google.com/docs/streaming), [Stateless function calls](https://aistudio.google.com/docs/function-calling).
- **Grok:** Native Responses and `web_search`; only the documented `grok-4.7` search route is currently allowlisted. [Official search example](https://docs.x.ai/developers/tools/web-search).
- **OpenRouter:** Chat Completions, no model fallback array, and `provider.allow_fallbacks:false`. Native server search is not assumed. Compatible reasoning continuations are replayed without showing them as assistant output. The initial provider chosen by OpenRouter is not pinned to a hosting company; the app does not claim otherwise. [Provider routing](https://openrouter.ai/docs/guides/routing/provider-selection), [Initial model and API](https://openrouter.ai/openai/gpt-6.1-sol), [Reasoning continuation](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens).
- **Custom:** Text, streaming and model discovery can be attempted, with an API key required by the current app and a manual model ID. Client tools and native web search remain disabled because a compatible endpoint does not establish those capabilities. A failed model listing remains an error; no fabricated catalog is substituted.

Model listing is explicit, never startup work. It returns at most the first 1,000 valid models from the first response page. Claude and Gemini are asked for their documented maximum page size instead of their smaller default pages. Gemini entries explicitly reporting no content-generation method are omitted; entries with unknown capability and manually entered IDs remain available. Discovery does not prove inference access or that every returned model supports the selected chat protocol. Initial model hints are documented examples, not promises of availability. [Claude models](https://platform.claude.com/docs/en/api/models/list), [Gemini models](https://ai.google.dev/api/models).

## Search and sources

Native search is enabled only for exact model IDs in `supports_native_search`; provider-level `native_search` is descriptive, not a substitute for this model check. Native search activity is reported separately from client tool calls. Citation events become bounded `Source` records with public HTTPS URLs. Native search errors remain errors. A model can decline to search even when the tool is offered; no search claim is fabricated.

For the compatible providers and models without known native search, the run controller can explicitly offer `search_web` only when the separate search key is configured. `web_search(query,key,cancel)` sends a POST only to `https://api.tavily.com/search`, with its own bearer key, at most five results, basic search, and raw content/images/answer disabled. No arbitrary page fetching is implemented, and provider keys cannot be obtained by this function. [Official Tavily endpoint](https://docs.tavily.com/documentation/api-reference/endpoint/search).

Mock is credential-free and deterministic. When offered, it requests `search_folder` and `search_web`; the real run controller executes those tools. Offered tools are the permission boundary; `ProviderRequest.web_search` asks for native provider search only. Mock then quotes only the returned reference snippets, carries their actual source IDs, and marks its answer Mock. The adapter does not invent a successful search or live connection. Ordinary Mock replies yield between batches without artificial token delays.

For a human-operable native Stop demonstration, send **“Demonstrate streaming and Stop”** or **“Write a longer answer”** to Mock. Only these explicit Mock fixtures add approximately three seconds of simulated latency and identify the pacing in the reply. Their waits are cancellation-aware. All live adapters remain undelayed.

## Transport boundaries

- HTTPS only; URL userinfo, local hostnames, private/reserved literal IPs, and endpoint query/fragment parameters are rejected.
- A custom DNS resolver rejects empty or mixed public/private answers and supplies exactly the checked addresses to reqwest. There is no validation/connection second lookup. All redirects are refused and environment proxies disabled. TLS certificate validation remains enabled.
- Cancellation interrupts connection setup, waiting for response headers, and blocked stream reads. The engine additionally protects the stop/write race. Dropping a request does not prove that the provider stopped billing work already accepted.
- Errors include provider and HTTP status when available. Raw HTTP bodies, redirect locations, key values, and provider-supplied error messages are not exposed or logged.
- SSE lines/events and complete streams are bounded at 1 MiB and 8 MiB. JSON responses and serialized requests are bounded at 2 MiB. Native step indices, tool counts, citations, query length and snippets are bounded. Truncated, invalid, incomplete, or explicitly failed streams never emit Done.
- The live ChatGPT plan endpoint was observed returning valid SSE without Content-Type. Only this fixed Responses route accepts an absent header, after bounded framing validation. Explicit incompatible types and other providers retain the normal SSE media-type requirement; the same decoder, completion and cancellation checks apply.
- Client tool calls are dispatched only after a successful protocol terminal event and validated argument JSON. IDs and fragments are preserved. Native continuation metadata is ephemeral (`ToolCall.provider_data` or a Claude paused-search continuation); the engine must not persist or expose it.

## Evidence and remaining live checks

Synthetic fixtures are under `tests/fixtures/providers/`: compatible, Responses, Messages and Interactions SSE. They contain no real conversations, account tokens, or usable credentials. Loopback HTTP/TLS exceptions exist only inside test code; production URL validation cannot select them.

Verification commands:

```sh
cargo test -p melly-core --lib providers
cargo test -p melly-core --test providers
cargo clippy -p melly-core --all-targets -- -D warnings
```

Coverage includes byte-fragmented UTF-8 and multiline SSE; tool argument assembly and IDs; all four native/compatible terminal contracts; broken streams and native search errors; sanitized 429/error bodies; cancellation while connecting/waiting/reading and mid-Mock; redirect credential isolation; private/reserved IP and mixed DNS answers; immutable endpoints and key identities; SIWC request fields/namespaces; native tool continuation; model visibility/order; and deterministic Mock tool/source behavior. Red failures were observed before the corresponding implementations.

Verified on 2026-11-13: **28 provider unit tests and 10 public integration tests passed**, including external search with the native-search flag off and cancellation during the explicit paced Mock demo. The full `melly-core` all-target Clippy check also passed with warnings denied.

The subsequent live ChatGPT fix added seven transport regressions; the full suite now passes 109 Rust tests and 25 UI tests. ChatGPT was verified natively with `gpt-5.6-sol` and synthetic conversation content, including an actual folder-tool round trip and source navigation. See [verification](verification.md).

Native ChatGPT web search subsequently passed with `gpt-6.1-sol` on the tested account, returning a completed reply with a source citation and no external search key. The local allowlist is conservative; it does not prove other model IDs lack search. See [the proposed app-owned search service](research/search-service.md) for a path that avoids end-user search keys across providers; that service has not been deployed.

Still pending for ChatGPT: renewal/revocation, usage-limit and entitlement failures, and other account/model combinations. Still pending for **other live providers**: account authorization, the selected model, actual inference and tool follow-up, native search/citation shape, region/plan entitlement and billing confirmation, real-service cancellation, and account-specific errors. Muse remains intentionally disabled.
