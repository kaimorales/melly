//! Durable, local composer drafts. Drafts are never model context until Send.
use crate::storage::Store;
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub content: String,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    pub editing_message_id: Option<String>,
}

impl Store {
    pub(crate) fn initialize_drafts(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS composer_drafts(key TEXT PRIMARY KEY,chat_id TEXT REFERENCES chats(id) ON DELETE CASCADE,content TEXT NOT NULL,attachments TEXT NOT NULL,editing_message_id TEXT REFERENCES messages(id) ON DELETE SET NULL);")?;
        Ok(())
    }
    pub fn composer_draft(&self, chat: Option<&str>) -> Result<Draft> {
        if let Some(chat) = chat {
            self.chat(chat)?;
        }
        Ok(self
            .conn
            .query_row(
                "SELECT content,attachments,editing_message_id FROM composer_drafts WHERE key=?1",
                [chat.unwrap_or("new")],
                |row| {
                    Ok(Draft {
                        content: row.get(0)?,
                        attachment_ids: serde_json::from_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        editing_message_id: row.get(2)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }
    pub fn save_composer_draft(&self, chat: Option<&str>, draft: &Draft) -> Result<()> {
        if let Some(chat) = chat {
            self.chat(chat)?;
        }
        ensure!(
            draft.content.len() <= 32_000 && !draft.content.contains('\0'),
            "Drafts support up to 32,000 bytes"
        );
        ensure!(draft.attachment_ids.len() <= 8, "Attach up to eight files");
        for id in &draft.attachment_ids {
            uuid::Uuid::parse_str(id)?;
        }
        if let Some(message) = &draft.editing_message_id {
            ensure!(
                Some(self.message(message)?.chat_id.as_str()) == chat,
                "The edited message belongs to another chat"
            );
        }
        if draft.content.is_empty()
            && draft.attachment_ids.is_empty()
            && draft.editing_message_id.is_none()
        {
            self.conn.execute(
                "DELETE FROM composer_drafts WHERE key=?1",
                [chat.unwrap_or("new")],
            )?;
        } else {
            self.conn.execute("INSERT INTO composer_drafts(key,chat_id,content,attachments,editing_message_id) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(key) DO UPDATE SET content=excluded.content,attachments=excluded.attachments,editing_message_id=excluded.editing_message_id",params![chat.unwrap_or("new"),chat,draft.content,serde_json::to_string(&draft.attachment_ids)?,draft.editing_message_id])?;
        }
        Ok(())
    }
    pub fn consume_composer_draft(&self, key: &str, sent: &str) -> Result<()> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT content FROM composer_drafts WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(text) = text
            && text.trim() == sent.trim()
        {
            self.conn.execute(
                "DELETE FROM composer_drafts WHERE key=?1 AND content=?2",
                params![key, text],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drafts_survive_restart_stay_scoped_and_clear_only_after_matching_send() -> Result<()> {
        let path = std::env::temp_dir().join(format!("melly-drafts-{}.db", uuid::Uuid::new_v4()));
        let store = Store::open(&path)?;
        let a = store.create_chat(None)?;
        store.insert_message(&a.id, "user", "synthetic", "complete", None, None, None)?;
        let b = store.create_chat(None)?;
        store.save_composer_draft(
            Some(&a.id),
            &Draft {
                content: "draft A".into(),
                ..Default::default()
            },
        )?;
        store.save_composer_draft(
            Some(&b.id),
            &Draft {
                content: "draft B".into(),
                ..Default::default()
            },
        )?;
        store.save_composer_draft(
            None,
            &Draft {
                content: "new chat".into(),
                ..Default::default()
            },
        )?;
        drop(store);
        let store = Store::open(&path)?;
        assert_eq!(store.composer_draft(Some(&a.id))?.content, "draft A");
        assert_eq!(store.composer_draft(Some(&b.id))?.content, "draft B");
        assert_eq!(store.composer_draft(None)?.content, "new chat");
        store.consume_composer_draft(&a.id, "older draft")?;
        assert_eq!(store.composer_draft(Some(&a.id))?.content, "draft A");
        store.consume_composer_draft(&a.id, "draft A")?;
        assert!(store.composer_draft(Some(&a.id))?.content.is_empty());
        store.delete_chat(&b.id)?;
        assert!(store.composer_draft(Some(&b.id)).is_err());
        drop(store);
        std::fs::remove_file(path)?;
        Ok(())
    }
}
