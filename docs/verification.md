# Current verification — November 15, 2026

The daily-use work now passes 200 Rust tests (zero failed; five opt-in tests ignored), 84 browser UI tests, seven installer regressions, TypeScript, Rust formatting and all-target Clippy. The current source is committed and pushed directly to main in feature checkpoints. Historical environment restrictions and test counts below describe earlier builds and no longer describe this session.

Coverage includes journal restart/conflict recovery and Undo; draft text and attachment ownership; real Python, Node and offline Cargo checks in disposable projects; denied network/outside writes; test cancellation/timeouts; native tool schemas and continuation; paged large-file search; changed/missing reference handling; context compaction and cancellation; and light/dark narrow-window UI behavior. Pytest was separately exercised in an opt-in disposable virtual environment. Standard test runs skip live models and platform-dependent OCR/semantic service checks.

A local synthetic Qwen3.8-27B coding smoke test used four native tool calls, edited a disposable Python fixture, and reached independently verified passing tests in 10.3 seconds. A separate synthetic compaction request retained an exact key and pending task in 6.47 seconds, preserving all twelve original messages and four recent raw messages. These are smoke checks, not 32K/64K coding benchmarks or evidence for untested cloud providers. No real repository content or credentials were sent to a cloud model.

The 14.75 MiB ad-hoc Mac app was built, strictly signature-verified, installed at `/Applications/Melly.app` and reopened with the existing profile. The native management commands passed without opening a profile. An unsent synthetic draft was saved, verified in SQLite, restored after quitting and reopening, and cleared afterward; no chat message was sent. The context ring exposed Compact context in the native app. The current conversation and history stayed in place.

The macOS CI workflow runs checks, synthetic tests, packaging and local bundle verification. Its remote status is separate from the completed local checks. Developer ID signing and notarization remain blocked by the absent signing identity/credentials. No release, Homebrew tap or feed was published. Finder file-drop gestures on the final build and the historical cloud-provider entitlement/billing cases remain unverified. Upstream `block 0.1.6` still emits a future-compatibility notice; current compilation and Clippy pass.

## Historical records


## One installed app and terminal updates — 2026-11-14

Consolidated the current production build at `/Applications/Melly.app`, retaining `com.melly.desktop`. Removed the two obsolete provider-update bundles and the isolated `MellyQA.app`; moving the current build into Applications consumed the build copy. A scan of Desktop, user/system Applications and this checkout's target directory found only the installed production bundle afterward. Opened that installed app natively and verified existing history remained available. No profile or Keychain records were removed.

The existing `~/.local/bin/melly` was the retired terminal app. Moved it to `target/retired-cli/melly` as a recoverable backup, verified its SHA-256, and replaced the command with a symbolic link to `/Applications/Melly.app/Contents/MacOS/melly-desktop`. Both the app and command now use the same binary. The installed command passed `--version` and `--help`; both update aliases correctly refused this unmanaged installation with local-update instructions and a nonzero exit. Unknown options also exit nonzero. Actual Homebrew metadata was readable and contained no installed Melly cask; no package was upgraded or installed.

`npm run install:local` now validates, stages and replaces the production app, rolls back a failed promotion, consumes the successful build artifact, and retains the existing profile. It refuses running apps, conflicting identities and Homebrew ownership. The Homebrew cask template and preparation command generate versioned, checksummed metadata without publishing anything. `melly --update` and `melly --upgrade` delegate only to the intended installed Homebrew cask; they do not implement a second updater or change channels.

Validation for this checkpoint:

- `npm run check`: passed TypeScript, Rust formatting and all-target Clippy.
- Selected Rust suite: **134 passed, zero failed, two ignored, 35 excluded**, using the same socket-free selection documented below. Four new CLI regressions cover alias parsing, the running-app gate, missing/wrong cask ownership and propagation of Homebrew failures.
- `npm run test:install`: **six passed**, covering replacement, rollback, staged-signature failure, running-app rejection, path aliases, conflicting launchers, persistent launcher targets and cask metadata validation.
- First install and replacement of the actual signed bundle passed in a temporary destination using real `ditto` and signature inspection. This isolated check supplied the running-state function; it does not establish an end-to-end run of the system installer or its process lookup in this restricted runner. The temporary bundles were removed.
- `npm run build`: passed, producing a 13.93 MiB local ad-hoc-signed app. Strict signature verification passed before and after its move to Applications. The final installed launcher target and retained old CLI hash were checked directly.

The native running-app lookup and a real Homebrew-managed upgrade still need distribution-environment verification; the running-app gate is unit-tested. Full browser/network suites remain blocked as described below and were not represented as passing. No tap, cask, release or public update feed was published. Git metadata remains read-only in this session, so these changes are saved as pending recovery patches rather than commits or pushes. Evidence is in `target/feature-checkpoints`: `update-check.log`, `update-rust-tests.log`, `install-tests.log`, `update-build.log`, `update-native-cli.log`, `app-cleanup.json` and `launcher-cleanup.json`.

## Attachments, continuity, portable data and usage — 2026-11-14

Implemented managed-file replacement/removal, Branch/Edit, native chat picker/drop handling, text/PDF/image OCR attachments, bounded context selection, same-chat earlier references, optional on-device semantic ranking, context/token UI, a usage ledger, explicit OpenRouter key usage, Markdown/project exports and additive backup/restore. The context ring sits beside the model selector; per-connection Usage & costs stays in Settings. Images provide recognized text only. Subscription percentages stay unavailable when the provider does not expose an allowance API.

### Automated checks

- `npm run check`: passed TypeScript, Rust formatting and all-target Clippy with warnings denied.
- Selected Rust suite: **130 passed, zero failed, two ignored, 35 excluded**. The exclusions are the auth/transport fixture modules and one loopback-dependent auth-namespace case, including some tests that do not themselves bind sockets. The ignored tests are the opt-in Keychain roundtrip and shell-only Vision integration. Native Vision was separately verified below.
- `node scripts/test-thinking.mjs`: passed. This checks rendering/state markup, not browser interaction.
- Full `npm test`: attempted; localhost fixture binding failed with `Operation not permitted`. A shutdown case then waited without completing and was stopped. The separate Playwright attempt failed because Vite could not bind `127.0.0.1:1420`. These suites did **not** pass. New browser cases in `tests/ui/continuity-usage.spec.ts` remain unrun here.
- New regressions cover one-use native drop tickets, failed-batch cleanup, attachment ownership across branch/delete/restart, export/restore identity and citation rewriting, replacement/removal boundaries, earlier-chat scope, synthetic vector ranking, usage-only fragmented SSE, cumulative counters, unknown/estimated/reported costs, usage admission and cancellation, oversized-draft preflight, and oversized tool/native-continuation metadata.

Logs are under `target/feature-checkpoints`: `final-next-check.log`, `rust-next-tests.log`, `thinking-next-check.log`, `full-next-tests.log` and `ui-next-tests.log`. The socket-free selected command is shown in the preceding-work record below and remains reproducible.

### Native checks

Used a separately identified `MellyQA.app` and isolated profile with synthetic files/conversations and the installed Ollama model `lfm2.5-8b-abliterated:latest`:

- Branch and Edit created new conversations. A revised 42-to-43 prompt returned 43 while the original conversation stayed intact. Branches and replies survived ordinary Quit/relaunch.
- The context dialog displayed an approximate input estimate and the reported local capacity of 8,192. Provider-reported input/output totals appeared in both the chat and the connection panel; they remained distinct from the input estimate.
- Native file selection imported a synthetic README and attached it to a saved message. A fresh conversation’s live reply used the extracted port 4343. The file picker remained responsive and cancellation worked.
- PNG and scanned-PDF attachments both produced the synthetic receipt amount 42 and reference 4343 through Apple Vision. Ollama used the extracted text; the completed follow-up returned “42 dollars.” One earlier attempt exhausted the 1,024-token output budget during reasoning. The final code raises the local reserve to 2,048 at 8,192 capacity; that change has a regression assertion and a production build, but was not rerun natively.
- Exported an isolated Aurora project containing two chats and two managed files, then restored it through the native picker and explicit count confirmation. Both projects remained available. The restored project’s context was off. Its rewritten PDF citation opened the correct second-page December 17 passage.

The native walkthrough found an Open-panel configuration issue. Explicit Cocoa Boolean selection flags and file-type reset fixed it; subsequent text/image/PDF selections worked. Final hardening also prevents file-drop acceptance behind dialogs, counts tool metadata before every provider round, and retains unknown ledger entries when a process exits before usage arrives. Native walkthrough evidence predates those final hardening changes.

The Finder-to-chat drag gesture itself is still unverified. Ticket validation and the shared import backend passed automated tests; picker success does not establish native drop delivery. Full-backup attachment roundtrips have Rust coverage; the native walkthrough used a project export. Managed-file removal/replacement and deletion recovery have Rust coverage; native destructive project deletion was not performed. Apple’s English sentence-embedding asset was unavailable on this machine, so native semantic-ranking quality remains unverified; keyword fallback and synthetic ranking/scope checks passed. New cloud cost/account-allowance calls remain fixture-tested, without live billing verification.

### Performance and delivery

The synthetic in-memory SQLite probe used 10,000 messages and 100 samples: schema open 2.576 ms; insert p50/p95 0.097/0.185 ms; folder search 15.820/17.562 ms; 60-message history page 0.083/0.186 ms. This measures storage operations, not native launch, disk I/O, OCR, semantic ranking or provider latency.

`npm run build` produced the 13.91 MiB ad-hoc-signed production bundle at `target/release/bundle/macos/Melly.app`. Strict `codesign --verify --deep --strict` verification passed. No release was published. The working tree retains concurrent provider-settings/artwork changes. This session cannot create `.git/index.lock`, so **no feature commits or pushes were made**. Numbered recovery patches are saved under `target/feature-checkpoints`; the module groups and final integration patch are not independently tested commits. Complete the unrestricted suite and reconstruct the feature commits before pushing.

The older entries below are historical and do not establish verification of this final tree.

## Completed-thinking indicator follow-up — 2026-11-14

The nine-square activity mark now disappears when reasoning ends, including answer streaming, completion, Stop, error and interruption. Completed reasoning remains available through the text disclosure. The existing markup checks now require the indicator to be absent in those states. `npm run check`, `node scripts/test-thinking.mjs`, the production rebuild and strict bundle-signature verification pass. The Playwright assertions were updated but remain unrun under this session's browser restrictions. The native walkthrough below predates this small display follow-up.

## Projects, local Ollama and Thinking — 2026-11-14

Implemented managed project directories and imports, text/PDF retrieval with citations, archive/restore and recoverable Trash-based project deletion, remembered project context across providers, read-only linked folders, and a distinct local Ollama connection. Existing folder IDs and chats migrate without a destructive schema rewrite. File editing, command execution, OCR and background folder watching are not implemented.

Readable provider reasoning now has a separate persisted channel and muted, expandable Thinking section. Leading `<think>` / `<thinking>` blocks are separated even when tags cross stream chunks. The waiting indicator is a small monochrome 3×3 grid with a clockwise opacity pulse; motion is CSS-only and respects both app and system reduced-motion settings. Thinking opens during reasoning, collapses when the answer starts unless the user chose otherwise, and stays out of copied answers, chat search and subsequent model context. Existing stored replies are not rewritten.

`npm run check` passes TypeScript, Rust formatting and Clippy with warnings denied. The selected Rust run passes **114 tests**, including 7 project integration tests, 19 engine tests, 3 local-provider tests and 10 desktop tests. One opt-in Keychain test remains ignored. The command excludes 35 tests in total: auth/transport fixture modules plus one loopback-dependent auth-namespace case; this selection also excludes some tests that do not themselves open sockets. The native PDFKit test verifies text and page extraction from a synthetic PDF. Linked-folder tests cover exclusions, cancellation, changed files, root identity and symlink/path boundaries; deletion tests cover Trash failure and retry after an interrupted transaction. Reasoning regressions cover fragmented tags and UTF-8, native thinking fields, mixed reasoning/answer chunks, encrypted-metadata exclusion, output limits, Stop/error, persistence and context/search exclusion. `node scripts/test-thinking.mjs` also passes component markup/state checks; it is not a browser interaction test. Fixtures contain no real credentials or conversations.

```sh
node scripts/rust.mjs test --workspace -- --skip auth::tests --skip providers::transport::tests --skip rebuilt_app_can_open_browser_without_reading_denied_active_or_legacy_records
```

Native WKWebView testing used the installed `lfm2.5-8b-abliterated:latest` model through Ollama Local. The new display was first checked in the normal app with synthetic arithmetic prompts; subsequent project tests used a separately identified `MellyQA.app` and isolated application profile. Verified directly:

- New replies keep reasoning in Thinking, with no `<think>` block in the answer. Waiting and reasoning grid states, manual collapse during streaming, Stop with retained partial reasoning, Retry, auto-collapse on answer, reopening completed reasoning, and answer-only Copy worked. Reasoning and answers survived ordinary Quit and relaunch. Light and Dark were visually inspected; the app's reduced-motion preference was exercised in Light.
- Project creation, text import, two-page PDF import, native picker cancellation, managed files in Finder, and read-only folder linking worked. A hidden synthetic `.env` was skipped. Text retrieval returned the synthetic 18% fact; a sibling chat inherited Project context and retrieved December 17 from PDF page 2. The citation dialog and native PDF viewer opened that exact page.
- Linked README retrieval returned port 4242. Editing the synthetic source caused the old citation to show a changed-file notice. Explicit Refresh indexed port 4343, and the next live reply used it. Unlink removed the link while the external README and hidden fixture stayed intact.
- Archive hid the project; Restore retained its chats, imports and link. The delete confirmation clearly distinguished permanent chat deletion, managed files moved to Trash, and untouched linked files. Cancel preserved the project. Actual deletion/recovery is covered by Rust tests; irreversible native chat deletion was not performed.

Native testing found a main-loop freeze after selection in the original blocking file picker. File, project-import and linked-folder pickers now use an asynchronous AppKit sheet. Repeated cancellation, text/PDF selection and folder selection passed in the rebuilt isolated QA app. The original already-frozen process could not be terminated under this session's process permissions; if it is still open, force-quit that older instance before opening the rebuilt app.

This session denies localhost socket binding. `npm test`, the Playwright suite and `npm run dev` therefore cannot complete here; their results must not be represented as passing. A socket-free Chromium attempt was also blocked by the session's browser-process/MachPort restrictions. New UI coverage is in `tests/ui/projects.spec.ts` and `tests/ui/thinking.spec.ts`, but those browser tests remain unrun here. Cloud reasoning routes are fixture-tested, not newly live-tested. The final `npm run build` produced the 13.36 MiB production bundle at `target/release/bundle/macos/Melly.app`, and `codesign --verify --deep --strict` passed. The isolated native QA bundle is at `target/release/bundle/macos/MellyQA.app`.

Git cannot create `.git/index.lock` in this session, so **these features are not committed or pushed**. Incremental feature patches are saved under `target/feature-checkpoints`, including separate Thinking and native-picker follow-ups. They preserve this work separately from concurrent provider-settings/artwork changes. The patches are pending checkpoints, not independently verified Git commits; verify the complete suite in an unrestricted session before committing/pushing.

The older records below describe their own earlier builds and do not establish verification of these changes.

Local verification on 2026-11-13, Apple Silicon macOS. The desktop uses Rust and the system WKWebView. No product release was published. Git checkpoints are pushed to `main` as requested.

## Provider setup and Ollama Cloud follow-up

The current full run passed `npm run check`, `npm test` (**128 Rust tests and 59 Playwright UI tests**), and `npm run build`. One opt-in native Keychain roundtrip remained ignored in the automated suite. The Rust tests cover isolated billing routes, folder boundaries, stream cancellation, Ollama catalog/request shapes and mock tool continuation; they do not prove live provider access.

The native provider key field was exercised in an isolated application profile with a synthetic key: typing, saving and removing it, keyboard navigation, Escape, provider switching and both appearances were checked. The final code was also inspected in that isolated app: the key placeholder is centered, the desktop starts with Connect provider instead of Demo, and Ollama Cloud shows its API-key and credit route in Light and Dark. The synthetic key was removed. No real provider credentials or chats were used for these checks.

The 12.89 MiB production bundle passed `codesign --verify --deep --strict` and was copied to `target/provider-update-20261113b/Melly.app`. The user's previous Melly build was left running; the production bundle itself was not launched against the user's profile. Quit the old app before opening this copy. Its ad-hoc signature may require the normal ChatGPT reconnect, as with prior development builds. No Mac password or Keychain access-rule change is required by this update.

Ollama Cloud uses the documented direct cloud API, not a local runtime. Ollama's current pricing says included plan credits are used before any purchased credits. Account entitlement, billing and inference are still pending a live Ollama key. Muse remains disabled; see [the host verification](muse.md).

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
- Startup never decrypts credentials or performs model discovery. A credential vault failure leaves history usable. The mock provider remains available to developer tests; it is hidden in the desktop product. Native secret input never passes through React, and HTTP errors omit response bodies and secrets.
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
| Meta Muse Code | Disabled. Matching 1.4.20 host and Rust SDK releases now exist; isolated fake-provider checks rejected several action tools, but reminder agents, host-owned login isolation and live subscription billing still need resolution. See [precise evidence and blockers](muse.md). This requires a supported integration contract, not a Mac password. |

Saved credentials mean configuration, not live verification. No API key or token from another application was imported. Provider artwork identifies integrations, not endorsements; [asset terms](research/provider-artwork.md) record remaining distribution permissions. The local build is not a claim of permission to publish a branded product release.

## Known limits

Single ChatGPT account selection; old inaccessible Keychain namespaces are retained, and remote revocation is manual. Chat data is local SQLite with restricted file permissions, not separately encrypted. An abrupt process kill can lose the current uncommitted partial response; startup marks abandoned placeholders interrupted. Ordinary Quit returns immediately when idle; during rotating authorization work it can wait up to 65 seconds to save the result. A force quit or expired shutdown deadline cannot guarantee recovery of a remotely rotated token. The upstream `block 0.1.6` dependency used by AppKit secure input emits a future Rust compatibility notice; the current pinned build and Clippy checks pass.
