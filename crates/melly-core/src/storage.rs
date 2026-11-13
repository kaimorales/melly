use crate::models::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection as SqlConnection, OptionalExtension, Row, params};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct Store {
    conn: SqlConnection,
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn bounded(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
fn title(value: &str, max: usize) -> Result<String> {
    let v = value.trim();
    ensure!(!v.is_empty(), "A name is required");
    ensure!(v.chars().count() <= max, "Name is too long");
    Ok(v.into())
}
fn row_chat(r: &Row<'_>) -> rusqlite::Result<Chat> {
    Ok(Chat {
        id: r.get(0)?,
        title: r.get(1)?,
        folder_id: r.get(2)?,
        updated_at: r.get(3)?,
    })
}
fn row_message(r: &Row<'_>) -> rusqlite::Result<Message> {
    let raw: String = r.get(5)?;
    Ok(Message {
        id: r.get(0)?,
        chat_id: r.get(1)?,
        role: r.get(2)?,
        content: r.get(3)?,
        status: r.get(4)?,
        sources: serde_json::from_str(&raw).unwrap_or_default(),
        created_at: r.get(6)?,
        provider_id: r.get(7)?,
        model_id: r.get(8)?,
        billing_route: r.get(9)?,
    })
}
const MESSAGE_COLUMNS: &str =
    "id,chat_id,role,content,status,sources,created_at,provider_id,model_id,billing_route";
fn fts_query(q: &str) -> String {
    let stop = [
        "the",
        "a",
        "an",
        "my",
        "other",
        "chat",
        "chats",
        "search",
        "folder",
        "for",
        "in",
        "look",
        "please",
        "and",
        "what",
        "was",
        "from",
        "about",
        "strongest",
    ];
    q.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && !stop.contains(&s.to_lowercase().as_str()))
        .take(16)
        .map(|s| format!("\"{}\"", bounded(s, 80)))
        .collect::<Vec<_>>()
        .join(" OR ")
}
impl Store {
    pub fn memory() -> Result<Self> {
        Self::initialize(SqlConnection::open_in_memory()?)
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let result = Self::initialize(SqlConnection::open(path)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(result)
    }
    fn initialize(conn: SqlConnection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(2))?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA cache_size=-2000;
            CREATE TABLE IF NOT EXISTS folders(id TEXT PRIMARY KEY,title TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS chats(id TEXT PRIMARY KEY,title TEXT NOT NULL,folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,updated_at INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS chats_folder ON chats(folder_id,updated_at DESC);
            CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY,chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,role TEXT NOT NULL,content TEXT NOT NULL,status TEXT NOT NULL,sources TEXT NOT NULL DEFAULT '[]',created_at INTEGER NOT NULL,provider_id TEXT,model_id TEXT,billing_route TEXT);
            CREATE INDEX IF NOT EXISTS messages_chat ON messages(chat_id,created_at DESC);
            CREATE INDEX IF NOT EXISTS messages_created ON messages(created_at DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(content,content='messages',content_rowid='rowid');
            CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN INSERT INTO messages_fts(rowid,content) VALUES(new.rowid,new.content); END;
            CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN INSERT INTO messages_fts(messages_fts,rowid,content) VALUES('delete',old.rowid,old.content); END;
            CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF content ON messages BEGIN INSERT INTO messages_fts(messages_fts,rowid,content) VALUES('delete',old.rowid,old.content); INSERT INTO messages_fts(rowid,content) VALUES(new.rowid,new.content); END;
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS connections(provider_id TEXT PRIMARY KEY,base_url TEXT NOT NULL,model_id TEXT NOT NULL);
            UPDATE messages SET status='interrupted' WHERE status='streaming'; PRAGMA user_version=1;")?;
        Ok(Self { conn })
    }
    fn timestamp(&self) -> Result<i64> {
        let last: i64 = self.conn.query_row(
            "SELECT coalesce(max(created_at),0) FROM messages",
            [],
            |r| r.get(0),
        )?;
        Ok(now().max(last + 1))
    }
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut s = self
            .conn
            .prepare("SELECT id,title FROM folders ORDER BY title COLLATE NOCASE LIMIT 500")?;
        Ok(s.query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                title: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
    }
    pub fn create_folder(&self, name: &str) -> Result<Folder> {
        let folder = Folder {
            id: Uuid::new_v4().to_string(),
            title: title(name, 80)?,
        };
        self.conn.execute(
            "INSERT INTO folders VALUES(?1,?2)",
            params![folder.id, folder.title],
        )?;
        Ok(folder)
    }
    pub fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        ensure!(
            self.conn.execute(
                "UPDATE folders SET title=?1 WHERE id=?2",
                params![title(name, 80)?, id]
            )? == 1,
            "Folder not found"
        );
        Ok(())
    }
    pub fn chats(&self) -> Result<Vec<Chat>> {
        let mut s = self.conn.prepare(
            "SELECT id,title,folder_id,updated_at FROM chats ORDER BY updated_at DESC LIMIT 500",
        )?;
        Ok(s.query_map([], row_chat)?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn create_chat(&self, folder: Option<&str>) -> Result<Chat> {
        let c = Chat {
            id: Uuid::new_v4().to_string(),
            title: "New chat".into(),
            folder_id: folder.map(str::to_string),
            updated_at: now(),
        };
        self.conn.execute(
            "INSERT INTO chats VALUES(?1,?2,?3,?4)",
            params![c.id, c.title, c.folder_id, c.updated_at],
        )?;
        Ok(c)
    }
    pub fn chat(&self, id: &str) -> Result<Chat> {
        self.conn
            .query_row(
                "SELECT id,title,folder_id,updated_at FROM chats WHERE id=?1",
                [id],
                row_chat,
            )
            .optional()?
            .context("Chat not found")
    }
    pub fn rename_chat(&self, id: &str, name: &str) -> Result<()> {
        ensure!(
            self.conn.execute(
                "UPDATE chats SET title=?1,updated_at=?2 WHERE id=?3",
                params![title(name, 120)?, now(), id]
            )? == 1,
            "Chat not found"
        );
        Ok(())
    }
    pub fn move_chat(&self, id: &str, folder: Option<&str>) -> Result<()> {
        ensure!(
            self.conn.execute(
                "UPDATE chats SET folder_id=?1,updated_at=?2 WHERE id=?3",
                params![folder, now(), id]
            )? == 1,
            "Chat not found"
        );
        Ok(())
    }
    pub fn delete_chat(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM chats WHERE id=?1", [id])?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn insert_message(
        &self,
        chat_id: &str,
        role: &str,
        content: &str,
        status: &str,
        provider: Option<&str>,
        model: Option<&str>,
        route: Option<&str>,
    ) -> Result<Message> {
        ensure!(matches!(role, "user" | "assistant"), "Invalid message role");
        let msg = Message {
            id: Uuid::new_v4().to_string(),
            chat_id: chat_id.into(),
            role: role.into(),
            content: content.into(),
            status: status.into(),
            sources: vec![],
            created_at: self.timestamp()?,
            provider_id: provider.map(str::to_string),
            model_id: model.map(str::to_string),
            billing_route: route.map(str::to_string),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO messages(id,chat_id,role,content,status,created_at,provider_id,model_id,billing_route) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![msg.id,msg.chat_id,msg.role,msg.content,msg.status,msg.created_at,msg.provider_id,msg.model_id,msg.billing_route])?;
        tx.execute(
            "UPDATE chats SET updated_at=?1 WHERE id=?2",
            params![msg.created_at, chat_id],
        )?;
        tx.commit()?;
        Ok(msg)
    }
    pub fn finish_message(
        &self,
        id: &str,
        content: &str,
        status: &str,
        sources: &[Source],
    ) -> Result<()> {
        ensure!(
            matches!(status, "complete" | "stopped" | "error" | "interrupted"),
            "Invalid final status"
        );
        ensure!(
            self.conn.execute(
                "UPDATE messages SET content=?1,status=?2,sources=?3 WHERE id=?4",
                params![content, status, serde_json::to_string(sources)?, id]
            )? == 1,
            "Message not found"
        );
        Ok(())
    }
    pub fn message(&self, id: &str) -> Result<Message> {
        self.conn
            .query_row(
                &format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE id=?1"),
                [id],
                row_message,
            )
            .optional()?
            .context("Message not found")
    }
    pub fn messages(&self, chat_id: &str, before: Option<i64>) -> Result<Vec<Message>> {
        let mut stmt=self.conn.prepare(&format!("SELECT {MESSAGE_COLUMNS} FROM messages WHERE chat_id=?1 AND created_at<?2 ORDER BY created_at DESC LIMIT 60"))?;
        let mut rows = stmt
            .query_map(params![chat_id, before.unwrap_or(i64::MAX)], row_message)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.reverse();
        Ok(rows)
    }
    pub fn context(&self, chat_id: &str) -> Result<Vec<WireMessage>> {
        let messages = self.messages(chat_id, None)?;
        let mut budget = 60_000;
        let mut wire = Vec::new();
        for m in messages
            .into_iter()
            .rev()
            .filter(|m| m.status == "complete")
        {
            let n = m.content.len();
            if n > budget {
                break;
            }
            budget -= n;
            wire.push(WireMessage::text(&m.role, m.content));
        }
        wire.reverse();
        Ok(wire)
    }
    pub fn retry_user_message(&self, chat_id: &str) -> Result<String> {
        let history = self.messages(chat_id, None)?;
        let last = history.last().context("Nothing to retry")?;
        ensure!(
            last.role == "assistant",
            "Only the last reply can be retried"
        );
        let user = history
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .context("No user message to retry")?;
        self.conn
            .execute("DELETE FROM messages WHERE id=?1", [&last.id])?;
        Ok(user.content.clone())
    }
    pub fn search_folder(
        &self,
        current_chat: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Source>> {
        let folder = self
            .chat(current_chat)?
            .folder_id
            .context("This chat is not in a folder")?;
        let q = fts_query(query);
        if q.is_empty() {
            return Ok(vec![]);
        }
        let mut s=self.conn.prepare("SELECT m.id,m.chat_id,c.title,snippet(messages_fts,0,'','',' … ',40) FROM messages_fts JOIN messages m ON messages_fts.rowid=m.rowid JOIN chats c ON c.id=m.chat_id WHERE messages_fts MATCH ?1 AND c.folder_id=?2 AND c.id<>?3 AND m.status='complete' ORDER BY bm25(messages_fts),m.created_at DESC LIMIT ?4")?;
        Ok(s.query_map(
            params![q, folder, current_chat, limit.clamp(1, 5) as i64],
            |r| {
                let id: String = r.get(0)?;
                let snippet: String = r.get(3)?;
                Ok(Source {
                    id: format!("chat:{id}"),
                    kind: "chat".into(),
                    title: r.get(2)?,
                    url: None,
                    chat_id: Some(r.get(1)?),
                    message_id: Some(id),
                    snippet: bounded(&snippet, 600),
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()?)
    }
    pub fn search_chats(&self, query: &str) -> Result<Vec<Chat>> {
        let q = query.trim();
        if q.is_empty() {
            return self.chats();
        }
        ensure!(q.chars().count() <= 500, "Search is too long");
        let literal = format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let fts = fts_query(q);
        let sql = if fts.is_empty() {
            "SELECT id,title,folder_id,updated_at FROM chats WHERE title LIKE ?1 ESCAPE '\' ORDER BY updated_at DESC LIMIT 100"
        } else {
            "SELECT id,title,folder_id,updated_at FROM chats WHERE title LIKE ?1 ESCAPE '\' OR id IN (SELECT m.chat_id FROM messages_fts JOIN messages m ON m.rowid=messages_fts.rowid WHERE messages_fts MATCH ?2) ORDER BY updated_at DESC LIMIT 100"
        };
        let mut s = self.conn.prepare(sql)?;
        let result = if fts.is_empty() {
            s.query_map(params![literal], row_chat)?
                .collect::<rusqlite::Result<_>>()?
        } else {
            s.query_map(params![literal, fts], row_chat)?
                .collect::<rusqlite::Result<_>>()?
        };
        Ok(result)
    }
    pub fn settings(&self) -> Result<Settings> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='preferences'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match raw {
            Some(s) => Ok(serde_json::from_str(&s)?),
            None => Ok(Settings::default()),
        }
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        ensure!(
            matches!(s.theme.as_str(), "system" | "light" | "dark"),
            "Invalid theme"
        );
        ensure!([14, 16, 18].contains(&s.font_size), "Invalid text size");
        ensure!(
            matches!(s.send_behavior.as_str(), "enter" | "cmd-enter"),
            "Invalid send behavior"
        );
        ensure!(
            s.system_prompt.chars().count() <= 8000,
            "System prompt must be 8000 characters or fewer"
        );
        self.conn.execute("INSERT INTO settings VALUES('preferences',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(s)?])?;
        Ok(())
    }
    pub fn connection(&self, id: &str) -> Result<Option<Connection>> {
        Ok(self
            .conn
            .query_row(
                "SELECT provider_id,base_url,model_id FROM connections WHERE provider_id=?1",
                [id],
                |r| {
                    Ok(Connection {
                        provider_id: r.get(0)?,
                        base_url: r.get(1)?,
                        model_id: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }
    pub fn save_connection(&self, c: &Connection) -> Result<()> {
        ensure!(
            !c.model_id.trim().is_empty() && c.model_id.len() <= 200,
            "Model ID is required and must be 200 characters or fewer"
        );
        self.conn.execute("INSERT INTO connections VALUES(?1,?2,?3) ON CONFLICT(provider_id) DO UPDATE SET base_url=excluded.base_url,model_id=excluded.model_id",params![c.provider_id,c.base_url,c.model_id])?;
        Ok(())
    }
    pub fn seed_demo(&self) -> Result<()> {
        if self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='demo_seeded'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_some()
        {
            return Ok(());
        }
        let folder = self.create_folder("Job search")?;
        let first = self.create_chat(Some(&folder.id))?;
        self.rename_chat(&first.id, "My experience")?;
        self.insert_message(&first.id,"user","My transit project improved ridership forecast accuracy by 18%. I explained the findings to a nontechnical planning team.","complete",None,None,None)?;
        self.insert_message(&first.id,"assistant","That is a useful interview story: a clear problem, a measured result, and an audience who could act on it.","complete",Some("mock"),Some("melly-demo"),Some("Mock"))?;
        let second = self.create_chat(Some(&folder.id))?;
        self.rename_chat(&second.id, "Interview prep")?;
        let other = self.create_folder("Writing")?;
        let private = self.create_chat(Some(&other.id))?;
        self.rename_chat(&private.id, "A different folder")?;
        self.insert_message(&private.id,"user","Synthetic boundary test: the project secret is CERULEAN-999. Job search must never retrieve this message.","complete",None,None,None)?;
        self.conn
            .execute("INSERT INTO settings VALUES('demo_seeded','1')", [])?;
        Ok(())
    }
    pub fn clear_connection(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM connections WHERE provider_id=?1", [id])?;
        Ok(())
    }
}
