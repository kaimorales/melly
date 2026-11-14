# Muse Code subscription feasibility

Updated 2026-11-14. The **experimental Muse Code Plan** row uses a private, temporary login and a pinned local host. The independently selectable **Meta Model API** row uses a metered API key for the official Responses API. The sandboxed probe completed a live text request after a separate browser login. The host confirmed an active plan and reported the account-login credential before and after the request. The allowance observation did not advance, so a quota deduction remains unverified.

In desktop Settings, choose Meta Muse Code, then **Sign in with Meta**. The experiment uses the official Apple Silicon host at `~/.local/bin/muse-bin-1.4.20-R8619.1` and verifies its SHA-256 and SDK handshake. The temporary Muse-owned login lasts until Disconnect or normal app exit. Choose Meta Model API and enter its key to use the separate metered route. [Meta Model API quickstart](https://dev.meta.ai/docs/quickstart)

In either route, the globe uses Melly's configured Tavily connection. Tavily receives the typed query. The attachment button supports one UTF-8 text file up to 24 KiB per message. Melly sends its contents for that turn over the selected provider route. Replies may quote source or attachment text and are saved in chat history. Plan replies arrive whole.

The live Rust adapter test confirmed browser authorization, an active plan, and a text reply from the pinned sandboxed host. The automated tests cover attachment and web search boundaries with synthetic data. Live Meta Model API and Tavily checks require their respective keys.

## Experimental plan check

Helicon's default macOS connection launches the installed `muse serve` in the project directory and connects over stdio. Its launch code leaves the default environment inherited, including any `META_API_KEY`. Its sign-in indicator checks for that variable or reads the Muse auth file to see whether any provider is recorded. Muse handles the actual authentication. Helicon also requests the `userShell` capability. [Host launch](https://github.com/HarjjotSinghh/helicon/blob/45da468cbdcf8a433f8d9d5c202fd34c8260ea1f/packages/server/src/server.ts#L3405-L3431), [Sign-in indicator](https://github.com/HarjjotSinghh/helicon/blob/45da468cbdcf8a433f8d9d5c202fd34c8260ea1f/packages/daemon/src/wsl.ts#L231-L260), [SDK connection](https://github.com/HarjjotSinghh/helicon/blob/45da468cbdcf8a433f8d9d5c202fd34c8260ea1f/packages/daemon/src/mspHost.ts#L66-L86)

Helicon's named profiles use Aonia. Its macOS profile flow redirects Muse's config and data roots and selects a file-backed credential store for a separate browser login. It cannot hand off the existing Keychain login to the confined Muse host. [Aonia profile design](https://github.com/HarjjotSinghh/aonia#how-it-works)

Melly's opt-in developer probe uses stdio with the pinned 1.4.20 host and sends a fixed test prompt through `model/complete` only after confirming the plan. The Helicon-style login lookup was also tested with a disposable workspace, a cleared environment, exact Muse-owned file exceptions, and Keychain metadata access. The official host still reported signed out. The test verified that the login Keychain database could not be opened for reading and stopped before sending the prompt.

```sh
python3 scripts/muse-host-probe.py --muse /path/to/verified-muse --plan-status
python3 scripts/muse-host-probe.py --muse /path/to/verified-muse --plan-test
python3 scripts/muse-host-probe.py --muse /path/to/verified-muse --plan-login-test
```

`--plan-status` checks the billing route without inference. `--plan-test` sends one fixed synthetic prompt capped at 256 output tokens. `--plan-login-test` first opens Meta's official approval page for a fresh browser login in the sandbox. The host writes its credential into a private temporary profile, which is removed after the test. This storage mode is for the one-time developer probe. The browser flow uses the existing subscription account and has no purchase step. All three modes require the host's effective `accountLogin` lane and a fresh `subscriptionActive: true`. An API key, missing entitlement, or any error stops the probe. On success the probe reports whether the host observed usage after the reply. A host-reported observation is separate from invoice verification.

The probe uses a fresh workspace and configuration, a cleared environment, contributor mode off, and a host sandbox that blocks home reads and child executables. It permits outbound TCP 443 and access to the macOS DNS resolver socket for the official host. Muse owns the credential lookup. The probe does not read auth files, copy credentials, print account details, or transmit real chat text. The mock completed and cancelled a stateless request. Four separate mock responses requesting shell, file read, file write, and web search triggered zero tool or session events and zero follow-up model requests.

On 2026-11-14, `muse usage --json` returned an allowance. A separate status-only MSP check launched the verified installed binary with normal Muse sign-in access and confirmed both `accountLogin` and `subscriptionActive: true`. No model request ran in that check. With the external sandbox in place, exact Muse-owned file exceptions and native Keychain lookup still left `account/read` at `loggedOut`. A diagnostic `usage/refresh` returned `-32081` (`usageRefreshUnavailable`). The normal probe stopped before inference. No relogin is needed when `muse usage` already shows an allowance. Direct access to the login Keychain database, broad home-directory reads, and removing the sandbox from the inference run are outside this experiment.

Run the billing-gate unit tests with `python3 -B -m unittest discover -s scripts -p test_muse_host_probe.py`.

## Installed host and protocol evidence

The initial public stable channel resolved to **1.4.18-R8587.1**. Its `aarch64_macos` artifact was downloaded directly from the manifest and verified before execution:

- Location: `~/Library/Application Support/Melly/muse/1.4.18-R8587.1/muse`
- Size: 486,415,488 bytes
- SHA-256: `3f6da0d865ceb97dfe3002372ff1551b6a187dccc740866456a546510b1412e7`
- `muse --version`: `Muse Code 1.4.18 (1.4.18-R8587.1)`
- Stable MSP fingerprint: `sha256:77dda18550adef14e257b015fe4e6490b722723f7d526df5ba4867939fff68a3`
- Experimental MSP fingerprint: `sha256:43a76702a69cdfe355db9edac2c7efdc8f75ae40daaae28789ad1d6ec68c3603`

The binary is confined to Melly's support directory; no shell startup file, PATH entry, service, or app startup hook was added. Inspection used a cleared child environment, separate Melly-owned XDG config/data roots, a fresh working directory, and an explicit unused auth-file path. `HOME` and `CODEX_HOME` were not changed. No existing credential file or credential value was inspected. [Official installer](https://dev.meta.ai/install.sh), [Stable channel](https://api.meta.ai/muse-code/channels/muse-stable), [Release manifest](https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.18-R8587.1&file=manifest.json)

The initial offline schema export and an `echo`-provider MSP initialization succeeded. That inspection sent only initialization and settings-schema queries. Later synthetic sessions and model requests are described below. The 1.4.18 host's exported `SessionConfig` has no `allowedBuiltInTools` member and tolerates unknown keys, so sending that member does not prove removal. [Host operations](https://meta-models.github.io/muse-code-sdk/next/guides/host-operations/), [Session configuration](https://meta-models.github.io/muse-code-sdk/next/generated/msp/types/sessionconfig/)

On recheck, the current published experimental schema describes `allowedBuiltInTools`, but says the host refuses that option until its enforcement is complete. That future interface is not evidence that the installed host enforces it today. Do not buy a subscription solely to test this integration until its plan route and host credential isolation can be demonstrated. [Experimental session configuration](https://meta-models.github.io/muse-code-sdk/next/generated/msp/types/sessionconfig/)

## New stable host and synthetic session checks

Meta's stable channel subsequently changed to **1.4.20-R8619.1**. Its `aarch64_macos` binary was downloaded into a temporary directory, matched the official manifest's 481,584,720-byte size and SHA-256 `89964f68f7f020896fc75679dc2f3975e5e15a6efe8537d91512a1b45ffb37aa`, and passed `codesign --verify --strict` using normal macOS signing services. Its stable schema fingerprint is `sha256:283b1b125dcc9172928fb5d6711a92a353796d030a7c186988ed527a13cf07f0`. The crates.io index now lists a matching **1.4.20** Rust SDK, removing the earlier version-pairing gap for this release. The new host was not installed into Melly or bundled with the app. [Stable channel](https://api.meta.ai/muse-code/channels/muse-stable), [Official manifest](https://lookaside.facebook.com/lookaside/muse/download/?channel=muse&version=1.4.20-R8619.1&file=manifest.json), [Rust SDK registry](https://index.crates.io/mu/se/muse-code-sdk)

The repeatable [host probe](../scripts/muse-host-probe.py) passed on both 1.4.18 and 1.4.20. It uses a fresh test directory, a known dummy file-backed credential and a loopback fake model patterned after [Meta's SDK fixture](https://github.com/meta-models/muse-code-sdk/blob/main/clients/sdk-quickstart/src/provider.ts). Before starting the host, its macOS sandbox self-check verifies that the user's home and other temporary files cannot be read, child executables cannot be launched, and outside network access is denied; only the fixture's local port is permitted. It neither changes `HOME` nor inspects a real credential. Run on macOS with a pinned, verified binary:

```sh
python3 scripts/muse-host-probe.py --muse /path/to/verified-muse
```

- With `--preset empty`, the observed main model requests advertised no tools. The fake model separately requested shell, file-read, file-write, and web-search actions; the host rejected each as an unknown tool, and the synthetic write did not occur. This demonstrates those cases for these exact host versions, not every possible tool or extension.
- The 1.4.18 host accepted the unknown `allowedBuiltInTools: []` field without proving enforcement. The 1.4.20 host refused it with `-32602`, consistent with its experimental schema. Never infer a restriction merely from field acceptance.
- Despite the empty preset, each synthetic session turn started **goal-reminder** and **verify-reminder** child agents. Their model calls included their own `submit_reminder_decision` tool. The fake model returned ordinary text instead of a reminder decision, causing retries: five test turns produced 49 fake requests. That count is a fixture behavior, not a prediction of live usage or price. The 1.4.20 `serve --help` exposes no `--disable-reminders` or `--no-foreign-personal-context` option.
- The **stable** `model/complete` method completed a three-message synthetic exchange with one tool-free fake model request and no session or reminder events. A pending completion settled `cancelled` after `model/cancel`; a pending session turn settled `cancelled` after `turn/interrupt`. These are local mock checks, not provider-side cancellation guarantees.
- `model/complete` returns the completed text rather than streaming deltas, supports at most 32 messages and 1,024 output tokens, offers no tool calls, and lets the host choose the model. Its 1.4.20 schema has only the `model/completed` notification. It cannot provide Melly's full streaming, explicit model selection or folder-tool experience.
- The fixture's `account/read` returned `apiKey`, as expected for its dummy metered-key lane. No `accountLogin` or `subscriptionActive` result was tested. The mock is not proof of subscription billing, login isolation, live model access, or streaming behavior against Meta.

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
| Official Rust SDK + `muse serve` | The 1.4.20 SDK and host are now available; the empty preset prevented the four synthetic main-session tool calls above. | Reminder child agents still ran. Full extension suppression, isolated host-owned login, effective plan billing, streamed live replies and use of an explicitly scoped Melly folder tool remain unverified. |
| Pinned `muse exec --json` | The actual help exposes empty preset, reminder/web-tool/input disabling, foreign-personal-context exclusion, no session log, and model-step limits. | It does not accept a piped conversation. `/dev/stdin` fails the regular-file check, no positional prompt is rejected, and `-` is a literal prompt. Melly will not pass chat history through process arguments or ordinary prompt files. |
| Stable `model/complete` | The host completed and cancelled synthetic, tool-free mock requests without session or reminder events. The live test completed under an active-plan login in a temporary profile. | Replies arrive whole, with at most 32 input messages and 1,024 output tokens. The host chooses the model. The live test did not observe a quota deduction. |

The Rust SDK warns on fingerprint mismatches rather than refusing them. A future integration must apply its own strict version/schema check and must not treat unknown configuration keys, a system prompt, an empty workspace, or a deny-all approval callback as proof of absent tools. [Compatibility contract](https://meta-models.github.io/muse-code-sdk/next/compatibility/), [Official Rust crate](https://crates.io/crates/muse-code-sdk/1.4.20), [Empty-preset release notes](https://dev.meta.ai/docs/muse-code/changelog), [Completion contract](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/model-complete/)

The actual `exec` input checks, with a synthetic string piped to stdin and all available restrictions selected, produced:

```text
--prompt-file /dev/stdin  -> exit 1: not a regular file
no positional prompt     -> exit 2: missing prompt
positional -             -> exit 0: output was "echo: -"
--prompt-file -           -> exit 1: tries to read a file named "-"
```

The echo check used no real conversation. No retained session files or synthetic-prompt matches were found in its isolated data directory. The host did create bootstrap diagnostics and unpack bundled skills/plugins, so `--no-session-log` must not be described as disabling all host disk activity. Wire records marked `durable` alone do not prove persistence; the local check found no retained session.

`-c hooks={}`, null managed hook/plugin/skill paths, and an empty runtime-capability map were accepted during an echo host launch. In contrast, `-c mcp_servers={}` was rejected because whole-map overrides are prohibited. Acceptance does not prove effective removal of every inherited or built-in capability; subsequent sessions used only a fake provider in the restrictive sandbox above. The official plugin documentation says served sessions inherit the host store, and the extension documentation describes default-on observer agents and foreign personal skill roots. [Plugin inheritance](https://meta-models.github.io/muse-code-sdk/next/guides/plugins/concepts/plugins-in-sdk-sessions/), [Extension defaults](https://dev.meta.ai/docs/muse-code/extending)

## Conditions for a future connection

Use only an explicitly selected, lazy-started official host with a pinned, verified protocol. The app must enforce no built-in/action tools, inherited MCP servers, hooks, plugins, reminders, or personal context before sending a turn. Prove both empty tool advertisement and refusal of a synthetic model-requested action. Bound input, output, process lifetime, and cancellation, including child processes. Conversations must travel through a supported in-memory pipe, with host retention disabled. A separate Melly-owned host auth namespace must be demonstrated before user login; redirecting config files alone is not proof that Keychain records are isolated.

Login must remain host-owned through the documented account flow. Melly must not extract the host's token or send it directly to a Meta inference endpoint. Reject API-key and unknown effective credential lanes. Current `usage/refresh` can report `subscriptionActive`; require an explicit active result for a plan connection, and label usage with its observation time. Never infer entitlement from `accountLogin` alone or switch to pay-as-you-go after a plan error. [Host account login](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/account-loginstart/), [Credential precedence](https://dev.meta.ai/docs/muse-code/auth), [Fresh usage observation](https://meta-models.github.io/muse-code-sdk/next/generated/msp/methods/usage-refresh/), [Subscription route](https://dev.meta.ai/docs/muse-code/subscriptions)

Meta describes SDK-driven desktop clients, while its subscription documentation restricts the plan credential to Muse Code and its terms restrict it to the applicable coding harness. Whether a general-purpose chat product using the official `muse serve` host qualifies is not established by those documents. Ask Meta before representing this as a supported plan integration. A separate Meta Model API key is a distinct pay-as-you-go route, not a subscription fallback. Meta also says Muse Code subscriptions default to Contributor mode; an integration must keep that mode explicit and never silently enable it. [SDK overview](https://meta-models.github.io/muse-code-sdk/), [Subscription documentation](https://dev.meta.ai/docs/muse-code/subscriptions), [Model API terms](https://dev.meta.ai/legal/terms-of-service)

Questions for Meta: (1) Does a general-purpose desktop chat app driving the official host through its SDK qualify for Muse Code plan usage? (2) How can a client isolate its host-owned login and verify the effective plan lane before any inference? (3) How can a served session suppress all reminder agents and inherited extensions while retaining streaming and explicitly selected read-only tools? (4) Can `model/complete` guarantee an exact model and stream its result? Do not send these questions automatically.

The developer probe can also run separately from the desktop app. The installed binary and generated inspection files live outside the repository.
