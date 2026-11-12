use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use melly::delegation::SavedTeam;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub accent: usize,
    pub motion: bool,
    pub expand_thinking: bool,
    pub expand_tools: bool,
    pub auto_delegate: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            accent: 0,
            motion: std::env::var_os("NO_MOTION").is_none(),
            expand_thinking: false,
            expand_tools: false,
            auto_delegate: true,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWorkspace {
    pub version: u32,
    #[serde(default)]
    pub title: String,
    pub team: SavedTeam,
    pub selected: String,
    pub drafts: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub workers: usize,
    pub modified: SystemTime,
}

// History does not need transcripts, briefs or command output. Skip those fields while
// deserializing so opening the picker doesn't allocate every saved worker's evidence.
#[derive(Deserialize)]
struct HistoryWorkspace {
    version: u32,
    #[serde(default)]
    title: String,
    team: HistoryTeam,
    drafts: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct HistoryTeam {
    lead_id: String,
    cwd: String,
    lead_started: bool,
    workers: Vec<serde::de::IgnoredAny>,
}

pub struct State {
    root: PathBuf,
    lock: Option<(String, File)>,
    last_saved: Option<Vec<u8>>,
    allow_rekey: bool,
}

impl State {
    pub fn open() -> Result<Self> {
        let root = if let Some(path) = std::env::var_os("MELLY_DATA_DIR") {
            PathBuf::from(path)
        } else if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
            PathBuf::from(path).join("melly")
        } else {
            PathBuf::from(std::env::var_os("HOME").context("Set HOME or MELLY_DATA_DIR")?)
                .join(".local/state/melly")
        };
        Ok(Self {
            root,
            lock: None,
            last_saved: None,
            allow_rekey: false,
        })
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(read(&self.root.join("settings.json"))?.unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write(
            &self.root.join("settings.json"),
            &serde_json::to_vec(settings)?,
        )
    }

    pub fn latest(&self, cwd: &Path) -> Result<Option<String>> {
        Ok(self
            .history()?
            .into_iter()
            .find(|s| Path::new(&s.cwd) == cwd)
            .map(|s| s.id))
    }

    /// A separate lock owner lets us prepare a switch without releasing the current session.
    pub fn fork(&self) -> Self {
        Self {
            root: self.root.clone(),
            lock: None,
            last_saved: None,
            allow_rekey: false,
        }
    }

    /// Read only on explicit history requests, never on the render or streaming path.
    pub fn history(&self) -> Result<Vec<SessionSummary>> {
        let entries = match fs::read_dir(self.root.join("workspaces")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(error.into()),
        };
        let mut sessions = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(Some(workspace)) = read::<HistoryWorkspace>(&path) else {
                continue;
            };
            if workspace.version != 1
                || self.workspace_path(&workspace.team.lead_id).ok().as_ref() != Some(&path)
                || (!workspace.team.lead_started
                    && workspace.team.workers.is_empty()
                    && workspace.title.is_empty()
                    && workspace.drafts.values().all(String::is_empty))
            {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            sessions.push(SessionSummary {
                id: workspace.team.lead_id,
                title: workspace.title,
                cwd: workspace.team.cwd,
                workers: workspace.team.workers.len(),
                modified,
            });
        }
        sessions.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
        Ok(sessions)
    }

    pub fn load(&mut self, id: &str) -> Result<Option<SavedWorkspace>> {
        let path = self.workspace_path(id)?;
        self.lock_workspace(id)?;
        let saved: Option<SavedWorkspace> = read(&path)?;
        if let Some(saved) = &saved
            && (saved.version != 1 || saved.team.lead_id != id)
        {
            bail!(
                "Invalid or unsupported Melly workspace at {}",
                path.display()
            );
        }
        self.allow_rekey = saved.as_ref().is_some_and(|saved| !saved.team.lead_started);
        Ok(saved)
    }

    pub fn save(&mut self, saved: &SavedWorkspace) -> Result<()> {
        let id = &saved.team.lead_id;
        let path = self.workspace_path(id)?;
        let bytes = serde_json::to_vec(saved)?;
        if self.last_saved.as_ref() != Some(&bytes) {
            if self.allow_rekey && self.lock.as_ref().is_some_and(|(locked, _)| locked != id) {
                // Keep the old manifest and lock until its replacement is durable.
                let previous = self.lock.take().unwrap();
                if let Err(error) = self.lock_workspace(id).and_then(|()| write(&path, &bytes)) {
                    self.lock = Some(previous);
                    return Err(error);
                }
                fs::remove_file(self.workspace_path(&previous.0)?)?;
                self.allow_rekey = false;
            } else {
                self.lock_workspace(id)?;
                write(&path, &bytes)?;
            }
            self.last_saved = Some(bytes);
        }
        Ok(())
    }

    fn workspace_path(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            bail!("Invalid Codex thread ID");
        }
        Ok(self.root.join("workspaces").join(format!("{id}.json")))
    }

    fn lock_workspace(&mut self, id: &str) -> Result<()> {
        if let Some((locked, _)) = &self.lock {
            if locked != id {
                bail!("Codex changed the lead thread ID");
            }
            return Ok(());
        }
        let path = self.workspace_path(id)?.with_extension("lock");
        private_dir(path.parent().unwrap())?;
        let file = private_file(&path, false)?;
        file.try_lock().with_context(|| {
            format!("Cannot lock Melly workspace {id}. Close its other Melly instance and retry.")
        })?;
        self.lock = Some((id.into(), file));
        Ok(())
    }
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("Invalid data in {}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Could not read {}", path.display())),
    }
}

fn private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn private_file(path: &Path, exclusive: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).create_new(exclusive);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    private_dir(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = private_file(&temporary, true)
        .with_context(|| format!("Could not prepare {}", path.display()))?;
    let result = (|| -> io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("Could not save {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(State);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "melly-history-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            Self(State {
                root,
                lock: None,
                last_saved: None,
                allow_rekey: false,
            })
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0.root);
        }
    }

    fn saved(id: &str, cwd: &str) -> SavedWorkspace {
        SavedWorkspace {
            version: 1,
            title: format!("Task {id}"),
            selected: "lead".into(),
            drafts: BTreeMap::new(),
            team: SavedTeam {
                lead_id: id.into(),
                cwd: cwd.into(),
                lead_started: true,
                lead_model: None,
                workers: vec![],
            },
        }
    }

    #[test]
    fn history_sorts_all_projects_skips_corrupt_and_mismatched_files_and_reads_legacy_titles() {
        let f = Fixture::new();
        let old = saved("old", "/work/a");
        let new = saved("new", "/work/b");
        let mut legacy = serde_json::to_value(&old).unwrap();
        legacy.as_object_mut().unwrap().remove("title");
        write(
            &f.0.workspace_path("old").unwrap(),
            &serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        File::open(f.0.workspace_path("old").unwrap())
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        write(
            &f.0.workspace_path("new").unwrap(),
            &serde_json::to_vec(&new).unwrap(),
        )
        .unwrap();
        write(&f.0.workspace_path("broken").unwrap(), b"{").unwrap();
        write(
            &f.0.workspace_path("wrong-id").unwrap(),
            &serde_json::to_vec(&new).unwrap(),
        )
        .unwrap();
        let sessions = f.0.history().unwrap();
        assert_eq!(
            sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["new", "old"]
        );
        assert!(sessions[1].title.is_empty());
        assert_eq!(
            f.0.latest(Path::new("/work/a")).unwrap().as_deref(),
            Some("old")
        );
        assert_eq!(f.0.latest(Path::new("/work/c")).unwrap(), None);
    }

    #[test]
    fn destination_lock_failure_keeps_current_workspace_lock_and_data() {
        let mut f = Fixture::new();
        f.0.save(&saved("current", "/work/a")).unwrap();
        let mut other = f.0.fork();
        other.save(&saved("destination", "/work/a")).unwrap();
        let mut next = f.0.fork();
        assert!(next.load("destination").is_err());
        assert!(f.0.fork().load("current").is_err());
        f.0.save(&saved("current", "/work/a")).unwrap();
        drop(other);
        assert!(next.load("destination").unwrap().is_some());
    }

    #[test]
    fn empty_launches_do_not_clutter_history_but_drafts_are_kept() {
        let mut f = Fixture::new();
        let mut s = saved("empty", "/work/a");
        s.title.clear();
        s.team.lead_started = false;
        f.0.save(&s).unwrap();
        assert!(f.0.history().unwrap().is_empty());
        s.drafts.insert("lead".into(), "unsent work".into());
        f.0.save(&s).unwrap();
        assert_eq!(f.0.history().unwrap().len(), 1);
    }
}
