# Search without user-supplied keys

Research and native check: 2026-11-13. This document proposes an app-owned service; no service was deployed, account created, credit purchased or shared key embedded.

## What works today

Melly already supports provider-native search on a conservative model allowlist and a separate Tavily adapter. `providers/catalog.rs::supports_native_search` currently enables ChatGPT native search for `gpt-6.1-sol`, not every discovered model. This is a configuration restriction, not proof that another model lacks search. `engine.rs` otherwise requires the user's dedicated search key. That explains the disabled search control on the previously selected `gpt-5.6-sol`.

Native live test: selected the account-discovered `gpt-6.1-sol`, enabled Search the web, and asked for the official Rust homepage. The ChatGPT plan route completed with a web source titled Rust Programming Language and a public source URL, with no external search key. The same test passed again on the white-icon build after its development credential was reconnected through the official flow. This proves the tested account/model route, not all ChatGPT accounts/models or every provider. [Capture](../screenshots/native-chatgpt-web-search.png).

Official provider documentation confirms hosted search exists:

- [OpenAI Responses web search](https://developers.openai.com/api/docs/guides/tools-web-search): a hosted `web_search` tool with citations. The [ChatGPT plan preview](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations) makes availability subject to model and account/workspace policy.
- [Claude web search](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool): native search with its own pricing and capabilities. Melly uses the direct basic version, avoiding implicit code execution.
- [Gemini Google Search grounding](https://aistudio.google.com/docs/google-search): live grounding and cited results.
- [Grok web search](https://docs.x.ai/developers/tools/web-search): hosted search through the provider's tools.

These hosted options use their provider connection and applicable billing; “no separate search key” does not mean free or unlimited search.

## Recommended product path

Keep one Search the web control. Use a verified native route when available. For the remaining routes, offer included Melly Search backed by a small service owned by Melly. The app owner provisions one upstream search account; end users do not see an API-key setup screen. Make the active search route and any allowance discoverable without crowding the composer. Never retry a native failure through a differently billed route silently.

Proposed flow:

`Melly Rust client → Melly search gateway → documented search API → bounded sources → selected chat provider`

The gateway owns the search key in server secrets. A key bundled into a Mac executable, JavaScript, or a shared client Keychain record is not a private service credential. Model-provider keys and ChatGPT tokens stay on the Mac and never authenticate to the search gateway.

The gateway accepts only a bounded query and small result limit, calls one configured search API, and returns source IDs, titles, public HTTPS URLs and short snippets. It is not an arbitrary URL proxy. The existing Rust tool boundary, cancellation and citation renderer can consume this result. Models with tools use `search_web`; a future text-only Custom route needs an explicit pre-search path before generation rather than falsely advertising tool support.

Use an independent Melly session or entitlement for quota enforcement, a per-answer search cap, rate limits, a global monthly spending ceiling and a kill switch. Anonymous installation IDs alone are resettable and cannot enforce a strong per-person allowance. Exhaustion returns a clear limit message without switching billing. No new network activity is needed at launch; search runs only on request.

The search service receives the query, not the full transcript. Results then go to the selected chat provider as untrusted reference material. Disclose this route, minimize query retention and keep content out of operational logs. Check the selected plan's result-storage/caching terms before persisting snippets or sharing a cache. Do not assume an API contract grants rights to underlying publisher content.

## Initial service choice and costs

Start a small pilot with Tavily because Melly already has its bounded basic-search adapter and citation mapping. This is an implementation-fit recommendation, not a measured search-quality comparison. Compare Brave before wider rollout.

| Service | Current published allowance/rate | Implication |
| --- | --- | --- |
| Tavily basic search | 1,000 free credits/month; one credit per basic request; pay-as-you-go $0.008/credit | Around $8 per 1,000 paid basic requests, before hosting and model tokens. Existing Melly adapter. |
| Brave Search | $5 per 1,000 requests; Search plan lists $5 monthly free credits | Lower listed search-request cost. Storage rights require checking the appropriate plan/custom terms. |

Sources: [Tavily credits and pricing](https://docs.tavily.com/documentation/api-credits), [Tavily plans](https://www.tavily.com/pricing), [Brave Search API plans and storage FAQ](https://brave.com/search/api/). These are request costs, not per-answer prices: an answer may make several searches. Allowances are shared across Melly's users when one service account funds them. Model input/output tokens and gateway hosting are additional. Prices must be rechecked before launch.

For example, 100 users making five basic searches per day for 30 days generate 15,000 requests. At Tavily's pay-as-you-go rate, that is $120 before any free-credit allowance, volume plan, hosting or model cost. This is arithmetic for sizing a budget, not a recommended spending commitment.

## What the owner would supply

One search-service account/key, a hosting destination, and a chosen usage/spending cap. Keys should be entered into hosting secrets directly, never pasted into a chat or committed. Users would only enable Search the web. The app-owned path remains proposed until those operational choices are made; provider-native ChatGPT search is already live-verified above.
