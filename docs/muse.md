# Muse Code subscription feasibility

Checked 2026-11-13 against the official documentation, published Rust crate, and a verified official macOS host. **The Muse provider remains disabled.** No Meta sign-in, subscription purchase, credential import, or cloud inference was performed. A saved account or installed host would not establish subscription entitlement.

## Installed host and protocol evidence

The public stable channel resolved to **1.4.18-R8587.1**. Its `aarch64_macos` artifact was downloaded directly from the manifest and verified before execution:

- Location: `~/Library/Application Support/Melly/muse/1.4.18-R8587.1/muse`
- Size: 486,415,488 bytes
- SHA-256: `3f6da0d865ceb97dfe3002372ff1551b6a187dccc740866456a546510b1412e7`
- `muse --version`: `Muse Code 1.4.18 (1.4.18-R8587.1)`
- Stable MSP fingerprint: `sha256:77dda18550adef14e257b015fe4e6490b722723f7d526df5ba4867939fff68a3`
- Experimental MSP fingerprint: `sha256:43a76702a69cdfe355db9edac2c7efdc8f75ae40daaae28789ad1d6ec68c3603`

The binary is confined to Melly's support directory; no shell startup file, PATH entry, service, or app startup hook was added. Inspection used a cleared child environment, separate Melly-owned XDG config/data roots, a fresh working directory, and an explicit unused auth-file path. `HOME` and `CODEX_HOME` were not changed. No existing credential file or credential value was inspected. [Official installer](https://dev.meta.ai/install.sh), [Stable channel](https://api.meta.ai/muse-code/channels/muse-stable), [Release manifest](https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.18-R8587.1&file=manifest.json)

Offline schema export and an `echo`-provider MSP initialization succeeded. The handshake confirmed the version, fingerprints, isolated `museHome`, ephemeral sessions, and available sandbox. Only initialization and settings-schema queries were sent; no MSP session, login, or model request was started. The actual host's `SessionConfig` has no `allowedBuiltInTools` member and tolerates unknown keys, so sending that member would not prove removal. [Host operations](https://meta-models.github.io/muse-code-sdk/next/guides/host-operations/), [Session configuration](https://meta-models.github.io/muse-code-sdk/next/generated/msp/types/sessionconfig/)

On recheck, the current published experimental schema describes `allowedBuiltInTools`, but says the host refuses that option until its enforcement is complete. That future interface is not evidence that the installed host enforces it today. The `echo` execution below proves synthetic input can be processed; it does not simulate a model requesting an action or prove that extensions and action tools are absent. Do not buy a subscription solely to test this integration until those limits and host credential isolation can be demonstrated. [Experimental session configuration](https://meta-models.github.io/muse-code-sdk/next/generated/msp/types/sessionconfig/)

Offline verification commands, all exiting successfully:

```sh
MELLY_MUSE_BIN="$HOME/Library/Application Support/Melly/muse/1.4.18-R8587.1/muse"
MELLY_INSPECTION="$HOME/Library/Application Support/Melly/muse/inspection"
"$MELLY_MUSE_BIN" --version
"$MELLY_MUSE_BIN" serve --help
"$MELLY_MUSE_BIN" exec --help
"$MELLY_MUSE_BIN" schema generate-json-schema --out "$MELLY_INSPECTION/schema-stable"
"$MELLY_MUSE_BIN" schema generate-json-schema --experimental --out "$MELLY_INSPECTION/schema-experimental"
```

`MELLY_INSPECTION` was the Melly-owned `muse/inspection` directory. The echo handshake additionally used `serve --provider echo --preset empty --no-session-log --disable-shell --disable-write --sandbox-network restricted --contributor-mode off --disable-request-user-input`, then JSONL `initialize`, `initialized`, and `settings/readSchema`. Stdin EOF stopped the host with exit 0.

## Why neither route is enabled

| Route | Verified behavior | Remaining blocker |
| --- | --- | --- |
| Official Rust SDK + `muse serve` | The host accepts `--preset empty`; the 1.4.8 changelog documents no built-in tools for that preset. | The latest published crate is 1.4.14, with a different stable fingerprint. The public source mirror reports 1.4.15; neither is the installed 1.4.18 host pair. The host also lacks `exec`'s explicit reminder and foreign-personal-context exclusion flags. Complete inherited-extension suppression and rejection of unexpected model tool calls have not been verified. |
| Pinned `muse exec --json` | The actual help exposes empty preset, reminder/web-tool/input disabling, foreign-personal-context exclusion, no session log, and model-step limits. | It does not accept a piped conversation. `/dev/stdin` fails the regular-file check, no positional prompt is rejected, and `-` is a literal prompt. Melly will not pass chat history through process arguments or ordinary prompt files. |
| Experimental `model/complete` | The exported schema defines a completion without a host session or tools, a 45-second timeout, and explicit terminal outcomes. | It is not streaming, permits at most 32 messages and 1,024 output tokens, and does not resolve the supported SDK pairing and host-auth isolation checks. It is not a substitute for full chat capability. |

The Rust SDK warns on fingerprint mismatches rather than refusing them. A future integration must apply its own strict version/schema check and must not treat unknown configuration keys, a system prompt, an empty workspace, or a deny-all approval callback as proof of absent tools. [Compatibility contract](https://meta-models.github.io/muse-code-sdk/next/compatibility/), [Official Rust crate](https://crates.io/crates/muse-code-sdk/1.4.14), [Published source anchor](https://github.com/meta-models/muse-code-sdk/blob/main/publish-anchor.json), [Empty-preset release notes](https://dev.meta.ai/docs/muse-code/changelog), [Completion contract](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/model-complete/)

The actual `exec` input checks, with a synthetic string piped to stdin and all available restrictions selected, produced:

```text
--prompt-file /dev/stdin  -> exit 1: not a regular file
no positional prompt     -> exit 2: missing prompt
positional -             -> exit 0: output was "echo: -"
--prompt-file -           -> exit 1: tries to read a file named "-"
```

The echo check used no real conversation. No retained session files or synthetic-prompt matches were found in its isolated data directory. The host did create bootstrap diagnostics and unpack bundled skills/plugins, so `--no-session-log` must not be described as disabling all host disk activity. Wire records marked `durable` alone do not prove persistence; the local check found no retained session.

`-c hooks={}`, null managed hook/plugin/skill paths, and an empty runtime-capability map were accepted during an echo host launch. In contrast, `-c mcp_servers={}` was rejected because whole-map overrides are prohibited. Acceptance does not prove effective removal of every inherited or built-in capability, so no real session was used to test that assumption. The official plugin documentation says served sessions inherit the host store, and the extension documentation describes default-on observer agents and foreign personal skill roots. [Plugin inheritance](https://meta-models.github.io/muse-code-sdk/next/guides/plugins/concepts/plugins-in-sdk-sessions/), [Extension defaults](https://dev.meta.ai/docs/muse-code/extending)

## Conditions for a future connection

Use only an explicitly selected, lazy-started official host with a pinned, verified protocol. The app must enforce no built-in/action tools, inherited MCP servers, hooks, plugins, reminders, or personal context before sending a turn. Prove both empty tool advertisement and refusal of a synthetic model-requested action. Bound input, output, process lifetime, and cancellation, including child processes. Conversations must travel through a supported in-memory pipe, with host retention disabled. A separate Melly-owned host auth namespace must be demonstrated before user login; redirecting config files alone is not proof that Keychain records are isolated.

Login must remain host-owned through the documented account flow. Melly must not extract the host's token or send it directly to a Meta inference endpoint. Reject API-key and unknown effective credential lanes. Current `usage/refresh` can report `subscriptionActive`; require an explicit active result for a plan connection, and label usage with its observation time. Never infer entitlement from `accountLogin` alone or switch to pay-as-you-go after a plan error. [Host account login](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/account-loginstart/), [Credential precedence](https://dev.meta.ai/docs/muse-code/auth), [Fresh usage observation](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/usage-refresh/), [Subscription route](https://dev.meta.ai/docs/muse-code/subscriptions)

No production Muse adapter, SDK dependency, shared contract change, or enabled-provider claim was added. The installed binary and generated inspection files are outside the repository and are not bundled with Melly. Live plan access remains unverified.
