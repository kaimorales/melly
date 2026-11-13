//! Bounded, advisory completion snapshots. Ownership is not edit attribution.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PACKET_LIMIT: usize = 32 * 1024;
const OUTPUT_LIMIT: usize = 32 * 1024;
const FILE_LIMIT: usize = 8 * 1024;
const MAX_FILES: usize = 128;
const CAPTURE_TIME: Duration = Duration::from_millis(1000);
const SOURCE: &str = "Owned paths versus HEAD at completion; not attribution; may include preexisting edits. Staged and unstaged changes plus Git-visible untracked files; ignored contents are omitted. This non-atomic snapshot may be stale after capture. It is not an acceptance verdict or proof of checks. If evidence is absent, omitted, failed, or changed later, inspect directly.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidence {
    pub source: String,
    pub captured_at_unix_ms: u64,
    pub head: Option<String>,
    pub complete: bool,
    pub diff: Option<String>,
    pub untracked_files: Vec<ReviewFile>,
    pub omissions: Vec<Omission>,
    pub omissions_omitted: usize,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFile {
    pub path: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omission {
    pub path: String,
    pub reason: String,
}

impl ReviewEvidence {
    fn new() -> Self {
        Self {
            source: SOURCE.into(),
            captured_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            head: None,
            complete: true,
            diff: None,
            untracked_files: Vec::new(),
            omissions: Vec::new(),
            omissions_omitted: 0,
            failure: None,
        }
    }

    fn failed(reason: &str) -> Self {
        let mut packet = Self::new();
        packet.complete = false;
        packet.failure = Some(reason.into());
        packet
    }

    fn omit(&mut self, path: &str, reason: &str) {
        self.complete = false;
        if self.omissions.len() < 32 {
            self.omissions.push(Omission {
                path: path.chars().take(512).collect(),
                reason: reason.into(),
            });
        } else {
            self.omissions_omitted += 1;
        }
    }

    fn encoded_len(&self) -> usize {
        serde_json::to_vec(self)
            .expect("serializable review evidence")
            .len()
    }

    fn bound(mut self) -> Self {
        // Budget the actual JSON, including escaping. Keep whole evidence entries only.
        while self.encoded_len() > PACKET_LIMIT {
            if let Some(file) = self.untracked_files.pop() {
                self.omit(&file.path, "serialized packet budget exceeded");
            } else if self.diff.take().is_some() {
                self.omit("(tracked diff)", "serialized packet budget exceeded");
            } else if self.omissions.pop().is_some() {
                self.omissions_omitted += 1;
            } else {
                break;
            }
        }
        self
    }
}

pub(crate) fn capture(workspace: &Path, owned_paths: &[String]) -> ReviewEvidence {
    let workspace = workspace.to_owned();
    let owned_paths = owned_paths.to_vec();
    let (send, receive) = mpsc::sync_channel(1);
    // Filesystem calls can also stall (for example on an unavailable mount). Never
    // join this thread on the UI path; each subprocess has the same inner deadline.
    let spawned = std::thread::Builder::new()
        .name("review-evidence".into())
        .spawn(move || {
            let deadline = Instant::now() + CAPTURE_TIME;
            let mut packet = ReviewEvidence::new();
            if let Err(reason) = capture_inner(&workspace, &owned_paths, deadline, &mut packet) {
                packet.complete = false;
                packet.failure = Some(reason);
            }
            let _ = send.send(packet.bound());
        });
    if spawned.is_err() {
        return ReviewEvidence::failed("Could not start completion capture; inspect directly.");
    }
    receive
        .recv_timeout(CAPTURE_TIME + Duration::from_millis(100))
        .unwrap_or_else(|_| {
            ReviewEvidence::failed("Completion capture timed out or failed; inspect directly.")
        })
}

fn capture_inner(
    workspace: &Path,
    owned_paths: &[String],
    deadline: Instant,
    packet: &mut ReviewEvidence,
) -> Result<(), String> {
    let root = workspace
        .canonicalize()
        .map_err(|_| "Workspace is unavailable; inspect directly.")?;
    let mut paths = Vec::new();
    for path in owned_paths {
        match checked_path(&root, path) {
            Ok(_) => paths.push(path.clone()),
            Err(reason) => packet.omit(path, reason),
        }
    }
    if paths.is_empty() {
        return Err("No safe owned paths available; inspect directly.".into());
    }
    let git = |args: &[&str], selectors: &[String]| run_git(&root, args, selectors, deadline);
    let head = git(&["rev-parse", "--verify", "HEAD"], &[])?;
    packet.head = Some(
        String::from_utf8(head)
            .map_err(|_| "Invalid Git HEAD; inspect directly.")?
            .trim()
            .into(),
    );

    let mut tracked = inventory(git(&["ls-files", "--cached", "-z", "--"], &paths)?, packet)?;
    // The index alone loses staged deletions. ls-tree (without --full-tree or
    // --full-name) returns paths relative to cwd, like ls-files and our selectors.
    let head_paths = inventory(
        git(
            &["ls-tree", "-r", "--name-only", "-z", "HEAD", "--"],
            &paths,
        )?,
        packet,
    )?;
    for path in head_paths {
        if tracked.len() < MAX_FILES || tracked.contains(&path) {
            tracked.insert(path);
        } else {
            packet.omit(&path, "combined HEAD/index inventory limit exceeded");
        }
    }
    // Git ls-files omits special files such as FIFOs. A bounded filesystem walk
    // accounts for those and ignored new files without ever opening them blindly.
    let visible = inventory(
        git(
            &["ls-files", "--others", "--exclude-standard", "-z", "--"],
            &paths,
        )?,
        packet,
    )?;
    let untracked = untracked_inventory(&root, &paths, &tracked, &visible, deadline, packet)?;
    for path in &paths {
        if !tracked
            .iter()
            .chain(&untracked)
            .any(|file| Path::new(file).starts_with(path))
        {
            packet.omit(path, "no tracked or untracked file found; inspect directly");
        }
    }
    let mut safe_tracked = Vec::new();
    for path in tracked {
        check_deadline(deadline)?;
        match checked_path(&root, &path) {
            Ok(Some(meta)) if !meta.is_file() => {
                packet.omit(&path, "directory/submodule or special file not captured")
            }
            Ok(_) => safe_tracked.push(path), // Missing tracked files are deletions.
            Err(reason) => packet.omit(&path, reason),
        }
    }
    if !safe_tracked.is_empty() {
        // Clean/process filters can execute programs even with --no-textconv.
        // Disable every configured filter driver for this capture as well.
        let keys = git(&["config", "--null", "--name-only", "--list"], &[])?;
        let mut args = Vec::new();
        for key in keys.split(|byte| *byte == 0).filter(|key| !key.is_empty()) {
            let key = std::str::from_utf8(key)
                .map_err(|_| "Invalid Git config key; inspect directly.")?;
            if key.starts_with("filter.")
                && [".clean", ".smudge", ".process", ".required"]
                    .iter()
                    .any(|suffix| key.ends_with(suffix))
            {
                args.push("-c".to_owned());
                args.push(format!(
                    "{key}={}",
                    if key.ends_with(".required") {
                        "false"
                    } else {
                        ""
                    }
                ));
            }
        }
        args.extend(
            [
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--no-color",
                "--no-relative",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "HEAD",
                "--",
            ]
            .map(str::to_owned),
        );
        let args: Vec<_> = args.iter().map(String::as_str).collect();
        match git(&args, &safe_tracked) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(diff) => {
                    if diff.contains("\nBinary files ") || diff.contains("\nGIT binary patch\n") {
                        packet.omit(
                            "(tracked diff)",
                            "binary contents unavailable; inspect directly",
                        );
                    }
                    packet.diff = Some(diff);
                }
                Err(_) => packet.omit("(tracked diff)", "non-UTF-8 diff omitted; inspect directly"),
            },
            Err(reason) => packet.omit("(tracked diff)", &reason),
        }
    } else {
        packet.diff = Some(String::new());
    }
    for path in untracked {
        check_deadline(deadline)?;
        match read_untracked(&root, &path) {
            Ok(content) => packet.untracked_files.push(ReviewFile { path, content }),
            Err(reason) => packet.omit(&path, reason),
        }
        // Reserve space for later omission metadata without growing an unbounded
        // intermediate packet when many escaped files are encountered.
        while packet.encoded_len() > PACKET_LIMIT - 4096 && !packet.untracked_files.is_empty() {
            let file = packet.untracked_files.pop().unwrap();
            packet.omit(&file.path, "serialized packet budget exceeded");
        }
    }
    if git(&["rev-parse", "--verify", "HEAD"], &[])?
        != format!("{}\n", packet.head.as_deref().unwrap()).as_bytes()
    {
        packet.omit("HEAD", "HEAD changed during capture; inspect directly");
    }
    Ok(())
}

fn inventory(bytes: Vec<u8>, packet: &mut ReviewEvidence) -> Result<BTreeSet<String>, String> {
    let mut paths = BTreeSet::new();
    for bytes in bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let Ok(path) = std::str::from_utf8(bytes) else {
            packet.omit("(unrepresentable path)", "non-UTF-8 path omitted");
            continue;
        };
        if paths.len() == MAX_FILES || path.len() > 512 {
            packet.omit(path, "file inventory limit exceeded");
        } else {
            paths.insert(path.into());
        }
    }
    Ok(paths)
}

fn untracked_inventory(
    root: &Path,
    owned: &[String],
    tracked: &BTreeSet<String>,
    visible: &BTreeSet<String>,
    deadline: Instant,
    packet: &mut ReviewEvidence,
) -> Result<BTreeSet<String>, String> {
    let mut pending: BTreeSet<String> = owned.iter().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut untracked = BTreeSet::new();
    while let Some(path) = pending.pop_first() {
        check_deadline(deadline)?;
        if !visited.insert(path.clone()) {
            continue;
        }
        if visited.len() > MAX_FILES * 2 {
            packet.omit(
                "(filesystem inventory)",
                "directory entry limit exceeded; inspect directly",
            );
            break;
        }
        let meta = match checked_path(root, &path) {
            Ok(Some(meta)) => meta,
            Ok(None) => continue,
            Err(reason) => {
                packet.omit(&path, reason);
                continue;
            }
        };
        if meta.is_dir() {
            if tracked.contains(&path) {
                packet.omit(&path, "submodule contents not captured");
                continue;
            }
            if !tracked
                .iter()
                .chain(visible)
                .any(|file| Path::new(file).starts_with(&path))
            {
                packet.omit(&path, "ignored or Git-unlisted directory omitted");
                continue;
            }
            let entries = match fs::read_dir(root.join(&path)) {
                Ok(entries) => entries,
                Err(_) => {
                    packet.omit(&path, "directory unreadable; inspect directly");
                    continue;
                }
            };
            for entry in entries.take(MAX_FILES * 2 + 1) {
                let Ok(entry) = entry else {
                    packet.omit(&path, "directory entry unreadable");
                    continue;
                };
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    packet.omit(&path, "non-UTF-8 directory entry omitted");
                    continue;
                };
                let child = format!("{path}/{name}");
                if name == ".git" || child.len() > 512 {
                    packet.omit(&child, "repository metadata or overlong path omitted");
                } else if pending.len() + visited.len() >= MAX_FILES * 2 {
                    packet.omit(&path, "directory entry limit exceeded; inspect directly");
                    break;
                } else {
                    pending.insert(child);
                }
            }
        } else if visible.contains(&path) {
            // A HEAD file removed from the index can also have a new untracked
            // worktree copy. Include both the deletion diff and this content.
            untracked.insert(path);
        } else if !tracked.contains(&path) {
            packet.omit(&path, "ignored or Git-unlisted special file omitted");
        }
    }
    Ok(untracked)
}

/// Refuse symlink components, including aliases within the workspace. Deleted
/// paths are allowed only after validating all existing ancestors.
fn checked_path(root: &Path, relative: &str) -> Result<Option<Metadata>, &'static str> {
    let path = Path::new(relative);
    if relative.is_empty()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("not a contained relative path");
    }
    let mut current = root.to_owned();
    let mut last = None;
    for part in path.components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("symlink path omitted; inspect directly");
                }
                if !current
                    .canonicalize()
                    .is_ok_and(|canonical| canonical.starts_with(root))
                {
                    return Err("path outside workspace or unavailable");
                }
                last = Some(meta);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => last = None,
            Err(_) => return Err("path unavailable; inspect directly"),
        }
    }
    Ok(last)
}

fn read_untracked(root: &Path, path: &str) -> Result<String, &'static str> {
    let initial = checked_path(root, path)?.ok_or("missing during capture")?;
    if !initial.is_file() {
        return Err("not a regular file; inspect directly");
    }
    if initial.len() > FILE_LIMIT as u64 {
        return Err("file exceeds 8 KiB content limit; inspect directly");
    }
    let target = root.join(path);
    let mut options = OpenOptions::new();
    options.read(true);
    // O_NOFOLLOW and O_NONBLOCK prevent a last-component symlink/FIFO replacement
    // from being followed or blocking open on supported platforms.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100 | 0x4);
        #[cfg(target_os = "linux")]
        options.custom_flags(0x20000 | 0x800);
    }
    let file = options
        .open(&target)
        .map_err(|_| "file could not be opened safely")?;
    let opened = file.metadata().map_err(|_| "file metadata unavailable")?;
    if !same_file(&initial, &opened)
        || !checked_path(root, path)?.is_some_and(|meta| same_file(&opened, &meta))
    {
        return Err("file changed during capture; inspect directly");
    }
    let mut bytes = Vec::new();
    let mut reader = file.take((FILE_LIMIT + 1) as u64);
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| "file could not be read")?;
    let after = reader
        .get_ref()
        .metadata()
        .map_err(|_| "file metadata unavailable")?;
    if bytes.len() > FILE_LIMIT {
        return Err("file exceeds 8 KiB content limit; inspect directly");
    }
    if !same_file(&opened, &after)
        || bytes.len() as u64 != after.len()
        || !checked_path(root, path)?.is_some_and(|meta| same_file(&after, &meta))
    {
        return Err("file changed during capture; inspect directly");
    }
    if bytes.contains(&0) {
        return Err("binary file omitted; inspect directly");
    }
    String::from_utf8(bytes).map_err(|_| "non-UTF-8 file omitted; inspect directly")
}

fn same_file(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return false;
        }
    }
    after.is_file()
        && before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
}

fn check_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("Completion capture deadline exceeded; inspect directly.".into())
    } else {
        Ok(())
    }
}

fn run_git(
    root: &Path,
    args: &[&str],
    paths: &[String],
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    check_deadline(deadline)?;
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.submodule=short",
        ])
        .args(args)
        .args(paths)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_EXTERNAL_DIFF",
        "GIT_DIFF_OPTS",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_GLOB_PATHSPECS",
        "GIT_NOGLOB_PATHSPECS",
        "GIT_ICASE_PATHSPECS",
    ] {
        command.env_remove(name);
    }
    run_bounded(command, deadline)
}

fn run_bounded(mut command: Command, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut child = command
        .spawn()
        .map_err(|_| "Git could not start; inspect directly.")?;
    let (send, receive) = mpsc::channel();
    fn read_pipe(
        mut pipe: impl Read + Send + 'static,
        send: mpsc::Sender<(bool, std::io::Result<Vec<u8>>)>,
        stdout: bool,
        limit: usize,
    ) {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe
                .by_ref()
                .take((limit + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = send.send((stdout, result));
        });
    }
    read_pipe(
        child.stdout.take().unwrap(),
        send.clone(),
        true,
        OUTPUT_LIMIT,
    );
    read_pipe(child.stderr.take().unwrap(), send, false, 2048);
    let mut stdout = None;
    let mut stderr_done = false;
    let result = loop {
        for (is_stdout, result) in receive.try_iter() {
            match result {
                Ok(bytes) if bytes.len() <= if is_stdout { OUTPUT_LIMIT } else { 2048 } => {
                    if is_stdout {
                        stdout = Some(bytes);
                    } else {
                        stderr_done = true;
                    }
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(
                        "Git output exceeded capture limit or failed; inspect directly.".into(),
                    );
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) if stdout.is_some() && stderr_done => {
                break if status.success() {
                    Ok(stdout.unwrap())
                } else {
                    Err("Git failed (repository/HEAD or file evidence unavailable); inspect directly.".into())
                };
            }
            Err(_) => break Err("Git status unavailable; inspect directly.".into()),
            _ => {}
        }
        if let Err(error) = check_deadline(deadline) {
            break Err(error);
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn command_deadline_kills_a_running_process() {
        let mut command = Command::new("/bin/sleep");
        command
            .arg("5")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let start = Instant::now();
        let result = run_bounded(command, start + Duration::from_millis(20));
        assert!(result.unwrap_err().contains("deadline"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn command_output_is_bounded_before_serialization() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "printf '%040000d' 0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let result = run_bounded(command, Instant::now() + Duration::from_secs(1));
        assert!(result.unwrap_err().contains("output exceeded"));
    }
}
