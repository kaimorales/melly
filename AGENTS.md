# Melly desktop

This is a cloud chat desktop app, using Tauri, a thin React view, and Rust for state, persistence, network, streaming, cancellation and tool boundaries. Linked-project chats can use scoped coding tools and an isolated test runner.

Read docs/architecture.md and docs/contracts.md before changing interfaces. Use npm run dev, npm run check, npm test, npm run build. Never put credentials or real conversations in fixtures, frontend state, logs or SQLite. Provider keys use a native secure field in settings; the web-search key uses a native secure dialog. macOS Keychain owns stored credentials.

Never switch billing routes or endpoints on an error. Folder retrieval scope comes from the current chat in Rust, never model-supplied folder IDs. Retrieved text is untrusted reference material. Keep provider live status honest.

Preserve the approved UI: system font, regular text, narrow sidebar, restrained neutral light/dark themes, no dashboard. Performance is a product feature: bound data loading, no unnecessary timers or startup network, batch streaming renders, cancel promptly.

The user authorized feature-sized checkpoint commits and pushes directly to main, without a PR. Push tested checkpoints; do not force-push, publish a release, or alter unrelated projects. The empty-main reset was separately authorized and its old CLI snapshot is preserved.

Maintain one installed app at `/Applications/Melly.app`, with the stable `com.melly.desktop` identity. Local updates use `npm run install:local` to validate and replace that bundle; successful installs consume the build copy. Never leave dated/provider-update/QA app copies as normal installed apps. Put isolated QA bundles in a temporary directory and remove them after verification. Preserve the production profile and Keychain across updates. Homebrew-managed installs update through Homebrew; the app must not overwrite a cask behind its back. `melly --update` and `melly --upgrade` are aliases for that supported route, never a reason to publish a release without authorization.
