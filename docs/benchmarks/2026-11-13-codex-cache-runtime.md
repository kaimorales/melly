# Codex cache routing experiment

This experiment found a reusable mechanism, not a coding-speed win. Stock Codex's native agents already share cache routing. Matching that behavior for public persistent forks does not establish an advantage over stock.

## Source and control

We built [Codex rust-v0.167.0](https://github.com/openai/codex/tree/rust-v0.167.0), commit `086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a` (Apache-2.0), in a disposable checkout. The production change removes only the `config.ephemeral` condition from the existing public-fork cache-routing branch in `codex-rs/core/src/session/session.rs`. The native-child exclusion remains. Actual thread IDs, persistent rollout storage, permissions, and owned usage records remain separate. The patch does not restore the shared route after a cold child resume.

Control and patched executables used the same Rust 1.95.0 compiler, dependencies, and unoptimized development profile. The upstream lock required normalizing 171 local workspace version entries; all 1,308 external package records stayed identical. V8 artifacts were checked against the source's release hashes. Seven cache-routing tests passed, including separate persistent identity and exact owned usage accounting. Both variants passed CLI/App Server/helper smoke checks. Nothing replaced the installed Codex or Melly executable.

## Live probe

Each arm creates one Sol High parent and two sequential Luna High siblings forked from its unchanged history. The task is a fixed-table lookup, with no tools or coding. Every parent and worker response is counted from its owning thread; inherited records are excluded. There were 24 total model calls: 12 exploratory installed-versus-development calls, followed by 12 same-build-profile controlled calls. All arms completed with correct markers, required settings, and complete accounting.

Controlled worker cached tokens / input tokens:

| Order | Runtime | First sibling | Second sibling |
|---|---|---:|---:|
| Forward | Control | 12,416 / 44,067 | 0 / 44,067 |
| Forward | Patched | 0 / 39,779 | 39,552 / 39,779 |
| Reverse | Control | 39,552 / 39,779 | 12,416 / 39,779 |
| Reverse | Patched | 0 / 39,779 | 39,552 / 39,779 |

Patched second siblings consistently cached 99.4% of their input. However, the reverse-order control cached more tokens across its two workers than the patched arm. Already-warm caches matter. These results do not establish universal cost savings, latency savings, or superiority to native agents. Live outbound routing headers were not captured; the source and mock tests verify the routing change.

The forward control also had a prompt confound: its host-skills catalog contained 149 entries versus 99, adding 18,207 characters. Ten of eleven visible pre-response text components matched exactly; only `host_skills.instructions` differed. Input was 4,288 tokens larger per response. Both thread creation and turn submission explicitly selected Sol, with no observed model-switch message. The catalog's reason for changing is unknown. Operational validity therefore does not imply prompt equality. The reverse arms have matching catalogs and input counts.

## Implications for Melly

- Keep the runtime patch experimental. Persistent fork integration additionally needs owned-history/usage boundaries and correct cold-resume routing. Do not use ephemeral workers to hide missing accounting or storage.
- Melly's existing 3,000-token lead and 1,200-token worker skill-catalog budgets already apply to this catalog. They limit metadata, not explicitly loaded skill bodies or total model input. Native role handling and public per-thread configuration differ, so equal requested settings alone are not proof of equal prompts.
- The supported per-thread `skills.include_instructions=false` hides automatic catalogs while retaining explicit skill mentions/inputs. It also removes automatic discovery for that worker. We have not shipped this as an optimization; relevant skills must remain available and inherited fork history may still contain earlier catalog text.
- A future fresh-worker cache-affinity API could share the stable prefix without copying the conductor's whole history. That would require both request-body cache keys and Responses session routing, plus persistence. It is a hypothesis, not an implemented or measured result.

The public [source's skill catalog implementation](https://github.com/openai/codex/tree/rust-v0.167.0/codex-rs/ext/skills/src) and [core skills integration tests](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/core/tests/suite/skills_extension.rs) support these configuration findings. Sanitized measurements and executable hashes are in [the companion results](../../benchmarks/results/2026-11-13-cache-runtime.json). Raw transcripts, account data, and local executable artifacts remain private.
