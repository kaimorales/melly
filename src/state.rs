use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
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
    pub team: SavedTeam,
    pub selected: String,
    pub drafts: BTreeMap<String, String>,
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
        let entries = match fs::read_dir(self.root.join("workspaces")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut latest = None;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(Some(workspace)) = read::<SavedWorkspace>(&path) else {
                continue;
            };
            if workspace.version != 1 || Path::new(&workspace.team.cwd) != cwd {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            if latest.as_ref().is_none_or(|(time, _)| modified > *time) {
                latest = Some((modified, workspace.team.lead_id));
            }
        }
        Ok(latest.map(|(_, id)| id))
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
