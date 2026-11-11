use std::{ffi::OsStr, path::Path, process::Stdio};

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct Rpc {
    _process: Child,
    input: ChildStdin,
    output: Lines<BufReader<ChildStdout>>,
}

impl Rpc {
    pub fn connect(executable: &OsStr, cwd: &Path) -> Result<Self> {
        let mut process = Command::new(executable)
            .arg("app-server")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "Could not launch {} app-server",
                    executable.to_string_lossy()
                )
            })?;
        let input = process.stdin.take().context("Missing Codex stdin")?;
        let output = BufReader::new(process.stdout.take().context("Missing Codex stdout")?).lines();
        Ok(Self {
            _process: process,
            input,
            output,
        })
    }

    pub async fn receive(&mut self) -> Result<Option<Value>> {
        while let Some(line) = self.output.next_line().await? {
            if !line.trim().is_empty() {
                return serde_json::from_str(&line)
                    .context("Invalid Codex JSON-RPC message")
                    .map(Some);
            }
        }
        Ok(None)
    }

    pub async fn send(&mut self, message: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(message)?;
        line.push(b'\n');
        self.input
            .write_all(&line)
            .await
            .context("Could not send a request to Codex")
    }
}
