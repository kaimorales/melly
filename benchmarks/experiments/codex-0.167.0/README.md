# Experimental public-fork runtime sources

This is a source-only, private-lab reproduction package. It is **not installed in
Melly**, a production permission policy, or evidence of a coding performance win.
No binaries, account data, runtime identifiers, raw traces, or private manifests
are included. Keep generated artifacts and traces outside this repository.

Upstream is [OpenAI Codex](https://github.com/openai/codex), tag
`rust-v0.167.0`, commit `086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a`.
The upstream code is Apache-2.0; its unmodified license and attribution are
[LICENSE-CODEX](LICENSE-CODEX) and [NOTICE-CODEX](NOTICE-CODEX). The patches mark
the experimental modifications to that source; they are not upstream releases.
The RPC adapter is Melly benchmark code, copied from the private experiment.

## Separate changes

| Artifact | Purpose |
| --- | --- |
| `public-live-fork-history.patch` | Correctness repair for a public fork taken while its parent has a pending custom/code-mode call. Adds a child-only `aborted` output at the interrupted boundary using the existing deterministic output ID function. Leaves parent history, original IDs, usage records, completed outputs, native fork behavior, and the normalizer assertion intact. Includes regression tests. |
| `cache-routing.patch` | Extends the existing ephemeral public-fork parent cache route to persisted public forks by removing the `config.ephemeral` guard. Keeps the native-agent exclusion and separate child identity/storage. Includes cache-policy, identity, persistence, and usage assertions. |
| `lock-normalization.patch` | Changes 171 local workspace lockfile versions from `0.0.0` to the release manifest's `0.167.0`, allowing `--locked` builds. All 1,308 external package records remain unchanged. |
| `rpc_tap.py` | Benchmark-only App Server proxy: pins High effort and converts fresh Melly worker `thread/start` requests to public `thread/fork` of the identified live conductor. |

Both matched runtime variants must contain the history fix and lock normalization.
Build the control with the original `config.ephemeral` cache guard and the patched
arm without it. Keeping both arms' other source, test additions, compiler, build
profile, dependencies, and helper identical isolates that single runtime guard.
This is a matched OSS runtime control, not the installed release binary.

## Apply and build in a disposable lab

The patches use paths relative to the upstream repository root. Set `EXPERIMENT`
to this directory, then prepare a separate checkout; do not patch an installed
runtime or a checkout serving a running trial.

```sh
EXPERIMENT="$(pwd)"  # Run from this experiment directory.
LAB="$(mktemp -d)"
git clone --branch rust-v0.167.0 --single-branch https://github.com/openai/codex.git "$LAB/source"
test "$(git -C "$LAB/source" rev-parse HEAD)" = 086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a
git -C "$LAB/source" apply --check "$EXPERIMENT/lock-normalization.patch" "$EXPERIMENT/public-live-fork-history.patch" "$EXPERIMENT/cache-routing.patch"
git -C "$LAB/source" apply "$EXPERIMENT/lock-normalization.patch" "$EXPERIMENT/public-live-fork-history.patch" "$EXPERIMENT/cache-routing.patch"
```

The tested platform was macOS ARM64, using upstream's pinned Rust/Cargo 1.95.0,
LLVM 22.1.2, two build jobs, and unoptimized `dev`/`test` profiles with debug
information disabled. This is not a release-profile comparison. Prepare an
isolated dependency cache before the offline build; use the same cache and flags
for both variants. Provisioning dependencies/toolchains may require network access.

Code mode requires the correct Codex-built V8 archive **and matching bindings**.
Follow this tag's `third_party/v8/README.md` and
`scripts/codex_package/v8.py`: verify the downloaded checksum manifest against
the checked-in `rusty_v8_152_2_0_release_manifests.sha256`, then verify both assets.
For the tested `rusty-v8-v152.2.0` macOS ARM64 pair:

- Manifest SHA256: `5cebc5830127b460ae8af23004430fd70b535bb22d599d68d024b60df08400d8`.
- `librusty_v8_ptrcomp_sandbox_release_aarch64-apple-darwin.a.gz`:
  `feab9ae0384bebaa99c1d000654462c9d42123d587185c7876aaeef7d76ff749`.
- `src_binding_ptrcomp_sandbox_release_aarch64-apple-darwin.rs`:
  `b9d5652ebca515eb2bf4c2ee4a6551b2567117b5aa720f430d2032616dd7e3b5`.

Set `RUSTY_V8_ARCHIVE` and `RUSTY_V8_SRC_BINDING_PATH` to those verified local
files. Set `CARGO_HOME` to the prepared lab cache and use the provisioned pinned
toolchain, then run:

```sh
export CARGO_TARGET_DIR="$LAB/target"
export CARGO_BUILD_JOBS=2 RUST_MIN_STACK=8388608
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export STABLE_GIT_COMMIT=086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a
cd "$LAB/source/codex-rs"
cargo +1.95.0 test --locked --offline -p codex-core --lib interrupted_fork -- --nocapture
cargo +1.95.0 test --locked --offline -p codex-core --test all suite::prompt_cache_key:: -- --nocapture
cargo +1.95.0 build --locked --offline -p codex-app-server -p codex-cli --bin codex-app-server --bin codex
cargo +1.95.0 build --locked --offline -p codex-code-mode-host --bin codex-code-mode-host
mkdir -p "$LAB/variants/patched" "$LAB/variants/control"
cp "$CARGO_TARGET_DIR/debug/codex" "$CARGO_TARGET_DIR/debug/codex-app-server" "$CARGO_TARGET_DIR/debug/codex-code-mode-host" "$LAB/variants/patched/"
```

For the control, change only this exact guard in
`codex-rs/core/src/session/session.rs` (leave the surrounding comments and tests
identical):

```diff
-                if !session_configuration.session_source.is_non_root_agent() =>
+                if config.ephemeral
+                    && !session_configuration.session_source.is_non_root_agent() =>
```

Repeat the same CLI/App Server build command and copy its two binaries into
`$LAB/variants/control/`. Copy the **same already-built** code-mode host from
`$LAB/variants/patched/` into the control directory. Restore the patched guard in
the source afterward. Freeze the bundle copies, record their hashes and build
settings privately, and select those copies explicitly; Cargo's mutable target
contains the last-built arm. The new persistent-cache expectation intentionally
does not pass under the control's original guard.

`codex-code-mode-host` must be beside each selected CLI/App Server binary; a
successful `command/exec` smoke alone does not exercise it. Verify actual host
handshake, session creation, JavaScript plus a nested tool callback, and clean
shutdown without a model before a coding trial. Also check CLI configuration,
App Server initialization/read-only command execution, and apply-patch helper
dispatch. macOS requires `rg` on PATH or in a managed bundle; other platforms
need their upstream sandbox companions too. Do not use a launcher that silently
falls back to an installed CLI for helpers.

## Use the existing benchmark runner

Create a private copy of Melly's `benchmarks/` and `tests/` directories, preserving
the runner's relative layout. Replace **only that copy's** `benchmarks/rpc_tap.py`
with this adapter and keep it executable. Leave fixture, contract tests, holdout
checks, prompts, model settings, and quality-audit rules unchanged. Follow the
prerequisites in the [benchmark protocol](../../README.md), including its stock
conductor profile. Freeze and identify the Melly executable used for the trial.

The runner selects its sibling adapter using `MELLY_CODEX_BIN`; it supplies the
chosen `--codex` path through `MELLY_BENCH_CODEX` and a private trace path through
`MELLY_BENCH_RPC_LOG`. The adapter launches that exact executable and forwards
CLI/helper arguments. It contains no installed-runtime fallback.

Example invocations from the private runner copy, after explicitly choosing to
spend model quota (these are live runs, unlike patch checks/builds):

```sh
python3 benchmarks/compare.py --arm melly --repeats 1 --timeout 600 --codex "$LAB/variants/patched/codex" --melly "$FROZEN_MELLY" --output "$LAB/results/melly"
python3 benchmarks/compare.py --arm stock --repeats 1 --timeout 600 --codex "$LAB/variants/control/codex" --melly "$FROZEN_MELLY" --output "$LAB/results/stock"
```

Use separate fresh output roots: each invocation writes a top-level manifest and
results summary. Preserve both; do not overwrite one arm's metadata. Record the
actual runtime hashes, arm order, adapter hash, and fork experiment label beside
those private outputs: the existing runner's generic protocol label does not
describe this extra intervention. Alternate arm order across repetitions. The
stock arm does not use the adapter. A Melly-versus-stock pair compares complete
workflows; isolating the cache guard itself requires the same Melly+fork adapter
under both runtime variants. A single incomplete or incorrect arm is not a win.

## Adapter behavior and limits

- The adapter recognizes the current conductor/worker developer-instruction
  prefixes, remembers the conductor's returned thread ID, and refuses worker
  conversion if it has no parent or sees unexpected `thread/start` keys. It is
  pinned to this Melly/App Server protocol, not a generic fork middleware.
- It converts fresh worker starts (read or edit) and leaves existing worker
  resumes as resumes. `excludeTurns=true` suppresses turns in the RPC response;
  it does **not** filter inherited model history. This is full public-fork context,
  not native selective child context or a production context-handoff design.
- It preserves submitted worker configuration, role instructions, subsequent
  prompts, and tool results; it pins thread/turn effort to High. Forks still
  inherit parent history and dynamic tool definitions. New role instructions do
  not erase earlier conductor messages. Melly remains responsible for its
  read-only conductor, Luna edit workers, delegation/turn/concurrency caps,
  rejection of recursive delegation, and explicit review. The proxy adds no
  permission or policy enforcement of its own.
- The adapter's `workspace-write` / `never` settings match this isolated benchmark
  only. They are **not production permission defaults**. They do not provide
  filesystem enforcement of each worker's owned-path brief; retain the harness
  controls and independent diff/quality checks. Do not apply them to general user
  sessions or change global configuration.
- Public forks retain inherited cumulative usage and transcript items. The
  benchmark must count per-response `token_usage_record` entries owned by the
  actual thread, exclude inherited records by `thread_id`, and deduplicate
  response IDs. It must include all worker-owned records, including forked
  workers. The existing runner does this; inherited totals are not new usage.
  This does not fix Melly's production UI accounting or history projection.
- Live parent cache affinity is not persisted through cold resume; resumed
  children follow upstream child-session routing. Resume, compaction, UI history
  boundaries, and per-agent totals are not production-qualified by this package.
- Raw adapter logs contain RPC messages and may contain account metadata, source,
  prompts, tool output, and real identifiers. Store them outside the repository
  and publish only independently reviewed aggregate evidence.

Offline validation before packaging covered a red-then-green pending-custom-call
regression, seven focused history tests, seven combined-source cache tests, and
zero-model runtime/code-mode smokes for both matched bundles. This publication
checks patch application and source consistency, without running new models or
building binaries. Neither those checks nor this source package establishes
end-to-end coding correctness, speed, or subscription savings.
