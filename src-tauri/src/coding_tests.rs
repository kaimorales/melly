use anyhow::{Context, Result, bail, ensure};
use melly_core::linked;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub(crate) struct Suite {
    identity: (u64, u64),
    runner: PathBuf,
    args: Vec<String>,
    label: String,
    result_label: String,
    runtime_reads: Vec<PathBuf>,
    manifest: Option<(String, String)>,
}

fn installed(name: &str, project: &Path) -> Result<PathBuf> {
    let mut paths: Vec<_> = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        paths.extend([
            PathBuf::from(&home).join(".local/bin"),
            PathBuf::from(home).join(".cargo/bin"),
        ]);
    }
    paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    paths
        .into_iter()
        .filter(|p| p.is_absolute() && !p.starts_with(project))
        .map(|p| p.join(name))
        .find(|p| {
            p.canonicalize()
                .is_ok_and(|canonical| canonical.is_file() && !canonical.starts_with(project))
        })
        .with_context(|| format!("Install {name} to run this project's checks"))
}

fn manifest(root: &Path, path: &str) -> Result<Option<String>> {
    if !root.join(path).try_exists()? {
        return Ok(None);
    }
    let bytes = linked::read_scoped_with_limit(
        root,
        Some(linked::root_identity(root)?),
        Path::new(path),
        256 * 1024,
    )?;
    Ok(Some(
        String::from_utf8(bytes).context("The project manifest must be text")?,
    ))
}

impl Suite {
    pub(crate) fn detect(root: &Path, check: Option<&str>) -> Result<Self> {
        ensure!(
            cfg!(target_os = "macos"),
            "The isolated check runner requires macOS"
        );
        let identity = linked::root_identity(root)?;
        let check = check.unwrap_or("test");
        ensure!(
            ["test", "build", "check", "typecheck", "lint"].contains(&check),
            "Choose test, build, check, typecheck, or lint"
        );
        if let Some(text) = manifest(root, "package.json")? {
            let package: Value = serde_json::from_str(&text).context("Invalid package.json")?;
            let node = installed("node", root)?.canonicalize()?;
            let mut reads = vec![node.parent().unwrap().to_owned()];
            if let Some(prefix) = node.parent().and_then(Path::parent) {
                reads.push(prefix.join("lib"));
            }
            let (args, label) = if package["scripts"][check].is_string() {
                let npm = installed("npm", root)?.canonicalize()?;
                reads.push(
                    npm.parent()
                        .and_then(Path::parent)
                        .context("Cannot locate npm's installation")?
                        .to_owned(),
                );
                (
                    vec![
                        npm.to_string_lossy().into_owned(),
                        "run".into(),
                        check.into(),
                    ],
                    format!("npm run {check}"),
                )
            } else if check == "test" {
                (vec!["--test".into()], "node --test".into())
            } else {
                bail!("This project has no {check} script in package.json");
            };
            return Ok(Self {
                identity,
                runner: node,
                args,
                label,
                result_label: format!("{} command", if check == "test" { "Test" } else { "Check" }),
                runtime_reads: reads,
                manifest: Some(("package.json".into(), text)),
            });
        }
        if let Some(text) = manifest(root, "Cargo.toml")? {
            ensure!(
                ["test", "check", "build"].contains(&check),
                "Rust projects support test, check, and build"
            );
            let runner = installed("cargo", root)?;
            return Ok(Self {
                identity,
                runner,
                args: vec![check.into(), "--offline".into()],
                label: format!("cargo {check} --offline"),
                result_label: if check == "test" {
                    "Tests".into()
                } else {
                    "Check".into()
                },
                runtime_reads: vec![],
                manifest: Some(("Cargo.toml".into(), text)),
            });
        }
        ensure!(
            check == "test",
            "No {check} command was found. Add a package script or Cargo project check."
        );
        let pytest = manifest(root, "pyproject.toml")?.is_some()
            || manifest(root, "pytest.ini")?.is_some()
            || root.join("conftest.py").is_file();
        let venv = root.join(".venv/bin/python3");
        let runner = if venv.exists() {
            venv
        } else {
            PathBuf::from("/Library/Developer/CommandLineTools/usr/bin/python3")
                .canonicalize()
                .or_else(|_| {
                    PathBuf::from("/Applications/Xcode.app/Contents/Developer/usr/bin/python3")
                        .canonicalize()
                })
                .context("Install Python or Apple Command Line Tools to run Python tests")?
        };
        let args: Vec<String> = if pytest {
            vec!["-B", "-m", "pytest", "-p", "no:cacheprovider"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else {
            let mut args = vec!["-B", "-m", "unittest", "discover", "-v"];
            if root.join("tests").is_dir() {
                args.extend(["-s", "tests"]);
            }
            args.into_iter().map(str::to_owned).collect()
        };
        let label = format!("python3 {}", args.join(" "));
        let canonical = runner.canonicalize()?;
        let runtime_reads = vec![canonical.parent().unwrap().to_owned()];
        Ok(Self {
            identity,
            runner,
            args,
            label,
            result_label: "Tests".into(),
            runtime_reads,
            manifest: None,
        })
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    #[cfg(target_os = "macos")]
    pub(crate) async fn run(self, project: &Path, cancel: CancellationToken) -> Result<Value> {
        use std::{os::unix::process::CommandExt, process::Stdio, time::Duration};
        ensure!(!cancel.is_cancelled(), "Check cancelled");
        ensure!(
            linked::root_identity(project)? == self.identity,
            "The linked project changed after approval"
        );
        let temp = tempfile::tempdir()?;
        let scratch = temp.path().canonicalize()?;
        let copy = scratch.join("project");
        let root = project.to_owned();
        let dest = copy.clone();
        let copying = cancel.clone();
        let identity = self.identity;
        tokio::task::spawn_blocking(move || snapshot(&root, identity, &dest, &copying)).await??;
        if let Some((path, expected)) = &self.manifest {
            ensure!(
                std::fs::read_to_string(copy.join(path))? == *expected,
                "The check command changed after approval. Run it again."
            );
        }
        let mut reads = self.runtime_reads;
        let mut dependency_roots = Vec::new();
        for dependency in ["node_modules", ".venv"] {
            let path = project.join(dependency);
            if path.exists() {
                let canonical = path.canonicalize()?;
                ensure!(
                    canonical.starts_with(project) && canonical.is_dir(),
                    "The {dependency} directory must stay inside the linked project"
                );
                std::os::unix::fs::symlink(&canonical, copy.join(dependency))?;
                dependency_roots.push(canonical.clone());
                reads.push(canonical);
            }
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let cargo_home = scratch.join("cargo");
        std::fs::create_dir(&cargo_home)?;
        if let Some(home) = &home {
            for directory in [".rustup", ".cargo/bin"] {
                if home.join(directory).exists() {
                    reads.push(home.join(directory));
                }
            }
            let registry = home.join(".cargo/registry");
            if registry.exists() {
                std::os::unix::fs::symlink(&registry, cargo_home.join("registry"))?;
                reads.push(registry);
            }
        }
        let runner = self.runner.canonicalize()?;
        let mut policy = String::from(
            r#"(version 1)
(deny default)
(allow process-fork)
(allow sysctl-read)
(allow file-read-metadata)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo") (global-name "com.apple.lsd.mapdb"))
(allow file-read* (subpath "/System") (subpath "/usr/lib") (subpath "/usr/bin") (subpath "/bin") (literal "/") (literal "/private") (subpath "/Library/Developer/CommandLineTools") (subpath "/Applications/Xcode.app") (subpath "/opt/homebrew") (subpath "/usr/local/lib") (subpath (param "SCRATCH")) (literal (param "RUNNER")) (literal "/private/etc/localtime") (literal "/private/etc/ssl/openssl.cnf") (literal "/private/etc/ssl/cert.pem") (literal "/dev/null") (literal "/dev/random") (literal "/dev/urandom"))
(allow process-exec (subpath "/usr/bin") (subpath "/bin") (subpath "/Library/Developer/CommandLineTools") (subpath "/Applications/Xcode.app") (subpath "/opt/homebrew") (subpath (param "SCRATCH")) (literal (param "RUNNER")))
(allow file-write* (subpath (param "SCRATCH")) (literal "/dev/null"))
(deny network*)
"#,
        );
        let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
        for (key, value) in [("SCRATCH", scratch.as_path()), ("RUNNER", runner.as_path())] {
            let mut define = std::ffi::OsString::from(format!("{key}="));
            define.push(value);
            command.arg("-D").arg(define);
        }
        for (index, path) in reads.iter().filter(|p| p.exists()).enumerate() {
            let name = format!("READ{index}");
            let mut value = std::ffi::OsString::from(format!("{name}="));
            value.push(path.canonicalize()?);
            command.arg("-D").arg(value);
            policy.push_str(&format!(
                "(allow file-read* process-exec (subpath (param \"{name}\")))\n"
            ));
        }
        for (index, path) in dependency_roots.iter().enumerate() {
            let mut pattern = String::from("^");
            for character in path.to_string_lossy().chars() {
                if ".^$[](){}*+?|\\".contains(character) {
                    pattern.push('\\');
                }
                pattern.push(character);
            }
            pattern.push_str("/([^/]*/)*([.]env($|[.])|[.]npmrc$|[.]pypirc$|credentials[^/]*$|id_rsa$|id_ed25519$)");
            let name = format!("PRIVATE{index}");
            command.arg("-D").arg(format!("{name}={pattern}"));
            policy.push_str(&format!(
                "(deny file-read-data (regex (param \"{name}\")))\n"
            ));
        }
        let mut paths = vec![
            copy.join("node_modules/.bin"),
            self.runner
                .parent()
                .context("Missing runner directory")?
                .to_owned(),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ];
        if let Some(home) = &home {
            paths.push(home.join(".cargo/bin"));
        }
        command
            .args(["-p", &policy])
            .arg(&self.runner)
            .args(&self.args)
            .current_dir(&copy)
            .env_clear()
            .env("HOME", &scratch)
            .env("TMPDIR", &scratch)
            .env("PATH", std::env::join_paths(paths)?)
            .env("CARGO_HOME", &cargo_home)
            .env("CARGO_TARGET_DIR", scratch.join("target"))
            .env("npm_config_cache", scratch.join("npm-cache"))
            .env("npm_config_update_notifier", "false")
            .env("CI", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(home) = &home {
            command.env("RUSTUP_HOME", home.join(".rustup"));
        }
        ensure!(
            linked::root_identity(project)? == self.identity,
            "The linked project changed after approval"
        );
        command.as_std_mut().process_group(0);
        let mut child = command
            .spawn()
            .context("Could not launch the isolated project check")?;
        let group = ProcessGroup(child.id().context("Missing check process ID")? as i32);
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let result = tokio::select! {
            _=cancel.cancelled()=>anyhow::bail!("Check cancelled"),
            result=tokio::time::timeout(Duration::from_secs(300),async {tokio::try_join!(child.wait(),capture(stdout),capture(stderr))})=>result.context("The check timed out after five minutes")??,
        };
        drop(group);
        let (status, stdout, stderr) = result;
        let output = format!("{stdout}\n{stderr}");
        let no_tests = output.contains("Ran 0 tests")
            || output.contains("no tests ran")
            || output.lines().any(|line| line.trim() == "# tests 0");
        Ok(
            json!({"command":self.label,"label":self.result_label,"passed":status.success()&&!no_tests,"no_tests_found":no_tests,"exit_code":status.code(),"stdout":stdout,"stderr":stderr,"isolated":true}),
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) async fn run(self, _: &Path, _: CancellationToken) -> Result<Value> {
        bail!("The isolated check runner requires macOS")
    }
}

#[cfg(target_os = "macos")]
fn snapshot(
    root: &Path,
    identity: (u64, u64),
    destination: &Path,
    cancel: &CancellationToken,
) -> Result<()> {
    use crate::file_edits::file_edit_target::Target;
    std::fs::create_dir(destination)?;
    let mut folders = vec![PathBuf::new()];
    let mut entries = 0;
    let mut total = 0;
    while let Some(relative) = folders.pop() {
        ensure!(!cancel.is_cancelled(), "Check cancelled");
        for (name, directory) in Target::entries(root, identity, &relative, 20_001)? {
            ensure!(!cancel.is_cancelled(), "Check cancelled");
            entries += 1;
            ensure!(
                entries <= 20_000,
                "The project is too large for an isolated check"
            );
            let path = relative.join(name);
            if directory {
                ensure!(
                    path.components().count() <= 32,
                    "Project nesting is too deep"
                );
                std::fs::create_dir(destination.join(&path))?;
                folders.push(path);
            } else {
                let bytes =
                    linked::read_scoped_with_limit(root, Some(identity), &path, 16 * 1024 * 1024)?;
                total += bytes.len();
                ensure!(
                    total <= 200 * 1024 * 1024,
                    "The project exceeds the 200 MiB check snapshot limit"
                );
                let output = destination.join(&path);
                std::fs::write(&output, bytes)?;
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(root.join(&path))?.permissions().mode() & 0o777;
                std::fs::set_permissions(output, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
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
    async fn python_checks_can_write_the_copy_but_not_the_project_or_network() -> Result<()> {
        let parent = tempfile::tempdir()?;
        let root = parent.path().join("repo");
        std::fs::create_dir(&root)?;
        let outside = parent.path().join("outside.txt");
        std::fs::write(&outside, "private")?;
        std::fs::write(root.join(".env"), "private")?;
        let code = format!(
            r#"import unittest, socket, os
class Scope(unittest.TestCase):
    def test_scope(self):
        with self.assertRaises(PermissionError):
            open({outside:?}).read()
        with self.assertRaises(PermissionError):
            open({original:?}, 'w').write('bad')
        with self.assertRaises((FileNotFoundError, PermissionError)):
            open('.env').read()
        with self.assertRaises(PermissionError):
            socket.create_connection(('127.0.0.1', 9), timeout=1)
        with open('generated.txt','w') as output:
            output.write('temporary')
        self.assertNotIn('MELLY_TEST_PRIVATE_VALUE',os.environ)
"#,
            outside = outside.to_string_lossy(),
            original = root.join("original.txt").to_string_lossy()
        );
        std::fs::write(root.join("test_scope.py"), code)?;
        let root = root.canonicalize()?;
        let result = Suite::detect(&root, None)?
            .run(&root, CancellationToken::new())
            .await?;
        assert_eq!(result["passed"], true, "{result}");
        assert!(!root.join("generated.txt").exists());
        assert!(!root.join("original.txt").exists());
        Ok(())
    }
    #[tokio::test]
    async fn runs_package_test_and_check_scripts_with_local_dependencies() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"test":"node --test test.cjs","check":"node --check test.cjs"}}"#,
        )?;
        std::fs::create_dir_all(root.join("node_modules/example"))?;
        std::fs::write(
            root.join("node_modules/example/index.js"),
            "module.exports = 42",
        )?;
        std::fs::write(
            root.join("node_modules/example/.env"),
            "synthetic-private-value",
        )?;
        std::fs::write(
            root.join("test.cjs"),
            "const test = require('node:test'); const assert = require('node:assert/strict'); test('local dependency',()=>{assert.equal(require('example'),42);assert.throws(()=>require('node:fs').readFileSync(require.resolve('example').replace('index.js','.env')), {code:'EPERM'});require('node:fs').writeFileSync('generated.txt','temporary')});",
        )?;
        for name in ["test", "check"] {
            let result = Suite::detect(&root, Some(name))?
                .run(&root, CancellationToken::new())
                .await?;
            assert_eq!(result["passed"], true, "{result}");
            assert_eq!(result["command"], format!("npm run {name}"));
        }
        assert!(!root.join("generated.txt").exists());
        assert!(Suite::detect(&root, Some("build")).is_err());
        assert!(Suite::detect(&root, Some("test; touch outside")).is_err());
        Ok(())
    }
    #[tokio::test]
    async fn rust_tests_compile_and_run_outside_the_checkout() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        std::fs::create_dir(root.join("src"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='synthetic_melly_check'\nversion='0.1.0'\nedition='2021'\n",
        )?;
        std::fs::write(
            root.join("src/lib.rs"),
            "#[test] fn sample() { assert_eq!(2 + 2, 4); }",
        )?;
        let result = Suite::detect(&root, None)?
            .run(&root, CancellationToken::new())
            .await?;
        assert_eq!(result["passed"], true, "{result}");
        assert!(result["stdout"].as_str().unwrap().contains("1 passed"));
        assert!(!root.join("target").exists());
        assert!(!root.join("Cargo.lock").exists());
        Ok(())
    }
    #[tokio::test]
    #[ignore = "Requires an explicitly prepared synthetic pytest environment"]
    async fn runs_real_pytest_in_a_local_virtualenv() -> Result<()> {
        let root = PathBuf::from(
            std::env::var_os("MELLY_PYTEST_FIXTURE")
                .context("Set MELLY_PYTEST_FIXTURE to a synthetic project")?,
        )
        .canonicalize()?;
        let result = Suite::detect(&root, None)?
            .run(&root, CancellationToken::new())
            .await?;
        assert_eq!(result["passed"], true, "{result}");
        assert!(
            result["stdout"].as_str().unwrap().contains("1 passed"),
            "{result}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn rejects_changed_commands_and_cancels_child_processes() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"test":"node -e 'setTimeout(()=>{},30000)'"}}"#,
        )?;
        let suite = Suite::detect(&root, None)?;
        std::fs::write(root.join("package.json"), "{}")?;
        assert!(
            suite
                .run(&root, CancellationToken::new())
                .await
                .unwrap_err()
                .to_string()
                .contains("changed after approval")
        );
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"test":"node -e 'setTimeout(()=>{},30000)'"}}"#,
        )?;
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            signal.cancel();
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            Suite::detect(&root, None)?.run(&root, cancel),
        )
        .await?;
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        Ok(())
    }
}
