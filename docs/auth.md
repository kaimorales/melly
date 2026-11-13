# Credentials and ChatGPT sign-in

Implemented against the official OpenAI documentation checked on 2026-11-13 using the OpenAI Docs skill. The local tests use synthetic values and signing keys generated only in memory. No live account authorization or plan inference has been attempted.

## Credential boundary

`credentials::KeychainStore` uses the macOS generic-password service `app.melly.desktop.credentials`. There is no file, SQLite, environment, browser-storage, or other-application credential fallback. `MemorySecrets` is the public in-memory test double and redacts its debug representation. Authentication records do not implement `Debug`.

Provider keys are entered through `src-tauri/src/secure_input.rs`: AppKit `NSAlert` with `NSSecureTextField`, dispatched through `AppHandle::run_on_main_thread`. Only Rust receives the value. The command caller writes it to the vault and returns success/cancel to the frontend. Empty input is treated as cancel; error messages omit input values.

Melly never requests the Mac login password. Secret reads, writes, updates and deletes explicitly use Security.framework's `kSecUseAuthenticationUIFail` policy; a protected or inaccessible item produces a sanitized error instead of an authentication dialog. This policy also covers the legacy login Keychain used by local ad-hoc-signed builds. Presence checks request attributes only and skip authentication-requiring items. A vault failure cannot prevent history, settings or Demo from opening. There is no interactive retry or plaintext fallback. [Apple authentication-UI policy](https://developer.apple.com/documentation/security/ksecuseauthenticationuifail)

## App-owned authorization

Melly persists a fresh host UUID in Keychain and opens the system browser through a Rust callback. First authorization uses `client_id=dynamic_agent_client` and `agent_name_hint=Melly` at `https://auth.openai.com/api/accounts/authorize`. The validated loopback callback supplies the issued `oaiapp_…` registration, which is saved before code exchange. Later attempts reuse that registration. This is the documented dynamic-registration entrypoint; it is not a separate registration API. [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)

Every attempt binds an available IPv4 loopback port at `/auth/callback` before opening the browser, creates fresh random state/nonce/PKCE material, and uses the exact callback URI for authorization and exchange. Callback state, host, port, path, expiry, duplicates, and returning client identity are checked. Browser replies contain only generic text. The attempt expires after five minutes and supports cancellation.

The native browser handoff validates the exact official authorization origin/path, cancels the listener if browser launch fails, and permits an immediate retry. Both initial and terminal connection events are published under a generation check held through delivery, so a disconnected or older attempt cannot overwrite a newer attempt's status.

The requested scopes are `openid profile email offline_access resource.invoke chatgpt.tokens.use.direct`, with resource `https://api.openai.com/v1`. Identity and inference permission remain separate. Identity-only completion is retained, produces a visible plan-permission error, and cannot supply an inference token. An explicit retry after identity-only consent requests `prompt=consent`; ordinary reauthorization does not force consent. [Consent recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery)

Tokens are exchanged only at `https://auth.openai.com/api/accounts/oauth/token`. OpenID discovery and JWKS URLs are pinned to the documented OpenAI values. The JWT verifier checks an allowed asymmetric signature and key usage, issuer, client audience, expiration, issued-at, nonce, and returning subject. Unsupported metadata, HTTP redirects, oversized responses, and invalid identities fail closed. Network/error messages retain safe status/code/request-ID details and omit request and response bodies. [Identity verification and discovery](https://developers.openai.com/siwc/website)

## Renewal and connection status

The token record holds the verified identity, issued client ID, granted scopes, tokens, and expiry together in Keychain. Refresh is serialized within the desktop process and replaces the entire token record in one vault write. Once requested, bounded credential renewal finishes independently of a chat cancellation, so stopping a reply cannot discard a rotated replacement. The application must run one instance to avoid competing refresh tokens across processes. Missing scope on refresh preserves the existing grant; an explicitly reduced grant disables inference. Terminal refresh errors clear tokens; temporary network/server errors retain them. Disconnect invalidates pending writes so a late result cannot restore tokens. [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)

Startup uses `has_saved_plan`, a metadata-only presence check for the nonsecret `chatgpt.saved-plan.v1` marker. It never calls the credential-value getter or validates a token. The marker is written only after a verified plan grant is saved, and removed on identity-only consent, scope loss, terminal renewal failure, or disconnect. Its meaning is **saved plan connection, checked on use**; expiry and current entitlement are deliberately unknown at startup. Records predating the marker are not migrated by decrypting credentials during launch.

`is_connected` is a deeper helper that reads credentials and checks for an unexpired access token or an unexpired renewable session; it must not run during bootstrap. Neither status is live API verification. `access_token` renews near expiry, honors `earliest_refresh_at`, and is the only way the chat route receives an OAuth token. A successful refresh renews the documented 30-day refresh lifetime. Expired refresh credentials, absent scopes, mismatched registrations, and malformed records require sign-in. [Token lifetimes](https://developers.openai.com/siwc/token-sharing-open-source/token-reference)

The current command contract supports one saved ChatGPT registration. Disconnect clears its local tokens and retains its registration and host ID for reauthorization. Remote revocation is not confirmed by this synchronous operation; users can remove Melly's access in [ChatGPT settings](https://chatgpt.com/settings/usage). A multi-account picker and remote-revocation workflow are not implemented.

## Verification and pending checks

`cargo test -p melly-core auth --lib` covers authorization construction, callback/state guards, signed identity claims and key-use validation, exact consent scopes, token rotation/concurrency, temporary/terminal refresh errors, disconnect races, expiry status, marker lifecycle, real local loopback completion, HTTP error sanitization/redirect refusal, and cancellation. `cargo test -p melly-core --test auth` covers vault isolation/redaction, malformed/empty credentials, cancellation before browser launch, metadata-only startup status with a getter that panics, and disconnect preservation of unrelated keys and registration.

The opt-in `native_keychain_roundtrip_without_authentication_ui` test passed on the Mac: a UUID-named synthetic credential was created, read, updated and deleted without authentication UI. Normal tests use an in-memory vault.

Pending live checks: eligible user/workspace consent, actual OpenAI discovery/JWKS and token exchange, model discovery, completed Responses inference on the plan route, and renewal/revocation with the real service. No live-ready claim follows from the offline tests. The provider adapter owns model/inference checks and must never switch to API-key billing after a plan-route error.
