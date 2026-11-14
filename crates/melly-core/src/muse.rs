//! Experimental subscription-only Muse chat over the pinned host's stdio.
use crate::models::{ProviderRequest, WireMessage};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex as AsyncMutex,
};
use tokio_util::sync::CancellationToken;

// The confined host uses a private temp profile so it never touches the
// existing Muse login. Melly never reads or exports the credential file.
const HOST_NAME: &str = "muse-bin-1.4.20-R8619.1";
const HOST_DIGEST: &str = "89964f68f7f020896fc75679dc2f3975e5e15a6efe8537d91512a1b45ffb37aa";
const PROTOCOL: &str = "sha256:283b1b125dcc9172928fb5d6711a92a353796d030a7c186988ed527a13cf07f0";
const FRAME_BYTES: u64 = 256 * 1024;
const MAX_MESSAGES: usize = 32;

#[derive(Default)]
pub struct Muse {
    profile: Mutex<Option<Arc<Profile>>>,
    operation: AsyncMutex<()>,
}

struct Profile {
    directory: tempfile::TempDir,
    cancel: CancellationToken,
    home: PathBuf,
}

impl Profile {
    fn prepare(home: PathBuf) -> Result<Self> {
        ensure!(cfg!(target_os = "macos"), "Muse requires macOS");
        let directory = tempfile::Builder::new()
            .prefix("melly-muse-")
            .tempdir_in("/private/tmp")
            .context("Could not create a private Muse profile")?;
        let root = directory.path().canonicalize()?;
        let mut source = std::fs::File::open(home.join(".local/bin").join(HOST_NAME))
            .with_context(|| format!("Install Muse Code {HOST_NAME} to connect this experiment"))?;
        ensure!(source.metadata()?.is_file(), "Invalid Muse installation");
        let mut binary = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join("muse"))?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0; 128 * 1024];
        use std::io::Write;
        let mut remaining = 481_584_720usize;
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            ensure!(
                count <= remaining,
                "Muse host does not match the tested release"
            );
            remaining -= count;
            digest.update(&buffer[..count]);
            binary.write_all(&buffer[..count])?;
        }
        ensure!(
            format!("{:x}", digest.finalize()) == HOST_DIGEST,
            "Muse host does not match the tested release"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            binary.set_permissions(std::fs::Permissions::from_mode(0o700))?;
        }
        for component in ["config", "data", "cache", "tmp", "workspace"] {
            std::fs::create_dir(root.join(component))?;
        }
        Ok(Self {
            directory,
            cancel: CancellationToken::new(),
            home,
        })
    }
}

impl Muse {
    pub fn connected(&self) -> bool {
        self.profile.lock().unwrap().is_some()
    }

    pub fn disconnect(&self) {
        if let Some(profile) = self.profile.lock().unwrap().take() {
            profile.cancel.cancel();
        }
    }

    pub async fn connect(
        &self,
        home: PathBuf,
        approve: impl FnOnce(&str, &str) -> Result<()>,
        cancel: CancellationToken,
    ) -> Result<()> {
        let _operation = self.operation.try_lock().context("Muse is busy")?;
        ensure!(!self.connected(), "Muse is already connected");
        let profile = tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("Muse sign-in cancelled"),
            result = tokio::task::spawn_blocking(move || Profile::prepare(home)) => Arc::new(result??),
        };
        let auth = async {
            let mut host = Host::open(&profile).await?;
            ensure!(
                host.rpc("account/read", json!({})).await?["state"] == "loggedOut",
                "Muse's new profile already has a credential"
            );
            let login = host
                .rpc("account/loginStart", json!({"type":"deviceCode"}))
                .await?;
            let (url, code) = browser_approval(&login)?;
            approve(url, code)?;
            let done = host
                .notification("account/loginCompleted", None, Duration::from_secs(240))
                .await?;
            ensure!(
                done["outcome"] == "granted",
                "Muse browser sign-in did not complete"
            );
            host.check_plan().await?;
            Ok::<(), anyhow::Error>(())
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("Muse sign-in cancelled"),
            result = auth => result?,
        }
        let mut connection = self.profile.lock().unwrap();
        ensure!(!cancel.is_cancelled(), "Muse sign-in cancelled");
        *connection = Some(profile);
        Ok(())
    }

    pub async fn complete(&self, request: &ProviderRequest) -> Result<String> {
        let profile = self
            .profile
            .lock()
            .unwrap()
            .clone()
            .context("Connect Muse in Settings")?;
        let _operation = self.operation.lock().await;
        let complete = async {
            let mut host = Host::open(&profile).await?;
            host.check_plan().await?;
            let command = uuid::Uuid::now_v7().to_string();
            let args = completion_args(request, &command)?;
            let ack = host.rpc("model/complete", args).await?;
            ensure!(
                ack["status"] == "accepted" && ack["commandId"] == command,
                "Muse did not accept the request"
            );
            let done = host
                .notification("model/completed", Some(&command), Duration::from_secs(50))
                .await?;
            ensure!(
                done["outcome"] == "completed",
                "Muse could not complete the reply"
            );
            host.check_account().await?;
            let text = done["text"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .context("Muse returned an empty reply")?;
            Ok(text.to_owned())
        };
        tokio::select! {
            biased;
            _ = profile.cancel.cancelled() => bail!("Muse was disconnected"),
            result = complete => result,
        }
    }
}

fn browser_approval(login: &Value) -> Result<(&str, &str)> {
    let url = login["verificationUrl"]
        .as_str()
        .context("Muse returned no sign-in URL")?;
    let target = url::Url::parse(url)
        .ok()
        .context("Invalid Muse sign-in URL")?;
    ensure!(
        target.scheme() == "https"
            && target.port_or_known_default() == Some(443)
            && target.username().is_empty()
            && target.password().is_none()
            && matches!(
                target.host_str(),
                Some(
                    "auth.meta.com"
                        | "dev.meta.ai"
                        | "meta.com"
                        | "work.meta.com"
                        | "www.facebook.com"
                        | "www.meta.com"
                )
            ),
        "Muse returned an unrecognized sign-in destination"
    );
    let code = login["userCode"]
        .as_str()
        .context("Muse returned no sign-in code")?;
    ensure!(
        (4..=20).contains(&code.len())
            && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "Muse returned an invalid sign-in code"
    );
    Ok((url, code))
}

fn completion_args(request: &ProviderRequest, command: &str) -> Result<Value> {
    ensure!(
        request.tools.is_empty() && !request.web_search,
        "Muse accepts only plain chat"
    );
    let mut recent: Vec<&WireMessage> = request.messages.iter().rev().take(MAX_MESSAGES).collect();
    ensure!(
        recent.first().is_some_and(|m| m.role == "user"),
        "Muse needs a user message"
    );
    while recent.last().is_some_and(|m| m.role != "user") {
        recent.pop();
    }
    ensure!(
        recent
            .iter()
            .all(|m| matches!(m.role.as_str(), "user" | "assistant")
                && m.tool_call_id.is_none()
                && m.tool_calls.is_empty()
                && m.claude_continuation.is_none()),
        "Muse cannot continue a tool session"
    );
    let messages: Vec<_> = recent
        .iter()
        .rev()
        .map(|m| json!({"role":m.role,"text":m.content}))
        .collect();
    Ok(
        json!({"commandId":command,"messages":messages,"system":request.system_prompt,"maxTokens":1024}),
    )
}

struct Host {
    // Dropping the request future kills the host, including on Stop and timeout.
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
    pending: Vec<Value>,
}

impl Host {
    async fn open(profile: &Profile) -> Result<Self> {
        let root = profile.directory.path().canonicalize()?;
        let binary = root.join("muse");
        let directory = serde_json::to_string(&root)?;
        let executable = serde_json::to_string(&binary)?;
        let home = serde_json::to_string(&profile.home)?;
        let sandbox = format!(
            r#"(version 1)
            (allow default)
            (deny file-read-data)
            (allow file-read-data (subpath {directory}) (subpath "/System") (subpath "/usr/lib")
                (subpath "/usr/share") (subpath "/private/etc")
                (literal "/") (literal "/private") (literal "/private/tmp")
                (literal "/Library") (literal "/Library/Preferences/com.apple.networkd.plist")
                (literal "/dev/null") (literal "/dev/urandom") (literal "/dev/random"))
            (deny file-read* (subpath "/Users") (subpath {home}) (subpath "/Volumes")
                (subpath "/private/var/folders"))
            (allow file-read* (subpath {directory}))
            (deny file-write*)
            (allow file-write* (subpath {directory}) (literal "/dev/null"))
            (deny process-exec)
            (allow process-exec (literal {executable}))
            (deny network-inbound)
            (deny network-outbound)
            (allow network-outbound (remote tcp "*:443"))
            (deny network-outbound (remote ip "localhost:*"))
            (allow network-outbound (remote unix-socket (path-literal "/private/var/run/mDNSResponder")))
            (deny mach-lookup (global-name "com.apple.SecurityServer")
                (global-name "com.apple.securityd.xpc")
                (global-name "com.apple.securityd")
                (global-name "com.apple.secd"))"#
        );
        let mut child = Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &sandbox])
            .arg(&binary)
            .args([
                "serve",
                "--provider",
                "meta",
                "--preset",
                "empty",
                "--no-session-log",
                "--disable-shell",
                "--disable-write",
                "--sandbox-network",
                "restricted",
                "--contributor-mode",
                "off",
                "--disable-request-user-input",
            ])
            .current_dir(root.join("workspace"))
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("TMPDIR", root.join("tmp"))
            .env("TBH_CREDENTIAL_BACKEND", "file")
            .env("TBH_DISABLE_TELEMETRY", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("Could not start Muse in the macOS sandbox")?;
        let input = child.stdin.take().context("Muse input unavailable")?;
        let output = BufReader::new(child.stdout.take().context("Muse output unavailable")?);
        let mut host = Self {
            _child: child,
            input,
            output,
            next_id: 0,
            pending: Vec::new(),
        };
        let init = host.rpc("initialize", json!({
            "clientInfo":{"name":"melly","version":"1.0"},
            "capabilities":{"experimentalApi":true,"requestedCapabilities":[],"userInputDialogs":false}
        })).await?;
        ensure!(
            init["serverInfo"]["version"] == "1.4.20"
                && init["schema"]["fingerprint"] == PROTOCOL
                && init["sessionDurability"] == "ephemeral"
                && init["museHome"]
                    .as_str()
                    .is_some_and(|home| Path::new(home).starts_with(&root))
                && init["grantedCapabilities"]
                    .as_array()
                    .is_some_and(Vec::is_empty),
            "Muse did not start with the expected isolated configuration"
        );
        host.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}))
            .await?;
        Ok(host)
    }

    async fn check_account(&mut self) -> Result<()> {
        let account = self.rpc("account/read", json!({})).await?;
        ensure!(
            account["state"] == "accountLogin" && account["credentialRequired"] == true,
            "Muse requires the experimental browser login. Reconnect in Settings."
        );
        Ok(())
    }

    async fn check_plan(&mut self) -> Result<()> {
        self.check_account().await?;
        let usage = self.rpc("usage/refresh", json!({})).await?;
        ensure!(
            usage["subscriptionActive"] == true,
            "Muse did not confirm an active subscription"
        );
        self.check_account().await
    }

    async fn send(&mut self, body: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&body)?;
        ensure!(
            bytes.len() < FRAME_BYTES as usize,
            "Muse input is too large"
        );
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(5), self.input.write_all(&bytes)).await??;
        Ok(())
    }

    async fn rpc(&mut self, method: &str, args: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":args}))
            .await?;
        let response = self
            .read_until(Duration::from_secs(25), |frame| frame["id"] == id)
            .await?;
        ensure!(
            response.get("error").is_none(),
            "Muse could not complete {method}"
        );
        response
            .get("result")
            .cloned()
            .context("Muse sent an invalid response")
    }

    async fn notification(
        &mut self,
        method: &str,
        command: Option<&str>,
        duration: Duration,
    ) -> Result<Value> {
        let frame = self
            .read_until(duration, |frame| {
                frame["method"] == method
                    && command.is_none_or(|id| frame["params"]["commandId"] == id)
            })
            .await?;
        frame
            .get("params")
            .cloned()
            .context("Muse sent an invalid notification")
    }

    async fn read_until(
        &mut self,
        duration: Duration,
        matches: impl Fn(&Value) -> bool,
    ) -> Result<Value> {
        if let Some(index) = self.pending.iter().position(&matches) {
            return Ok(self.pending.remove(index));
        }
        tokio::time::timeout(duration, async {
            loop {
                let mut bytes = Vec::new();
                (&mut self.output)
                    .take(FRAME_BYTES)
                    .read_until(b'\n', &mut bytes)
                    .await?;
                ensure!(
                    bytes.last() == Some(&b'\n') && bytes.len() < FRAME_BYTES as usize,
                    "Muse closed the connection or sent an oversized frame"
                );
                let frame: Value = serde_json::from_slice(&bytes)
                    .ok()
                    .context("Muse sent an invalid frame")?;
                if let Some(method) = frame["method"].as_str() {
                    ensure!(
                        frame.get("id").is_none()
                            && !method.starts_with("session/")
                            && !method.starts_with("turn/")
                            && !method.starts_with("item/"),
                        "Unexpected Muse tool or session activity"
                    );
                }
                if matches(&frame) {
                    return Ok(frame);
                }
                if matches!(
                    frame["method"].as_str(),
                    Some("account/loginCompleted" | "model/completed")
                ) {
                    ensure!(
                        self.pending.len() < 4,
                        "Unexpected Muse completion notifications"
                    );
                    self.pending.push(frame);
                }
            }
        })
        .await
        .context("Muse took too long to respond")?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Connection;

    #[tokio::test]
    async fn disconnected_host_cannot_use_a_saved_api_key() {
        let muse = Muse::default();
        let request = ProviderRequest {
            connection: Connection {
                provider_id: "muse".into(),
                base_url: "muse://local".into(),
                model_id: "host-default".into(),
            },
            system_prompt: String::new(),
            messages: vec![WireMessage::text("user", "Hello")],
            tools: vec![],
            web_search: false,
        };
        assert!(
            muse.complete(&request)
                .await
                .unwrap_err()
                .to_string()
                .contains("Connect Muse")
        );
        let sink = Arc::new(|_| panic!("Muse Plan must never reach HTTPS transport"));
        assert!(
            crate::providers::stream(
                request,
                Some("synthetic-key".into()),
                sink,
                CancellationToken::new()
            )
            .await
            .is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    #[ignore = "requires an installed Muse host, browser approval and an active plan"]
    async fn installed_host_connects_and_answers_through_the_native_adapter() {
        let Ok(home) = std::env::var("HOME") else {
            panic!("Home directory unavailable");
        };
        let muse = Muse::default();
        muse.connect(home.into(), |url, code| {
            let status = std::process::Command::new("/usr/bin/open").arg(url).status()?;
            ensure!(status.success(), "Could not open Meta sign-in");
            if !url::Url::parse(url)?.query_pairs().any(|(_, value)| value == code) {
                let script = format!("display dialog \"Enter this code on the Meta page: {code}\" with title \"Melly Muse test\" buttons {{\"OK\"}} giving up after 235");
                std::process::Command::new("/usr/bin/osascript")
                    .args(["-e", &script])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
            }
            Ok(())
        }, CancellationToken::new()).await.expect("Muse plan sign-in");
        let request = ProviderRequest {
            connection: Connection {
                provider_id: "muse".into(),
                base_url: "muse://local".into(),
                model_id: "host-default".into(),
            },
            system_prompt: "Answer in one word.".into(),
            messages: vec![WireMessage::text("user", "Say hello.")],
            tools: vec![],
            web_search: false,
        };
        assert!(
            !muse
                .complete(&request)
                .await
                .expect("Muse plan completion")
                .is_empty()
        );
        muse.disconnect();
        assert!(!muse.connected());
    }
}
