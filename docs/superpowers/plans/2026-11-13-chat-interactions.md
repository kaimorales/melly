# Chat organization and code readability

The user explicitly authorized implementation and routine decisions on main. Files/coding/knowledge-graph work is a research proposal only. Execute inline with feature-sized, tested checkpoints.

## Design

Use browser-native dragging inside WKWebView, without a drag framework. A chat carries only its ID; an in-app drag session must match before a drop is accepted. Highlight a destination folder, spring open a collapsed folder after 450 ms, scroll only while dragging at the sidebar edge, and support removing a folder assignment by dropping onto Recent. Keep the selected conversation unchanged. Rust's existing move command is authoritative; active streams cannot move.

Expose a quiet ellipsis on hover/focus and a right-click menu on every chat. Reuse rename/move/delete actions with the header menu. Delete a captured chat ID after confirmation, keep errors inside that dialog, and preserve the selected conversation when deleting another chat. Do not add trash or change the existing permanent-delete contract.

Render fenced code using a lazy, bounded syntax worker and safe text/span tokens. Include common programming languages, language labels and a copy-code control. Unknown languages and oversized blocks stay readable as plain text. Stream updates never wait on highlighting. Colors follow the active theme; no language detection or HTML execution.

## Checkpoints

- [ ] Chat organization: add `tests/ui/chat-management.spec.ts` regressions for real drag/drop, collapsed destinations, removal to Recent, external-drop rejection, failed moves, background deletion, cancellation and active-stream guards. Run them against current code and verify missing-affordance failures. Implement `src/ChatList.tsx`, `src/ChatActions.tsx`, focused App callbacks and sidebar CSS. Keep existing Rust commands, add a storage deletion regression for message/search removal, and align the browser mock's active-run guard. Run UI tests and Rust checks. Commit and push this feature alone.
- [ ] Code readability: add `tests/ui/code-blocks.spec.ts` for token colors in both themes, aliases, exact copy, untrusted markup, unsupported languages, large blocks and streaming/Stop. Verify the current plain renderer fails the highlighting assertions. Implement `src/CodeBlock.tsx`, `src/syntax.ts`, `src/syntax.worker.ts` and a focused palette. Keep syntax work off the UI thread, bound input to 32 KiB and tokens to 6,000, use a watchdog and discard stale work. Run checks/tests/build and inspect native light/dark rendering. Commit and push separately.
- [ ] Research only: inspect official Pi SDK/CLI/tool/license sources and compare an isolated Pi sidecar with a Rust agent loop and OpenCode server. Document a workspace-scoped Rust file boundary, explicit links/backlinks, lazy graph, reviewable writes, credential routing, cancellation and measurable performance gates. Do not enable file writes, shell tools, extra OAuth or new tabs. Commit the proposal separately.

## Verification

Run `npm run check`, `npm test`, `npm run build` and strict bundle signature verification. Exercise native dragging, source-scope updates and the delete confirmation on synthetic chats. Inspect syntax colors and copy in light/dark. Preserve real chats and credentials. UI tests cover confirmed deletion; native checking can cancel its dialog. Record actual native evidence and any unverified claims in `docs/verification.md`.
