//! Explicit, credential-free snapshots. Restore always creates new identities.
use crate::{models::*, storage::Store};
use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};
const MAX_BYTES: usize = 48 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    version: u32,
    folders: Vec<Folder>,
    chats: Vec<Chat>,
    messages: Vec<Message>,
    files: Vec<File>,
    attachments: Vec<Attachment>,
}
#[derive(Serialize, Deserialize)]
struct File {
    id: String,
    folder: String,
    name: String,
    data: String,
    chunks: Vec<Chunk>,
}
#[derive(Serialize, Deserialize)]
struct Chunk {
    id: String,
    page: Option<u32>,
    first: u32,
    last: u32,
    text: String,
}
#[derive(Serialize, Deserialize)]
struct Attachment {
    id: String,
    name: String,
    kind: String,
    text: String,
    data: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub projects: usize,
    pub chats: usize,
    pub messages: usize,
    pub files: usize,
    pub attachments: usize,
}
impl Backup {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 96 * 1024 * 1024, "Backup exceeds 96 MiB");
        let backup: Self =
            serde_json::from_slice(bytes).context("Choose a Melly backup JSON file")?;
        backup.validate()?;
        Ok(backup)
    }
    pub fn summary(&self) -> BackupSummary {
        BackupSummary {
            projects: self.folders.len(),
            chats: self.chats.len(),
            messages: self.messages.len(),
            files: self.files.len(),
            attachments: self.attachments.len(),
        }
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "This backup version is not supported");
        ensure!(
            self.folders.len() <= 500
                && self.chats.len() <= 5000
                && self.messages.len() <= 20000
                && self.files.len() <= 5000
                && self.attachments.len() <= 2000,
            "Backup contains too many items"
        );
        let unique = |ids: Vec<&str>| -> Result<HashSet<String>> {
            let mut set = HashSet::new();
            for id in ids {
                crate::projects::valid_id(id)?;
                ensure!(set.insert(id.to_owned()), "Duplicate identity in backup");
            }
            Ok(set)
        };
        let folders = unique(self.folders.iter().map(|f| f.id.as_str()).collect())?;
        let chats = unique(self.chats.iter().map(|c| c.id.as_str()).collect())?;
        unique(self.messages.iter().map(|m| m.id.as_str()).collect())?;
        unique(self.files.iter().map(|f| f.id.as_str()).collect())?;
        let attachments = unique(self.attachments.iter().map(|a| a.id.as_str()).collect())?;
        for folder in &self.folders {
            ensure!(
                !folder.title.trim().is_empty()
                    && folder.title.chars().count() <= 80
                    && !folder.deleting,
                "Invalid project in backup"
            );
        }
        for chat in &self.chats {
            ensure!(
                (0..=4_102_444_800_000i64).contains(&chat.updated_at),
                "Invalid chat timestamp"
            );
            ensure!(
                !chat.title.trim().is_empty()
                    && chat.title.chars().count() <= 120
                    && chat
                        .folder_id
                        .as_ref()
                        .is_none_or(|id| folders.contains(id)),
                "Invalid chat in backup"
            );
        }
        let mut total = 0;
        for message in &self.messages {
            ensure!(
                (0..=4_102_444_800_000i64).contains(&message.created_at),
                "Invalid message timestamp"
            );
            ensure!(
                chats.contains(&message.chat_id)
                    && matches!(message.role.as_str(), "user" | "assistant")
                    && message.content.len() + message.reasoning.len() <= 2 * 1024 * 1024
                    && message.sources.len() <= 100
                    && message.attachments.len() <= 8,
                "Invalid message in backup"
            );
            ensure!(
                message
                    .attachments
                    .iter()
                    .all(|a| attachments.contains(&a.id)),
                "Missing attachment in backup"
            );
            ensure!(
                message.sources.iter().all(|s| s.snippet.len() <= 16000
                    && s.title.len() <= 1000
                    && s.id.len() <= 300
                    && s.url.as_ref().is_none_or(|u| u.len() <= 4096)),
                "Invalid reference in backup"
            );
            total += message.content.len()
                + message.reasoning.len()
                + serde_json::to_vec(&message.sources)?.len();
        }
        let mut chunk_ids = HashSet::new();
        for file in &self.files {
            ensure!(
                folders.contains(&file.folder) && file.chunks.len() <= 2000,
                "Invalid file project"
            );
            crate::projects::valid_name(&file.name)?;
            let bytes = STANDARD.decode(&file.data).context("Invalid file data")?;
            ensure!(
                !bytes.is_empty() && bytes.len() <= 20 * 1024 * 1024,
                "Invalid file size"
            );
            total += bytes.len();
            for chunk in &file.chunks {
                crate::projects::valid_id(&chunk.id)?;
                ensure!(
                    chunk_ids.insert(chunk.id.clone())
                        && chunk.text.len() <= 16000
                        && chunk.first <= chunk.last
                        && chunk.page.is_none_or(|p| (1..=500).contains(&p)),
                    "Invalid passage in backup"
                );
                total += chunk.text.len();
            }
        }
        for attachment in &self.attachments {
            crate::projects::valid_name(&attachment.name)?;
            ensure!(
                matches!(attachment.kind.as_str(), "text" | "pdf" | "image-text")
                    && !attachment.text.trim().is_empty()
                    && attachment.text.len() <= 2 * 1024 * 1024
                    && !attachment.text.contains('\0'),
                "Invalid attachment text"
            );
            let bytes = STANDARD
                .decode(&attachment.data)
                .context("Invalid attachment data")?;
            ensure!(
                !bytes.is_empty() && bytes.len() <= 20 * 1024 * 1024,
                "Invalid attachment size"
            );
            total += bytes.len() + attachment.text.len();
        }
        ensure!(
            total <= MAX_BYTES,
            "Backup content exceeds 48 MiB. Export individual projects instead."
        );
        Ok(())
    }
}
impl Store {
    pub fn export_chat_markdown(&self, id: &str) -> Result<String> {
        let chat = self.chat(id)?;
        let messages = self.portable_messages(Some(id), None)?;
        let mut output = format!("# {}\n\n", chat.title);
        for message in messages {
            output.push_str(if message.role == "user" {
                "## You\n\n"
            } else {
                "## Assistant\n\n"
            });
            output.push_str(&message.content);
            output.push_str("\n\n");
            if !message.attachments.is_empty() {
                output.push_str("Attached files: ");
                output.push_str(
                    &message
                        .attachments
                        .iter()
                        .map(|f| f.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                output.push_str("\n\n");
            }
            for source in message.sources {
                output.push_str(&format!("Source: {}\n{}\n\n", source.title, source.snippet));
            }
        }
        Ok(output)
    }
    fn portable_messages(&self, chat: Option<&str>, folder: Option<&str>) -> Result<Vec<Message>> {
        let mut statement=self.conn.prepare("SELECT id FROM messages WHERE (?1 IS NULL OR chat_id=?1) AND (?2 IS NULL OR chat_id IN(SELECT id FROM chats WHERE folder_id=?2)) ORDER BY created_at LIMIT 20001")?;
        let ids = statement
            .query_map(params![chat, folder], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            ids.len() <= 20000,
            "Export fewer than 20,000 messages at once"
        );
        let mut size = 0;
        let mut messages = Vec::new();
        for id in ids {
            let message = self.message(&id)?;
            size += message.content.len() + message.reasoning.len();
            ensure!(
                size <= MAX_BYTES,
                "Export a smaller conversation or project"
            );
            messages.push(message);
        }
        Ok(messages)
    }
    pub fn backup(
        &self,
        folder: Option<&str>,
        read: impl Fn(&Path) -> Result<Vec<u8>>,
    ) -> Result<Backup> {
        if let Some(folder) = folder {
            self.folder(folder)?;
        }
        let folder_ids = self
            .conn
            .prepare("SELECT id FROM folders WHERE ?1 IS NULL OR id=?1 LIMIT 501")?
            .query_map([folder], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(folder_ids.len() <= 500, "Export fewer projects at once");
        let folders = folder_ids
            .iter()
            .map(|id| self.folder(id))
            .collect::<Result<Vec<_>>>()?;
        let chat_ids = self
            .conn
            .prepare("SELECT id FROM chats WHERE ?1 IS NULL OR folder_id=?1 LIMIT 5001")?
            .query_map([folder], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(chat_ids.len() <= 5000, "Export fewer chats at once");
        let chats = chat_ids
            .iter()
            .map(|id| self.chat(id))
            .collect::<Result<Vec<_>>>()?;
        let messages = self.portable_messages(None, folder)?;
        let mut size = messages
            .iter()
            .map(|m| m.content.len() + m.reasoning.len())
            .sum::<usize>();
        let mut files = Vec::new();
        for folder in &folders {
            for file in self.project_files(&folder.id)? {
                if file.linked_path.is_some() {
                    continue;
                }
                ensure!(
                    size + file.bytes as usize <= MAX_BYTES,
                    "Export a smaller project: backup exceeds 48 MiB"
                );
                let bytes = read(&self.managed_file_path(&file.id)?)?;
                ensure!(
                    crate::linked::revision(&bytes) == file.revision,
                    "An imported file changed. Replace it before exporting."
                );
                size += bytes.len();
                let chunks=self.conn.prepare("SELECT id,page,first_line,last_line,content FROM project_chunks WHERE file_id=?1 ORDER BY rowid")?.query_map([&file.id],|r|Ok(Chunk{id:r.get(0)?,page:r.get(1)?,first:r.get(2)?,last:r.get(3)?,text:r.get(4)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;
                size += chunks.iter().map(|c| c.text.len()).sum::<usize>();
                ensure!(size <= MAX_BYTES, "Export a smaller project");
                files.push(File {
                    id: file.id,
                    folder: folder.id.clone(),
                    name: file.name,
                    data: STANDARD.encode(bytes),
                    chunks,
                });
            }
        }
        let ids: HashSet<_> = messages
            .iter()
            .flat_map(|m| m.attachments.iter().map(|a| a.id.clone()))
            .collect();
        let mut attachments = Vec::new();
        for id in ids {
            let length: usize = self.conn.query_row(
                "SELECT length(bytes)+length(content) FROM attachments WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            ensure!(size + length <= MAX_BYTES, "Export a smaller project");
            let (meta, bytes) = self.attachment_content(&id)?;
            let text: String =
                self.conn
                    .query_row("SELECT content FROM attachments WHERE id=?1", [&id], |r| {
                        r.get(0)
                    })?;
            size += bytes.len() + text.len();
            attachments.push(Attachment {
                id,
                name: meta.name,
                kind: meta.kind,
                text,
                data: STANDARD.encode(bytes),
            });
        }
        let backup = Backup {
            version: 1,
            folders,
            chats,
            messages,
            files,
            attachments,
        };
        backup.validate()?;
        Ok(backup)
    }
    pub fn restore_backup(&self, backup: Backup) -> Result<BackupSummary> {
        backup.validate()?;
        let summary = backup.summary();
        let tx = self.conn.unchecked_transaction()?;
        let mut map = HashMap::<String, String>::new();
        for id in backup
            .folders
            .iter()
            .map(|f| &f.id)
            .chain(backup.chats.iter().map(|c| &c.id))
            .chain(backup.messages.iter().map(|m| &m.id))
            .chain(backup.files.iter().map(|f| &f.id))
            .chain(
                backup
                    .files
                    .iter()
                    .flat_map(|f| f.chunks.iter().map(|c| &c.id)),
            )
            .chain(backup.attachments.iter().map(|a| &a.id))
        {
            ensure!(
                map.insert(id.clone(), uuid::Uuid::new_v4().to_string())
                    .is_none(),
                "Conflicting backup identities"
            );
        }
        let mut created = Vec::new();
        let result = (|| {
            for folder in &backup.folders {
                tx.execute(
                    "INSERT INTO folders VALUES(?1,?2)",
                    params![map[&folder.id], folder.title],
                )?;
                tx.execute(
                    "INSERT INTO project_state(folder_id,archived,deleting) VALUES(?1,?2,0)",
                    params![map[&folder.id], folder.archived],
                )?;
                // Restoring data never silently restores permission to send project context.
                tx.execute(
                    "INSERT INTO project_context VALUES(?1,0)",
                    [&map[&folder.id]],
                )?;
            }
            for chat in &backup.chats {
                tx.execute(
                    "INSERT INTO chats VALUES(?1,?2,?3,?4)",
                    params![
                        map[&chat.id],
                        chat.title,
                        chat.folder_id.as_ref().map(|id| &map[id]),
                        chat.updated_at
                    ],
                )?;
            }
            for attachment in &backup.attachments {
                tx.execute(
                    "INSERT INTO attachments VALUES(?1,?2,?3,?4,?5)",
                    params![
                        map[&attachment.id],
                        attachment.name,
                        attachment.kind,
                        attachment.text,
                        STANDARD.decode(&attachment.data)?
                    ],
                )?;
            }
            for file in &backup.files {
                let root =
                    crate::projects::project_directory(&self.project_root()?, &map[&file.folder])?;
                if !created.contains(&root) {
                    created.push(root.clone());
                }
                let directory = root.join(&map[&file.id]);
                std::fs::create_dir(&directory)?;
                let bytes = STANDARD.decode(&file.data)?;
                let path = directory.join(&file.name);
                {
                    use std::io::Write;
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut output = options.open(path)?;
                    output.write_all(&bytes)?;
                    output.sync_all()?;
                }
                tx.execute(
                    "INSERT INTO project_files VALUES(?1,?2,?3,?4,?5)",
                    params![
                        map[&file.id],
                        map[&file.folder],
                        file.name,
                        bytes.len(),
                        crate::linked::revision(&bytes)
                    ],
                )?;
                for chunk in &file.chunks {
                    tx.execute(
                        "INSERT INTO project_chunks VALUES(?1,?2,?3,?4,?5,?6)",
                        params![
                            map[&chunk.id],
                            map[&file.id],
                            chunk.page,
                            chunk.first,
                            chunk.last,
                            chunk.text
                        ],
                    )?;
                }
            }
            for message in &backup.messages {
                let mut sources = message.sources.clone();
                let mut content = message.content.clone();
                for source in &mut sources {
                    let old = source.id.clone();
                    if let Some(id) = source.id.strip_prefix("file:") {
                        source.id =
                            map.get(id)
                                .map(|id| format!("file:{id}"))
                                .unwrap_or_else(|| {
                                    format!("file:unavailable-{}", uuid::Uuid::new_v4())
                                });
                    }
                    if let Some(id) = &source.message_id {
                        if let Some(mapped) = map.get(id) {
                            source.id = source.id.replace(id, mapped);
                            source.message_id = Some(mapped.clone());
                        } else {
                            source.message_id = None;
                        }
                    }
                    source.chat_id = source.chat_id.as_ref().and_then(|id| map.get(id).cloned());
                    if source.id != old {
                        content = content.replace(
                            &format!("melly-source:{old}"),
                            &format!("melly-source:{}", source.id),
                        );
                    }
                }
                let attachments = message
                    .attachments
                    .iter()
                    .map(|a| AttachmentInfo {
                        id: map[&a.id].clone(),
                        ..a.clone()
                    })
                    .collect::<Vec<_>>();
                tx.execute("INSERT INTO messages(id,chat_id,role,content,status,sources,created_at,provider_id,model_id,billing_route,reasoning,attachments) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![map[&message.id],map[&message.chat_id],message.role,content,if message.status=="streaming" {"interrupted"} else {&message.status},serde_json::to_string(&sources)?,message.created_at,message.provider_id,message.model_id,message.billing_route,message.reasoning,serde_json::to_string(&attachments)?])?;
                for attachment in attachments {
                    tx.execute(
                        "INSERT INTO attachment_messages VALUES(?1,?2)",
                        params![attachment.id, map[&message.id]],
                    )?;
                }
            }
            Ok::<_, anyhow::Error>(())
        })();
        if let Err(reason) = result {
            for root in created {
                let _ = std::fs::remove_dir_all(root);
            }
            return Err(reason);
        }
        if let Err(reason) = tx.commit() {
            for root in created {
                let _ = std::fs::remove_dir_all(root);
            }
            return Err(reason.into());
        }
        Ok(summary)
    }
}
