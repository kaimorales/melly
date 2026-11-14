# Verification record

Local verification on 2026-11-13, Apple Silicon macOS. The desktop uses Rust and the system WKWebView. No product release was published. Git checkpoints are pushed to `main` as requested.

## Provider reliability follow-up

The provider changes through `5cef14a` passed `npm run check` and `npm test`: **122 Rust tests passed**, one optional native Keychain test was ignored, and **50 Playwright UI tests passed**. The tests include Claude mixed and paused searches, bounded continuation and Stop, model discovery, safe HTTP error messages, billing-route isolation and cross-folder denial. They use synthetic data and do not establish live provider access.

An isolated `npm run build` produced a 12.78 MiB ad-hoc-signed macOS bundle. `codesign --verify --deep --strict` passed on the copy at `target/provider-update/Melly.app`. The previous Melly app was running, so this updated bundle was not launched or tested natively in this follow-up. Quit the old app before launching the updated copy. As observed in earlier builds, changing an ad-hoc signature can require an official ChatGPT reconnect; no Keychain policy was changed. Live tests for Claude and the other API-key routes remain pending suitable credentials and account permission.

## Reproducible checks

Run `npm run check`, `npm test`, and `npm run build`. `npm run check` checks TypeScript, Rust formatting and Clippy with warnings denied. `npm test` runs Rust plus Playwright browser UI tests. Browser tests use the labeled mock backend and do not prove native IPC or live provider access.

Final automated run: **110 Rust tests passed** (104 core and 6 native login-state tests), **49 Playwright UI tests passed**, zero failures. One optional Keychain test is excluded from the default suite and was run separately below. TypeScript, formatting and Clippy passed. The UI suite includes reopening a pending browser sign-in and retaining its cancel action.

The release build passed and produced `target/release/bundle/macos/Melly.app` (12.78 MiB); launch with `npm run launch`. This is a local ad-hoc signed build, not a notarized distribution. It requires no Rust, Node or development server at runtime.

The initial bundle had only a linker signature and failed strict resource-seal verification. The build now explicitly sets macOS `signingIdentity` to `-`, using [Tauri's documented local ad-hoc signing mode](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing). This signs the complete bundle without a signing certificate, Keychain password or Apple account; it does not claim notarization or permission to publish a release. Verify packaging with `codesign --verify --deep --strict target/release/bundle/macos/Melly.app`.

The rebuilt bundle passed that strict verification (exit 0), with identifier `com.melly.desktop`, an ad-hoc runtime signature, bound Info.plist and sealed icon resource. It was launched again successfully with history intact.

The Mac Keychain smoke check is deliberately opt-in:

```sh
node scripts/rust.mjs test -p melly-core native_keychain_roundtrip_without_authentication_ui -- --ignored
```

It creates, reads, updates and deletes one fresh UUID-named synthetic item. It never reads existing account credentials. Normal suites use an in-memory secret store. The scoped legacy Keychain policy test verifies that authentication UI is disabled during an operation and its prior policy is restored on failure.

The separate native Keychain check passed (1 test, 0 failures, 0.06 seconds), without authentication UI.

## Boundary and regression coverage

- Folder search excludes the current chat and every other folder, even for matching content or a model-supplied scope. Moving a chat changes its scope immediately. Unfiled chats have no implicit memory. Results and snippets are bounded.
- Quoted prompts, multiline reference text, negative requests and mere mentions do not enable folder access. A clear direct request or the explicit per-turn control is required.
- Qwen routes have distinct immutable endpoints and credential IDs. Plan errors cannot fall back to API-key routes. Keys are bound to provider and normalized endpoint; redirects, private/reserved destinations and mixed public/private DNS answers are rejected.
- Fragmented SSE, tool-call fragments, rate limits, broken connections, missing terminal events, output limits and cancellation are fixtures. A failed external search produces a visible error and cannot finish as a successful searched reply.
- Stop preserves partial output, prevents late deltas, and allows another turn. Background streams cannot alter another selected conversation. Retry does not duplicate the user message and is unavailable on older history pages.
- SQLite persistence, bounded pagination, literal chat search (including `%`, `_`, `!`), exact message citation navigation, renamed sources outside recent history, and crash-interrupted notices have regressions.
- Startup never decrypts credentials or performs model discovery. A credential vault failure leaves history and Demo usable. Native secret input never passes through React, and HTTP errors omit response bodies and secrets.
- ChatGPT fixtures cover app-owned registration, PKCE/state/nonce, loopback callbacks, signed identities, consent scopes, renewal, cancellation, generation races and immutable namespace selection. They do not authorize a real account.
- Damaged/unreadable ChatGPT selector metadata leaves history and other providers available, preserves the original file on explicit recovery, and rejects a substituted symlink. Orderly Quit drains tracked token renewal/sign-in work independently of stopped chats; queued exchanges are cancelled and new auth work is rejected.

## Native checks

The built app was launched and operated through macOS accessibility controls. Created a folder and two chats, retrieved the synthetic 18% fact by explicit request, opened its exact source, demonstrated streaming/Stop/Retry, opened a synthetic web citation in Safari, inspected both Qwen setup paths, and opened/cancelled the secure DeepSeek key dialog. Light/dark settings and official provider logos were visually inspected. Captures are in the [native walkthrough](demo.md).

Native testing found two issues that the browser mock could not reveal: a malformed SQLite `ESCAPE` expression and a legacy Keychain ACL prompt during sign-in. The SQL query now uses an explicit `!` escape with a Rust regression; credential operations use both per-query and scoped legacy noninteractive policies. Fresh ChatGPT authorization uses new app-owned namespaces instead of reading inaccessible old development items.

On the rebuilt app, native title/content search returned results without the SQL error. The saved folder fact, exact source, stopped partial response and web citation survived relaunch. Continue with ChatGPT reached the official OpenAI account chooser for Melly without a Mac password prompt. Reopening the pending panel retained Cancel; cancellation returned to Not connected. Account selection and consent were not performed, and the test browser tab was closed.

## Performance scope

`npm run perf` runs an in-memory SQLite probe with 10,000 synthetic messages and 100 samples. Final measured run: schema/open 1,448 µs; insert p50/p95 72/133 µs; same-folder full-text search 14.51/14.79 ms; 60-message history page 61/110 µs. This excludes cloud latency, disk I/O and GUI rendering. The synthetic cancellation regression requires completion within 150 ms; it does not benchmark a real provider.

The native shell writes one content-free `startup-metric.json` record after bootstrap and two animation frames. Three launches after navigation polish measured **506, 533 and 506 ms** from process entry to rendered UI. After the bundle-signing fix, its native launch measured **569 ms**. Every record was checked for freshness and `debugBuild: false`. This excludes the OS launch request and is not a guarantee for other machines or larger histories. These are local warm-cache measurements, not a controlled cold-boot benchmark.

Verified application code: `a26065d`; signed packaging configuration: `5964287`. The final navigation polish was rebuilt, checked with TypeScript/formatting/Clippy and all 25 UI tests, then operated natively: Settings → Back to chat retained the selected conversation and created no empty chat. The Rust suite was unchanged by that final one-line UI change. The app was left open in light mode on the synthetic demo.

## Sign-in wording and icon follow-up

On the same date, replaced the static “consent required” billing text with “ChatGPT plan”; connection state and the Continue with ChatGPT action remain separate. Re-ran TypeScript, formatting, Clippy, all 102 Rust tests and all 25 UI tests successfully. The actual authorization checks were not relaxed.

Reproduced the silver icon frame in Finder on macOS 26.6.2. A full-bleed graphite source removed it while preserving the approved conversation mark. The release build and strict bundle signature verification passed; the bundled ICNS is byte-identical to the checked-in asset. Relaunched Melly, confirmed history remained present, and inspected the updated ChatGPT panel and the final app's icon in Finder Quick Look. No password prompt or new authorization was involved. Native captures: [ChatGPT settings](screenshots/native-chatgpt-plan-light.png) and [packaged icon](screenshots/native-icon-borderless.png). Older macOS icon appearance remains unverified; see [icon packaging notes](design/app-icon.md).

## Live ChatGPT stream correction

Reproduced the user's failed `hi` reply in the native app: authorization had succeeded, but the adapter reported “chatgpt did not return an event stream.” Temporary content-free instrumentation identified HTTP 200, an absent Content-Type header, and SSE framing from the official plan Responses endpoint. No token, arbitrary header, account identity or response body was printed. The temporary diagnostic code was removed.

Commit `4c543ab` accepts a missing header only on the fixed ChatGPT Responses route after bounded SSE-prefix validation. It does not accept an explicit JSON/HTML content type, use another endpoint or change billing. The existing decoder still requires the native completion event. The regression first failed against the original guard, then passed with the fix. Seven additional regressions cover the observed response, provider/protocol isolation, invalid media types and bodies, false completion, fragmented prefixes, broken connections and cancellation.

The fixed release build and strict bundle-signature check passed. Native verification with the actual connected ChatGPT account and `gpt-5.6-sol` passed:

- Official browser reconnect, verified token exchange and account-specific model discovery, without a Mac password prompt.
- Retry of the original failed `hi`, returning a completed live reply.
- A synthetic two-turn memory check: the follow-up correctly returned `marigold`.
- A live long reply streamed incrementally, stopped with partial text retained, and accepted a subsequent completed reply.
- A fresh chat in the synthetic Job search demo folder called the read-only folder tool, received three snippets, continued through a second provider request, and cited the 18% forecast result. Opening its citation navigated to the exact My experience message. [Native capture](screenshots/native-chatgpt-live-folder.png).
- After ordinary Quit and relaunch of the same final build, history and the saved connection remained usable. A new live follow-up again returned `marigold`, without another sign-in. The app was left open on the synthetic ChatGPT connection check.

Changing an ad-hoc-signed development binary can make its previous Keychain entries inaccessible. This was observed during the diagnostic rebuild and resolved through the existing official reconnect flow, without weakening item access rules. The final app's connection is separate from the discarded diagnostic build; protected old namespaces remain retained.

## White default icon and live web search

The user-selected black-on-white logo is now the packaged default. The complete check, test and release-build commands passed again: 109 Rust tests, 25 UI tests, TypeScript, formatting and Clippy. Strict bundle signature verification passed and the bundled ICNS matched the checked-in asset. Finder Quick Look displayed the white rounded icon without an inset silver frame.

Selected the account-discovered `gpt-6.1-sol`, enabled Search the web and requested the official Rust homepage. The actual ChatGPT plan response completed with one web source, Rust Programming Language, and its public link. No external search key was configured. [Native capture](screenshots/native-chatgpt-web-search.png). The previous model's disabled control reflected Melly's conservative native-search allowlist; it did not establish that every other model lacks search.

The icon rebuild changed the ad-hoc binary and its prior Keychain item was inaccessible. The official reconnect flow restored the same account and plan permissions without a Mac password prompt or access-rule changes. Retry then completed another live web-search reply on the final build. Melly was left open on that conversation with `gpt-6.1-sol` selected. A stable distribution signing identity remains needed to address development-build credential continuity.

Provider-independent search without user-supplied keys remains a proposed Melly-owned gateway, not a deployed feature. See [research, architecture and costs](research/search-service.md).

## Chat organization and syntax colors

The organization checkpoint passed 110 Rust tests and 42 UI tests. The complete check, test and release-build commands passed; its 11.86 MiB app passed strict bundle signature verification. UI coverage includes real pointer dragging, spring-open destinations, removal to Recent, edge scrolling, Escape/blur/outside cancellation, external-drop rejection, failed moves with preserved drafts, active-stream guards, captured-target deletion, deletion errors, keyboard menus and revocation of old folder permission. Rust verifies that deleting a chat removes its messages and both full-text/folder-search entries while preserving its sibling.

The first HTML5 implementation passed Chromium tests but stalled during native Mac drags. A plain-text pasteboard probe did not fix that observed failure and was discarded. The final `useChatDrag` implementation uses an internal pointer gesture, six-pixel threshold, lightweight pointer-following preview and time-based edge scrolling. It writes no chat data to the system drag clipboard. New regressions verify that it never starts an OS drag and stops scroll work when focus is lost.

Native checks on the final WKWebView build passed: dragged the synthetic **Code colors demo** from Recent into a collapsed **Job search demo**, then to **Writing**, then back to Recent. The selected conversation stayed open and the folder tool permission reset to off on the folder change. Right-click opened the intended chat menu; Delete displayed the correct captured title, and Cancel preserved it. Confirmed permanent deletion was tested in the synthetic browser and Rust suites, not performed on native user history.

A live ChatGPT Python response was inspected with colored keywords, function names, strings, comments and numbers in both Light and Dark. Native Copy displayed Copied. Captures contain synthetic content only: [dark](screenshots/native-code-colors-dark.png), [light](screenshots/native-code-colors-light.png). Automated code tests additionally cover exact clipboard text, language aliases, measured palette contrast, unknown/large blocks, inert HTML, worker failure, streaming colors and Stop. The 92.06 kB grammar worker is a separate asset loaded only when supported code appears; it has bounded input/output and a two-second watchdog.

Verified application source: `b12be21` (including the native dragging correction `ea46233`). As with earlier ad-hoc rebuilds, final packaging required official ChatGPT reconnect with the same account and plan scopes. The synthetic follow-up completed with `6` on `gpt-5.6-terra`; after ordinary Quit and relaunch of that same build, Retry completed again without another sign-in. No Mac password prompt or credential access-rule change was involved. History and the unfiled chat persisted. Melly was left open in the user's Dark/14-point appearance with the original model selected. The fresh release startup record reported **470 ms** process-to-rendered UI on this warm-cache relaunch; this is a single local observation, not a controlled performance benchmark or a cold-launch claim.

The files/coding/knowledge-graph request remains [a research proposal](research/workspace-and-graph.md). No workspace tab, file tools, Pi/OpenCode runtime or additional OAuth flow was added.

## Simple chat menus and LaTeX

The sidebar's right-click and ellipsis menus now expose only Rename and Delete. Checkpoint `1f3d47a` was tested and pushed separately. Eleven chat-management regressions passed, including the absent folder selector, keyboard focus, active-stream guards and existing drag behavior. The header retains the keyboard-accessible Move to folder alternative. On the final Mac build, right-click visibly exposed exactly the two actions, and Rename changed the synthetic math chat to **LaTeX demo**.

The final full run passed `npm run check`, `npm test` (110 Rust and 49 UI tests), `npm run build`, and strict deep bundle-signature verification. `npm audit --omit=dev` reported zero vulnerabilities. Seven math UI regressions cover the four delimiter forms, both themes, local font loading, MathML, exact copying, code/prices, malformed/unsafe/oversized expressions, isolated macros, unavailable workers, unfinished blocks, wide-equation scrolling and streaming/Stop. An added whitespace case first failed, then passed after explicit `\( … \)` delimiters were exempted from the single-dollar price heuristic.

The native release is 12.78 MiB. Its 262.16 kB KaTeX worker, 30.25 kB stylesheet and bundled fonts load only when math appears; the production startup JavaScript was checked to exclude the renderer. The app uses local assets and keeps backend provider/persistence behavior unchanged. Dependency and font notices ship in `public/licenses/`; the fonts' separate OFL license was checked against their embedded metadata.

A live ChatGPT plan reply on `gpt-5.6-terra` rendered Euler's identity inline, a definite integral and a matrix in the native WKWebView. Both [Light](screenshots/native-math-light.png) and [Dark](screenshots/native-math-dark.png) were visually inspected; the captures contain only the synthetic prompt/reply. The final build needed the established official reconnect flow after its ad-hoc signature changed. The same account/plan permissions were used; credential access rules were not relaxed. After ordinary Quit and relaunch, the saved equations reappeared and a new live follow-up rendered `2+2=4` without signing in again. The app was left open in the user's Light/14-point appearance.

The fresh `debugBuild: false` startup record changed to **533 ms** process-to-rendered UI on that relaunch. This is a single warm-cache observation on this Mac, not a controlled cold-start benchmark. KaTeX handles supported math notation; it does not compile complete LaTeX documents. Invalid or unsupported equations stay readable as source.

## Live checks still required

ChatGPT has now completed live plan replies, folder-tool continuation and native web search on the tested account/model. The following checks remain pending; mock coverage does not substitute for them.

| Connection | Remaining live verification |
| --- | --- |
| ChatGPT Plan | Expiry/renewal, remote revocation, entitlement and usage-limit failure cases, and other account/model combinations. Native search was verified only on the tested account with `gpt-6.1-sol`. No ChatGPT history or memory is imported. |
| Claude, Gemini, Grok, OpenRouter, DeepSeek, Moonshot, Ollama Cloud, Custom | A key for the configured endpoint, live model/inference/tool behavior and account-specific billing/error/cancellation checks. Custom is text-only. Ollama's credit use must be checked on the user's actual account. |
| QwenCloud | Independently verify Standard API and Token Plan credentials, Singapore region, entitlement and billing. No cross-route fallback exists. |
| External web search | Tavily key and real results/error checks. Other provider-native search routes still need live model/auth verification. A shared Melly search service is proposed only. |
| Meta Muse Code | Disabled. The actual official host was inspected, but safe chat-only constraints and a compatible SDK/host pairing are unverified. See [precise evidence and blockers](muse.md). This requires a supported integration contract, not a Mac password. |

Saved credentials mean configuration, not live verification. No API key or token from another application was imported. Provider artwork identifies integrations, not endorsements; [asset terms](research/provider-artwork.md) record remaining distribution permissions. The local build is not a claim of permission to publish a branded product release.

## Known limits

Single ChatGPT account selection; old inaccessible Keychain namespaces are retained, and remote revocation is manual. Chat data is local SQLite with restricted file permissions, not separately encrypted. An abrupt process kill can lose the current uncommitted partial response; startup marks abandoned placeholders interrupted. Ordinary Quit returns immediately when idle; during rotating authorization work it can wait up to 65 seconds to save the result. A force quit or expired shutdown deadline cannot guarantee recovery of a remotely rotated token. The upstream `block 0.1.6` dependency used by AppKit secure input emits a future Rust compatibility notice; the current pinned build and Clippy checks pass.
