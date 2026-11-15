//! Native-selected project directory links, independent of the project index.
use crate::storage::Store;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = crate::file_edits::MAX_FILE_BYTES as u64;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLink {
    pub folder_id: String,
    pub name: String,
    pub status: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub device: u64,
    #[serde(skip)]
    pub inode: u64,
}

pub fn excluded(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with('.')
        || name.contains("secret")
        || name.contains("credential")
        || matches!(
            name.as_str(),
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | "venv"
                | "__pycache__"
                | "coverage"
                | "package-lock.json"
                | "cargo.lock"
        )
}

impl Store {
    pub(crate) fn initialize_file_edits(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS project_links(folder_id TEXT PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE,path TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,status TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS file_edit_scopes(message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,device INTEGER NOT NULL,inode INTEGER NOT NULL,applied INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS file_edit_diffs(message_id TEXT PRIMARY KEY REFERENCES file_edit_scopes(message_id) ON DELETE CASCADE,diff TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS coding_activity(id INTEGER PRIMARY KEY,message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,result TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS coding_activity_by_message ON coding_activity(message_id,id);",
        )?;
        Ok(())
    }

    pub fn ensure_active_project(&self, folder_id: &str) -> Result<()> {
        ensure!(
            self.conn
                .query_row("SELECT 1 FROM folders WHERE id=?1", [folder_id], |_| Ok(()))
                .optional()?
                .is_some(),
            "Project not found"
        );
        Ok(())
    }

    pub fn project_link(&self, folder_id: &str) -> Result<Option<ProjectLink>> {
        self.conn
            .query_row(
                "SELECT path,device,inode,status FROM project_links WHERE folder_id=?1",
                [folder_id],
                |row| {
                    let path = PathBuf::from(row.get::<_, String>(0)?);
                    Ok(ProjectLink {
                        folder_id: folder_id.to_owned(),
                        name: path
                            .file_name()
                            .unwrap_or(path.as_os_str())
                            .to_string_lossy()
                            .into_owned(),
                        path,
                        device: row.get::<_, i64>(1)? as u64,
                        inode: row.get::<_, i64>(2)? as u64,
                        status: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_project_link(&self, folder_id: &str, path: &Path) -> Result<()> {
        self.ensure_active_project(folder_id)?;
        ensure!(
            std::fs::symlink_metadata(path)?.is_dir(),
            "Choose a real folder"
        );
        let path = path.canonicalize()?;
        let (device, inode) = crate::linked::root_identity(&path)?;
        let path = path.to_str().context("Choose a folder with a UTF-8 path")?;
        self.conn.execute(
            "INSERT INTO project_links(folder_id,path,device,inode,status) VALUES(?1,?2,?3,?4,'Linked')
             ON CONFLICT(folder_id) DO UPDATE SET path=excluded.path,device=excluded.device,inode=excluded.inode,status=excluded.status",
            params![folder_id, path, device as i64, inode as i64],
        )?;
        Ok(())
    }

    pub fn unlink_project(&self, folder_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM project_links WHERE folder_id=?1", [folder_id])?;
        Ok(())
    }
}

impl Store {
    pub fn record_file_edit_scope(&self, message_id: &str, link: &ProjectLink) -> Result<()> {
        self.conn.execute(
            "INSERT INTO file_edit_scopes(message_id,folder_id,device,inode) VALUES(?1,?2,?3,?4)",
            params![
                message_id,
                link.folder_id,
                link.device as i64,
                link.inode as i64
            ],
        )?;
        Ok(())
    }

    pub fn file_edit_scope(&self, chat_id: &str, message_id: &str) -> Result<(ProjectLink, bool)> {
        let message = self.message(message_id)?;
        ensure!(
            message.chat_id == chat_id
                && message.role == "assistant"
                && message.status == "complete",
            "The chat reply must complete before you can apply a file change"
        );
        let (folder_id, device, inode, applied): (String, i64, i64, bool) = self
            .conn
            .query_row(
                "SELECT folder_id,device,inode,applied FROM file_edit_scopes WHERE message_id=?1",
                [message_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .context("Ask for a new edit in a linked project")?;
        ensure!(
            self.chat(chat_id)?.folder_id.as_deref() == Some(&folder_id),
            "The chat moved. Ask for a new edit."
        );
        self.ensure_active_project(&folder_id)?;
        let link = self
            .project_link(&folder_id)?
            .context("This project is no longer linked")?;
        ensure!(
            link.device == device as u64 && link.inode == inode as u64,
            "The linked folder changed. Ask for a new edit."
        );
        Ok((link, applied))
    }

    pub fn saved_file_edit_diff(
        &self,
        chat_id: &str,
        message_id: &str,
    ) -> Result<Option<Vec<crate::file_edits::DiffLine>>> {
        let diff: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT d.diff FROM file_edit_scopes s JOIN messages m ON m.id=s.message_id
             LEFT JOIN file_edit_diffs d ON d.message_id=s.message_id
             WHERE s.applied=1 AND m.chat_id=?1 AND s.message_id=?2",
                params![chat_id, message_id],
                |row| row.get(0),
            )
            .optional()?;
        diff.map(|value| {
            value
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map(|diff| diff.unwrap_or_default())
        })
        .transpose()
        .map_err(Into::into)
    }

    pub fn mark_file_edit_applied(
        &self,
        message_id: &str,
        diff: &[crate::file_edits::DiffLine],
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE file_edit_scopes SET applied=1 WHERE message_id=?1",
            [message_id],
        )?;
        tx.execute(
            "INSERT INTO file_edit_diffs(message_id, diff) VALUES (?1,?2)",
            params![message_id, serde_json::to_string(diff)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn mentioned_edit_files(
        &self,
        link: &ProjectLink,
        request: &str,
    ) -> Vec<(String, String)> {
        let mut files = Vec::new();
        let mut total = 0;
        let quoted = request
            .split('`')
            .enumerate()
            .filter_map(|(i, segment)| (i % 2 == 1).then_some(segment));
        let words = request
            .split_whitespace()
            .map(|word| word.trim_matches(['\'', '"', '`', ',', '.', ';', '(', ')']));
        for candidate in quoted.chain(words).take(512) {
            let path = Path::new(candidate);
            if candidate.is_empty() || candidate.len() > 1024 || !candidate.contains('.') ||
                !path.components().all(|part| matches!(part, std::path::Component::Normal(name) if name.to_str().is_some_and(|name| !excluded(name)))) ||
                files.iter().any(|(name, _)| name == candidate) {
                continue;
            }
            let Ok(bytes) =
                crate::linked::read_scoped(&link.path, Some((link.device, link.inode)), path)
            else {
                continue;
            };
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            if text.contains('\0') || total + text.len() > MAX_FILE_BYTES as usize {
                continue;
            }
            total += text.len();
            files.push((candidate.to_owned(), text));
            if files.len() == 3 {
                break;
            }
        }
        files
    }
}
