use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub(crate) struct Suite {
    runner: PathBuf,
    args: &'static [&'static str],
    label: &'static str,
}

impl Suite {
    pub(crate) fn detect(root: &Path) -> Result<Self> {
        ensure!(
            cfg!(target_os = "macos"),
            "The isolated test runner requires macOS"
        );
        if root.join("package.json").is_file() {
            let runner = std::env::var_os("PATH")
                .into_iter()
                .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
                .filter(|dir| dir.is_absolute() && !dir.starts_with(root))
                .map(|dir| dir.join("node"))
                .find_map(|candidate| {
                    candidate
                        .canonicalize()
                        .ok()
                        .filter(|path| path.is_file() && !path.starts_with(root))
                })
                .context("Install Node to run this project's Node test suite")?;
            return Ok(Self {
                runner,
                args: &["--test"],
                label: "node --test",
            });
        }
        let runner = std::fs::canonicalize("/Library/Developer/CommandLineTools/usr/bin/python3")
            .or_else(|_| {
                std::fs::canonicalize("/Applications/Xcode.app/Contents/Developer/usr/bin/python3")
            })
            .context("Apple Command Line Tools (Python 3) are required to run Python tests")?;
        let (args, label): (&[&str], _) = if root.join("tests").is_dir() {
            (
                &["-B", "-m", "unittest", "discover", "-s", "tests", "-v"],
                "python3 -m unittest discover -s tests",
            )
        } else {
            (
                &["-B", "-m", "unittest", "discover", "-v"],
                "python3 -m unittest discover",
            )
        };
        Ok(Self {
            runner,
            args,
            label,
        })
    }

    pub(crate) fn label(&self) -> &str {
        self.label
    }

    #[cfg(target_os = "macos")]
    pub(crate) async fn run(self, project: &Path, cancel: CancellationToken) -> Result<Value> {
        use std::{os::unix::process::CommandExt, process::Stdio, time::Duration};
        const POLICY: &str = r#"
(version 1)
(deny default)
(allow process-exec (literal (param "RUNNER")) (subpath "/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework") (subpath "/Applications/Xcode.app/Contents/Developer/Library/Frameworks/Python3.framework"))
(allow process-fork)
(allow sysctl-read)
(allow file-read-metadata)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo") (global-name "com.apple.lsd.mapdb"))
(allow file-read* (subpath "/System") (subpath "/usr/lib") (subpath "/usr/bin") (literal "/") (literal "/private")
  (subpath "/Library/Developer/CommandLineTools") (subpath "/Applications/Xcode.app")
  (subpath (param "PROJECT")) (subpath (param "SCRATCH")) (literal (param "RUNNER"))
  (literal "/private/etc/localtime") (literal "/dev/null") (literal "/dev/random") (literal "/dev/urandom"))
(deny file-read-data (regex (param "EXCLUDED")))
(allow file-write* (subpath (param "SCRATCH")) (literal "/dev/null"))
(deny network*)
"#;
        ensure!(!cancel.is_cancelled(), "Test run cancelled");
        let temp = tempfile::tempdir()?;
        let scratch = temp.path().canonicalize()?;
        let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
        for (key, value) in [
            ("PROJECT", project),
            ("SCRATCH", scratch.as_path()),
            ("RUNNER", self.runner.as_path()),
        ] {
            let mut define = std::ffi::OsString::from(format!("{key}="));
            define.push(value);
            command.arg("-D").arg(define);
        }
        let project = project.to_str().context("Project path must be UTF-8")?;
        let mut boundary = String::from("^");
        for character in project.chars() {
            if ".^$[](){}*+?|\\".contains(character) {
                boundary.push('\\');
            }
            boundary.push(character);
        }
        boundary.push_str("/([^/]*/)*([.]|[^/]*([Ss][Ee][Cc][Rr][Ee][Tt]|[Cc][Rr][Ee][Dd][Ee][Nn][Tt][Ii][Aa][Ll]))");
        command.arg("-D").arg(format!("EXCLUDED={boundary}"));
        command
            .args(["-p", POLICY])
            .arg(&self.runner)
            .args(self.args)
            .current_dir(project)
            .env_clear()
            .env("HOME", &scratch)
            .env("TMPDIR", &scratch)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .context("Could not launch the isolated test runner")?;
        let group = ProcessGroup(child.id().context("Missing test process ID")? as i32);
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let result = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("Test run cancelled"),
            result = tokio::time::timeout(Duration::from_secs(90), async {
                tokio::try_join!(child.wait(), capture(stdout), capture(stderr))
            }) => result.context("The test run timed out after 90 seconds")??,
        };
        drop(group);
        let (status, stdout, stderr) = result;
        let combined = format!("{stdout}\n{stderr}");
        let no_tests = combined.contains("Ran 0 tests")
            || combined.lines().any(|line| line.trim() == "# tests 0");
        Ok(
            json!({"command":self.label, "passed":status.success() && !no_tests, "no_tests_found":no_tests,
            "exit_code":status.code(),"stdout":stdout,"stderr":stderr}),
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) async fn run(self, _: &Path, _: CancellationToken) -> Result<Value> {
        anyhow::bail!("The isolated test runner requires macOS")
    }
}

#[cfg(target_os = "macos")]
struct ProcessGroup(i32);

#[cfg(target_os = "macos")]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[cfg(target_os = "macos")]
async fn capture(mut read: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<String> {
    use tokio::io::AsyncReadExt;
    const LIMIT: usize = 4096;
    let mut chunk = [0u8; 2048];
    let mut output = Vec::new();
    let mut truncated = false;
    loop {
        let bytes = read.read(&mut chunk).await?;
        if bytes == 0 {
            break;
        }
        output.extend_from_slice(&chunk[..bytes]);
        if output.len() > LIMIT {
            truncated = true;
            output.drain(..output.len() - LIMIT);
        }
    }
    let output = String::from_utf8_lossy(&output);
    Ok(if truncated {
        format!("… earlier output omitted …\n{output}")
    } else {
        output.into_owned()
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_test_code_without_network_project_writes_or_outside_reads() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, "outside")?;
        let root = temp.path().join("repo");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join(".env"), "secret")?;
        let tests = format!(
            r#"
import os
import socket
import unittest

class ScopeTests(unittest.TestCase):
    def test_scope(self):
        for path in ({outside:?}, ".env"):
            with self.assertRaises(PermissionError):
                with open(path): pass
        with self.assertRaises(PermissionError):
            with open("protected.txt", "w"): pass
        with self.assertRaises(PermissionError):
            socket.create_connection(("127.0.0.1", 9), timeout=1)

    def test_runs(self):
        self.assertEqual(20 + 22, 42)
"#,
            outside = outside.to_string_lossy()
        );
        std::fs::write(root.join("test_scope.py"), tests)?;
        let root = root.canonicalize()?;
        let result = Suite::detect(&root)?
            .run(&root, CancellationToken::new())
            .await?;
        assert_eq!(result["passed"], true, "{result}");
        assert!(result["stderr"].as_str().unwrap().contains("Ran 2 tests"));
        assert!(!root.join("protected.txt").exists());
        Ok(())
    }
}
