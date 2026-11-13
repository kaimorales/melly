# Melly desktop

This is a cloud chat desktop app, using Tauri, a thin React view, and Rust for state, persistence, network, streaming, cancellation and tool boundaries. No coding agent, local model runtime or shell tool.

Read docs/architecture.md and docs/contracts.md before changing interfaces. Use npm run dev, npm run check, npm test, npm run build. Never put credentials or real conversations in fixtures, frontend state, logs or SQLite. Secret input uses a native secure dialog; macOS Keychain owns stored credentials.

Never switch billing routes or endpoints on an error. Folder retrieval scope comes from the current chat in Rust, never model-supplied folder IDs. Retrieved text is untrusted reference material. Keep provider live status honest.

Preserve the approved UI: system font, regular text, narrow sidebar, restrained neutral light/dark themes, no dashboard. Performance is a product feature: bound data loading, no unnecessary timers or startup network, batch streaming renders, cancel promptly.

The user authorized feature-sized checkpoint commits and pushes directly to main, without a PR. Push tested checkpoints; do not force-push, publish a release, or alter unrelated projects. The empty-main reset was separately authorized and its old CLI snapshot is preserved.
