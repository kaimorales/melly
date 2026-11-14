# Workspace and graph proposal

Status: brainstorming only, 2026-11-13. No workspace tab, file access, coding host, shell tool or additional OAuth was implemented. This is a feasible direction, not a claim that a harness integration is already working.

## Product shape

Keep two primary places: **Chat** and **Workspace**. Chat retains today's quiet conversation UI and folder boundaries. Workspace opens a folder the user chooses, then shows a slim file tree, a document/editor surface and an assistant. The same typography, composer and provider picker make the transition familiar. Each place remembers its selection, draft and scroll position.

Within Workspace, start with **Ask** and **Edit** capabilities rather than adding several competing top-level pages. Ask searches selected files and cites exact file ranges. Edit proposes reviewable changes. A Graph button sits alongside the file list and backlinks; it is a view of that workspace, not the default launch screen. Selecting a node opens its file, selecting a citation opens its exact passage, and asking about a selection attaches only that explicit selection.

This supports writing and research as well as code. An existing Obsidian vault can be opened as an ordinary folder of Markdown files. Preserve its files and links without requiring conversion, uploading the entire vault, modifying `.obsidian`, loading its plugins or copying Obsidian source/branding.

## Graph before “AI memory”

The first graph should represent facts already in the files: Markdown links, `[[wikilinks]]`, backlinks, tags and unresolved targets. Obsidian's documented graph likewise visualizes notes and their internal links, with a local graph for the current note. [Official Graph view](https://obsidian.md/help/plugins/graph).

Use Rust for incremental scanning, link extraction, file watching, hashes and SQLite indexing. Start with Markdown/text/code; binary files can appear by name but are not silently parsed or sent to a model. Resolve links deterministically, retain the source range for each edge, and make duplicate basenames explicit rather than linking a random file. Folders describe containment; distinguish those edges from actual document references.

Default to a local graph around the active file with a bounded neighbor count, not thousands of animated nodes. Load graph rendering only when opened, compute layout in a worker, stop simulation when stable/hidden, and use a list/backlinks alternative for keyboard and screen-reader access. Full-workspace graph is an optional zoomed-out view with filtering and node limits. The graph is an index and navigation aid, not a reason to attach all connected files to prompts.

Later semantic links should be opt-in suggestions, visually distinct from authored links and backed by evidence. Embeddings, entity extraction and a graph database are not prerequisites for the first useful release. SQLite tables for files, links, tags and bounded text search are sufficient initially.

## Pi: promising, with a deliberate boundary

The former `badlogic/pi-mono` repository redirects to [earendil-works/pi](https://github.com/earendil-works/pi). At inspection the coding-agent package reported 1.5.2 and Node >=22.19.0. It is TypeScript/Node or Bun, not Rust. Its [MIT license](https://github.com/earendil-works/pi/blob/main/LICENSE) permits reuse with the copyright and permission notices retained; each dependency/asset also needs its own notice review before distribution.

Pi supports both an [in-process SDK](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md) and [JSONL RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/cli-integration.md). The full SDK defaults include file-backed settings/sessions and resource discovery. The [SDK source](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/sdk.ts) exposes tool/resource overrides. Its [read-tool source](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/tools/read.ts) accepts relative **or absolute** paths and calls filesystem operations. Merely setting `cwd` does not confine file access.

I would first spike **Pi's smaller agent core in a lazy sidecar**, with an explicit tool list and a custom model-stream bridge. Its [agent-core interface](https://github.com/earendil-works/pi/blob/main/packages/agent/README.md) exposes custom tools, `streamFn`, events and abort. Melly would keep its existing Rust provider adapters, Keychain access, explicit billing routes and authorization; Pi would receive normalized model events through private IPC, not another application's credentials or a copied ChatGPT token. Whether the bridge preserves every needed tool/reasoning/usage event must be proven in the spike.

If the full coding-agent SDK provides enough useful session/editing behavior to justify it, compare a custom wrapper that disables standard resource discovery, extensions, MCP, shell and default filesystem tools. Do not launch a user's installed Pi CLI with inherited config/auth and assume it is restricted. No automatic import of `~/.pi`, project plugins, environment secrets or third-party provider sessions. An isolated subprocess improves crash/cancellation handling, but it is **not by itself an OS sandbox**.

Pi's [RPC protocol](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md) distinguishes command acceptance from run completion and supports orderly shutdown. A wrapper must bound messages and outstanding calls, attach a generation/run ID, wait for actual terminal state and terminate a stuck child after a cancellation deadline. Run no Pi process while ordinary Chat is being used.

## Alternatives

| Approach | Why consider it | Main tradeoff |
| --- | --- | --- |
| Pi core/custom SDK sidecar, recommended spike | Reuse a configurable agent loop while retaining Melly's native UI and Rust boundaries | Adds a JS runtime and event/IPC bridge; package size, memory and startup need measurement |
| Small Rust agent loop | Best fit for current adapters, credentials and tool enforcement; no second runtime | Melly owns compaction, tool scheduling, recovery and session semantics |
| OpenCode embedded SDK | Rich session/worktree/plugin infrastructure | Larger conceptual integration surface; defaults and auth still need adaptation |

OpenCode's **current** [embedded SDK](https://opencode.ai/docs/build/sdk/) can host its server/router in-process without an HTTP listener; it should not be evaluated only against the older V1 client/server docs. Its runtime still needs isolation from the webview and an explicit file/credential boundary. No speed winner has been measured here. Pi's reputation for speed is a reason to benchmark it, not proof that it will make Melly faster or reduce cloud latency.

## Rust-owned workspace tools

Proposed model-facing operations are bounded `search_files`, `read_file_range`, `list_links` and `propose_patch`. Workspace IDs come from the selected session in Rust, never model arguments. Each result includes a file ID, relative path, revision hash and range for source navigation. Retrieved files remain reference material; their text cannot grant new capabilities.

Root access is granted explicitly per workspace. Enforce relative paths, descriptor-relative/no-follow file access, symlink-escape checks and revision checks in Rust, including at write time. Ignore `.git`, dependency/build trees, credentials, environment files and binaries by default; changing exclusions is an explicit user action. Renames, deletions and watcher overflow trigger reconciliation. A rescan is cancellable and does not block launch or typing.

`propose_patch` creates a staged diff, never writes immediately. The user reviews changed files and applies the patch through a native command. Before applying, compare the original hash, reject conflicts, write atomically and retain a reversible local snapshot. Do not overwrite external edits or commit/push a repository automatically. Terminal commands, package installation and network actions remain a later, separately approved capability with real sandboxing; a UI label or a prompt is not enforcement.

## Suggested sequence and acceptance gates

1. **Read-only Workspace:** choose one root, incrementally index, browse/search, ask about selected files, open exact citations. Test root escapes, symlinks, excluded files, cancellation, renamed/deleted files and large directories. Ordinary Chat never acquires these tools.
2. **Links and local graph:** explicit links/backlinks with source ranges, deterministic resolution, bounded layout and accessible list view. Test cycles, duplicate names, unresolved links, file moves and hidden-view CPU use.
3. **Harness spike:** compare Rust loop, Pi core and a restricted Pi SDK wrapper on the same synthetic tasks, model/mock streams and tool budget. Measure cold/warm workspace opening, peak/idle RSS, bytes shipped, p50/p95 IPC/tool overhead, cancellation time, context tokens and correctness. Separate cloud first-token latency from app/harness overhead. Discard the spike if it cannot preserve credential routing and tool restrictions.
4. **Reviewable edits:** patch preview, conflict detection, apply and restore; no shell initially. Test partial failures, concurrent external edits, cancellation, prompt injection and root-boundary enforcement.

Proposed performance gates, not measured results: Chat launch regresses by no more than 50 ms at p95 on the same machine; no workspace/graph/harness work on Chat startup; visible interactions stay inside a 16 ms frame budget on the reference Mac; Stop changes UI immediately and cancels work within 150 ms in controlled fixtures, with a bounded child-termination fallback. Graph layout sleeps when hidden or settled. Measure on 1k/10k/50k-file synthetic workspaces and modest hardware before setting public claims.

The decision to make later: whether the first Workspace release is **notes and files first** (recommended) or an IDE-like coding workspace immediately. The first choice gives the graph a useful foundation and validates safe reads/writes before adopting a full coding harness.
