//! Local undo journal. Snapshots never enter model context or exported conversations.
use crate::{file_edits::MAX_FILE_BYTES, projects::ProjectLink, storage::Store};
use anyhow::{Result, ensure};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub message: String,
    pub folder: String,
    pub root: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub mode: Option<u32>,
    pub state: String,
}

impl Store {
    pub(crate) fn initialize_file_history(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS file_history(sequence INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE, message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE, data TEXT NOT NULL, state TEXT NOT NULL); CREATE INDEX IF NOT EXISTS file_history_message ON file_history(message_id,sequence);")?;
        Ok(())
    }

    pub fn checkpoint_file(
        &self,
        message: &str,
        workspace: &ProjectLink,
        path: &str,
        before: Option<&str>,
        after: Option<&str>,
        mode: Option<u32>,
    ) -> Result<String> {
        ensure!(before != after, "The proposed file change has no effect");
        ensure!(
            [before, after]
                .iter()
                .flatten()
                .all(|value| value.len() <= MAX_FILE_BYTES),
            "File snapshot is too large"
        );
        crate::file_edits::validate_path(path)?;
        let owner = self.message(message)?;
        ensure!(
            owner.role == "assistant"
                && self.chat(&owner.chat_id)?.folder_id.as_deref() == Some(&workspace.folder_id),
            "The file change is outside this chat's project"
        );
        let change = Change {
            id: uuid::Uuid::new_v4().to_string(),
            message: message.into(),
            folder: workspace.folder_id.clone(),
            root: workspace.path.clone(),
            device: workspace.device,
            inode: workspace.inode,
            path: path.into(),
            before: before.map(str::to_owned),
            after: after.map(str::to_owned),
            mode,
            state: "prepared".into(),
        };
        self.conn.execute(
            "INSERT INTO file_history(id,message_id,data,state) VALUES(?1,?2,?3,'prepared')",
            params![change.id, message, serde_json::to_string(&change)?],
        )?;
        Ok(change.id)
    }

    pub fn set_file_change_state(&self, id: &str, state: &str) -> Result<()> {
        ensure!(
            ["applied", "undoing", "undone", "failed"].contains(&state),
            "Invalid file change state"
        );
        ensure!(
            self.conn.execute(
                "UPDATE file_history SET state=?2 WHERE id=?1",
                params![id, state]
            )? == 1,
            "File change not found"
        );
        Ok(())
    }

    pub fn set_file_change_states(&self, ids: &[&str], state: &str) -> Result<()> {
        ensure!(["undoing", "undone"].contains(&state), "Invalid undo state");
        let tx = self.conn.unchecked_transaction()?;
        for id in ids {
            ensure!(
                tx.execute(
                    "UPDATE file_history SET state=?2 WHERE id=?1",
                    params![id, state]
                )? == 1,
                "File change not found"
            );
        }
        tx.commit()?;
        Ok(())
    }

    pub fn file_changes(&self, chat: &str, message: &str) -> Result<Vec<Change>> {
        ensure!(
            self.message(message)?.chat_id == chat,
            "File change belongs to another chat"
        );
        let mut query = self.conn.prepare("SELECT data,state FROM file_history WHERE message_id=?1 ORDER BY sequence DESC LIMIT 64")?;
        query
            .query_map([message], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .map(|entry| {
                let (data, state) = entry?;
                let mut change: Change = serde_json::from_str(&data)?;
                change.state = state;
                Ok(change)
            })
            .collect()
    }

    pub fn has_later_file_change(&self, change: &Change) -> Result<bool> {
        let mut query = self.conn.prepare("SELECT data FROM file_history WHERE sequence > (SELECT sequence FROM file_history WHERE id=?1) AND state IN ('prepared','applied','undoing')")?;
        for entry in query.query_map([&change.id], |row| row.get::<_, String>(0))? {
            let other: Change = serde_json::from_str(&entry?)?;
            if other.message != change.message
                && other.root == change.root
                && other.device == change.device
                && other.inode == change.inode
                && other.path == change.path
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
