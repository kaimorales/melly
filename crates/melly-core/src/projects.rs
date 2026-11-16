//! Project files are owned by a folder ID. Paths never come from model arguments.
use crate::{models::*, storage::Store};
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_FILES: usize = 500;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub id: String,
    pub folder_id: String,
    pub name: String,
    pub bytes: u64,
    pub revision: String,
    pub linked_path: Option<String>,
}

pub fn valid_id(value: &str) -> Result<()> {
    ensure!(
        Uuid::parse_str(value).is_ok(),
        "Invalid project file identifier"
    );
    Ok(())
}
pub fn valid_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 255
            && !value.contains(['/', '\\'])
            && value != "."
            && value != ".."
            && !value.chars().any(char::is_control),
        "Choose a file with a plain filename"
    );
    Ok(())
}
pub fn ensure_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir(path).context("Could not create the project directory")?;
    }
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Project directory must be a real directory"
    );
    Ok(())
}
pub fn project_directory(root: &Path, folder_id: &str) -> Result<PathBuf> {
    valid_id(folder_id)?;
    ensure_directory(root)?;
    let directory = root.join(folder_id);
    ensure_directory(&directory)?;
    Ok(directory)
}

impl Store {
    pub(crate) fn initialize_projects(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS project_links(folder_id TEXT PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE,path TEXT NOT NULL,device INTEGER NOT NULL,inode INTEGER NOT NULL,status TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS file_edit_scopes(message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,device INTEGER NOT NULL,inode INTEGER NOT NULL,applied INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS file_edit_diffs(message_id TEXT PRIMARY KEY REFERENCES file_edit_scopes(message_id) ON DELETE CASCADE, diff TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS coding_activity(id INTEGER PRIMARY KEY,message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,result TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS coding_activity_by_message ON coding_activity(message_id,id);
            CREATE TABLE IF NOT EXISTS project_context(folder_id TEXT PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE, enabled INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS project_state(folder_id TEXT PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE, archived INTEGER NOT NULL DEFAULT 0, deleting INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS project_files(
            id TEXT PRIMARY KEY, folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
            name TEXT NOT NULL, bytes INTEGER NOT NULL, revision TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS project_file_locations(file_id TEXT PRIMARY KEY REFERENCES project_files(id) ON DELETE CASCADE,relative_path TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS project_files_folder ON project_files(folder_id);
            CREATE TABLE IF NOT EXISTS project_chunks(id TEXT PRIMARY KEY,file_id TEXT NOT NULL REFERENCES project_files(id) ON DELETE CASCADE,page INTEGER,first_line INTEGER NOT NULL,last_line INTEGER NOT NULL,content TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS project_chunks_file ON project_chunks(file_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS project_chunks_fts USING fts5(content,content='project_chunks',content_rowid='rowid');
            CREATE TRIGGER IF NOT EXISTS project_chunks_ai AFTER INSERT ON project_chunks BEGIN INSERT INTO project_chunks_fts(rowid,content) VALUES(new.rowid,new.content); END;
            CREATE TRIGGER IF NOT EXISTS project_chunks_ad AFTER DELETE ON project_chunks BEGIN INSERT INTO project_chunks_fts(project_chunks_fts,rowid,content) VALUES('delete',old.rowid,old.content); END;",
        )?;
        Ok(())
    }
    pub fn folder(&self, id: &str) -> Result<Folder> {
        self.conn
            .query_row("SELECT f.id,f.title,coalesce(p.archived,0),coalesce(p.deleting,0),coalesce(c.enabled,0) FROM folders f LEFT JOIN project_state p ON p.folder_id=f.id LEFT JOIN project_context c ON c.folder_id=f.id WHERE f.id=?1", [id], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    archived: r.get(2)?, deleting: r.get(3)?,
                    context_enabled: r.get(4)?,
                })
            })
            .optional()?
            .context("Project not found")
    }
    pub fn project_root(&self) -> Result<PathBuf> {
        self.project_root
            .clone()
            .context("Project files need a persistent profile")
    }
    pub fn project_files(&self, folder_id: &str) -> Result<Vec<ProjectFile>> {
        self.folder(folder_id)?;
        let mut statement = self.conn.prepare("SELECT id,folder_id,name,bytes,revision,(SELECT relative_path FROM project_file_locations WHERE file_id=project_files.id) FROM project_files WHERE folder_id=?1 ORDER BY name COLLATE NOCASE LIMIT 500")?;
        Ok(statement
            .query_map([folder_id], |r| {
                Ok(ProjectFile {
                    id: r.get(0)?,
                    folder_id: r.get(1)?,
                    name: r.get(2)?,
                    bytes: r.get(3)?,
                    revision: r.get(4)?,
                    linked_path: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn project_file(&self, id: &str) -> Result<ProjectFile> {
        self.conn
            .query_row(
                "SELECT id,folder_id,name,bytes,revision,(SELECT relative_path FROM project_file_locations WHERE file_id=project_files.id) FROM project_files WHERE id=?1",
                [id],
                |r| {
                    Ok(ProjectFile {
                        id: r.get(0)?,
                        folder_id: r.get(1)?,
                        name: r.get(2)?,
                        bytes: r.get(3)?,
                        revision: r.get(4)?,
                linked_path: r.get(5)?,
                    })
                },
            )
            .optional()?
            .context("File is no longer available")
    }
    pub fn add_project_file(&self, file: &ProjectFile) -> Result<()> {
        self.ensure_active_project(&file.folder_id)?;
        valid_id(&file.id)?;
        valid_name(&file.name)?;
        ensure!(
            self.project_files(&file.folder_id)?.len() < MAX_FILES,
            "A project can contain up to 500 files"
        );
        self.conn.execute(
            "INSERT INTO project_files(id,folder_id,name,bytes,revision) VALUES(?1,?2,?3,?4,?5)",
            params![
                file.id,
                file.folder_id,
                file.name,
                file.bytes,
                file.revision
            ],
        )?;
        Ok(())
    }
    pub fn managed_file_path(&self, id: &str) -> Result<PathBuf> {
        let file = self.project_file(id)?;
        if let Some(relative) = &file.linked_path {
            self.read_project_file(&file)?;
            return Ok(self
                .project_link(&file.folder_id)?
                .context("Linked folder unavailable")?
                .path
                .join(relative));
        }
        valid_id(&file.id)?;
        valid_name(&file.name)?;
        let folder = project_directory(&self.project_root()?, &file.folder_id)?;
        let directory = folder.join(&file.id);
        let meta = std::fs::symlink_metadata(&directory).context("File is no longer available")?;
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "File directory has changed"
        );
        let path = directory.join(&file.name);
        let meta = std::fs::symlink_metadata(&path).context("File is no longer available")?;
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "File has changed"
        );
        Ok(path)
    }
    pub fn remove_managed_file(&self, id: &str) -> Result<()> {
        let file = self.project_file(id)?;
        self.ensure_active_project(&file.folder_id)?;
        ensure!(
            file.linked_path.is_none(),
            "Linked files stay in their folder. Unlink the folder instead."
        );
        self.conn
            .execute("DELETE FROM project_files WHERE id=?1", [id])?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FilePassage {
    pub page: Option<u32>,
    pub first_line: u32,
    pub last_line: u32,
    pub text: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSource {
    pub file_id: String,
    pub title: String,
    pub page: Option<u32>,
    pub text: String,
}

pub fn text_passages(text: &str, page: Option<u32>) -> Vec<FilePassage> {
    let mut passages = Vec::new();
    let mut content = String::new();
    let mut first = 1;
    let mut last = 1;
    let mut end_line = 1;
    for (index, line) in text.lines().enumerate() {
        last = index as u32 + 1;
        for character in line.chars().chain(std::iter::once('\n')) {
            if content.len() + character.len_utf8() > 2400 {
                if !content.trim().is_empty() {
                    passages.push(FilePassage {
                        page,
                        first_line: first,
                        last_line: end_line,
                        text: std::mem::take(&mut content),
                    });
                }
                content.clear();
                first = last;
            }
            content.push(character);
            end_line = last;
        }
    }
    if !content.trim().is_empty() {
        passages.push(FilePassage {
            page,
            first_line: first,
            last_line: last,
            text: content,
        });
    }
    passages
}

impl Store {
    pub fn index_project_file(&self, file: &ProjectFile, passages: &[FilePassage]) -> Result<()> {
        self.replace_project_file(None, file, passages)
    }
    pub fn replace_project_file(
        &self,
        previous: Option<&str>,
        file: &ProjectFile,
        passages: &[FilePassage],
    ) -> Result<()> {
        ensure!(passages.len() <= 2000, "File contains too much text");
        let tx = self.conn.unchecked_transaction()?;
        if let Some(id) = previous {
            let old = self.project_file(id)?;
            ensure!(
                old.folder_id == file.folder_id,
                "File belongs to another project"
            );
            self.remove_managed_file(id)?;
        }
        self.add_project_file(file)?;
        for passage in passages {
            tx.execute("INSERT INTO project_chunks(id,file_id,page,first_line,last_line,content) VALUES(?1,?2,?3,?4,?5,?6)",
                params![Uuid::new_v4().to_string(),file.id,passage.page,passage.first_line,passage.last_line,passage.text])?;
        }
        tx.commit()?;
        let _ = self.index_semantic_file(&file.id);
        Ok(())
    }
    pub fn search_project_files(&self, chat_id: &str, query: &str) -> Result<Vec<Source>> {
        let folder = self
            .chat(chat_id)?
            .folder_id
            .context("This chat has no project")?;
        self.ensure_active_project(&folder)?;
        let query = crate::storage::fts_query(query);
        if query.is_empty() {
            return Ok(vec![]);
        }
        let mut statement = self.conn.prepare("SELECT c.id,f.name,c.page,c.first_line,c.last_line,c.content FROM project_chunks_fts
            JOIN project_chunks c ON c.rowid=project_chunks_fts.rowid JOIN project_files f ON f.id=c.file_id
            WHERE project_chunks_fts MATCH ?1 AND f.folder_id=?2 ORDER BY bm25(project_chunks_fts) LIMIT 5")?;
        Ok(statement
            .query_map(params![query, folder], |r| {
                let id: String = r.get(0)?;
                let name: String = r.get(1)?;
                let page: Option<u32> = r.get(2)?;
                let first: u32 = r.get(3)?;
                let last: u32 = r.get(4)?;
                Ok(Source {
                    id: format!("file:{id}"),
                    kind: "file".into(),
                    title: passage_title(&name, page, first, last),
                    snippet: r.get(5)?,
                    ..Source::default()
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn file_source(&self, source_id: &str) -> Result<FileSource> {
        let id = source_id
            .strip_prefix("file:")
            .context("Invalid file citation")?;
        self.conn.query_row("SELECT f.id,f.name,c.page,c.first_line,c.last_line,c.content FROM project_chunks c JOIN project_files f ON f.id=c.file_id WHERE c.id=?1", [id], |r| {
            let name:String = r.get(1)?;
            let page = r.get(2)?;
            Ok(FileSource {file_id:r.get(0)?,title:passage_title(&name,page,r.get(3)?,r.get(4)?),page,text:r.get(5)?})
        }).optional()?.context("This file citation is no longer available")
    }
}
fn passage_title(name: &str, page: Option<u32>, first: u32, last: u32) -> String {
    match page {
        Some(page) => format!("{name} · page {page}"),
        None => format!("{name} · lines {first}–{last}"),
    }
}

impl Store {
    pub fn ensure_active_project(&self, folder_id: &str) -> Result<()> {
        let folder = self.folder(folder_id)?;
        ensure!(
            !folder.archived && !folder.deleting,
            "Restore this project before using it"
        );
        Ok(())
    }
    pub fn archived_projects(&self) -> Result<Vec<Folder>> {
        let mut s = self.conn.prepare("SELECT f.id,f.title,p.archived,p.deleting,coalesce(c.enabled,0) FROM folders f JOIN project_state p ON p.folder_id=f.id LEFT JOIN project_context c ON c.folder_id=f.id WHERE p.archived=1 OR p.deleting=1 ORDER BY f.title COLLATE NOCASE LIMIT 500")?;
        Ok(s.query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                title: r.get(1)?,
                archived: r.get(2)?,
                deleting: r.get(3)?,
                context_enabled: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
    }
    pub fn set_project_archived(&self, folder_id: &str, archived: bool) -> Result<()> {
        ensure!(
            !self.folder(folder_id)?.deleting,
            "Finish deleting this project first"
        );
        self.conn.execute("INSERT INTO project_state(folder_id,archived) VALUES(?1,?2) ON CONFLICT(folder_id) DO UPDATE SET archived=excluded.archived", params![folder_id,archived])?;
        Ok(())
    }
    pub fn begin_project_deletion(&self, folder_id: &str) -> Result<()> {
        self.folder(folder_id)?;
        self.conn.execute("INSERT INTO project_state(folder_id,archived,deleting) VALUES(?1,1,1) ON CONFLICT(folder_id) DO UPDATE SET archived=1,deleting=1", [folder_id])?;
        Ok(())
    }
    pub fn finish_project_deletion(&self, folder_id: &str) -> Result<()> {
        ensure!(
            self.folder(folder_id)?.deleting,
            "Project deletion has not started"
        );
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM chats WHERE folder_id=?1", [folder_id])?;
        tx.execute("DELETE FROM folders WHERE id=?1", [folder_id])?;
        tx.commit()?;
        Ok(())
    }
}

impl Store {
    pub fn set_project_context(&self, folder_id: &str, enabled: bool) -> Result<()> {
        self.ensure_active_project(folder_id)?;
        self.conn.execute("INSERT INTO project_context(folder_id,enabled) VALUES(?1,?2) ON CONFLICT(folder_id) DO UPDATE SET enabled=excluded.enabled", params![folder_id,enabled])?;
        Ok(())
    }
    pub fn project_references(&self, chat_id: &str, query: &str) -> Result<Vec<Source>> {
        let files = self.hybrid_file_references(chat_id, query)?;
        let chats = self.search_folder(chat_id, query, 5)?;
        Ok(files
            .iter()
            .take(3)
            .chain(chats.iter().take(2))
            .chain(files.iter().skip(3))
            .chain(chats.iter().skip(2))
            .take(5)
            .cloned()
            .collect())
    }
}

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
impl Store {
    pub fn project_link(&self, folder_id: &str) -> Result<Option<ProjectLink>> {
        self.folder(folder_id)?;
        Ok(self
            .conn
            .query_row(
                "SELECT path,device,inode,status FROM project_links WHERE folder_id=?1",
                [folder_id],
                |r| {
                    let path = PathBuf::from(r.get::<_, String>(0)?);
                    Ok(ProjectLink {
                        folder_id: folder_id.into(),
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path,
                        device: r.get(1)?,
                        inode: r.get(2)?,
                        status: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }
    pub fn set_project_link(&self, folder_id: &str, path: &Path) -> Result<ProjectLink> {
        self.ensure_active_project(folder_id)?;
        let (device, inode) = crate::linked::root_identity(path)?;
        let tx = self.conn.unchecked_transaction()?;
        self.clear_linked_files(folder_id)?;
        tx.execute("INSERT INTO project_links(folder_id,path,device,inode,status) VALUES(?1,?2,?3,?4,'Refresh to index files') ON CONFLICT(folder_id) DO UPDATE SET path=excluded.path,device=excluded.device,inode=excluded.inode,status=excluded.status",params![folder_id,path.to_str().context("Folder name must be UTF-8")?,device,inode])?;
        tx.commit()?;
        self.project_link(folder_id)?
            .context("Could not link folder")
    }
    fn clear_linked_files(&self, folder_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM project_files WHERE folder_id=?1 AND id IN (SELECT file_id FROM project_file_locations)", [folder_id])?;
        Ok(())
    }
    pub fn unlink_project(&self, folder_id: &str) -> Result<()> {
        self.ensure_active_project(folder_id)?;
        let tx = self.conn.unchecked_transaction()?;
        self.clear_linked_files(folder_id)?;
        tx.execute("DELETE FROM project_links WHERE folder_id=?1", [folder_id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn replace_linked_index(
        &self,
        link: &ProjectLink,
        files: &[(ProjectFile, Vec<FilePassage>)],
        status: &str,
    ) -> Result<()> {
        self.ensure_active_project(&link.folder_id)?;
        let current = self
            .project_link(&link.folder_id)?
            .context("Linked folder was removed")?;
        ensure!(
            current.path == link.path
                && current.device == link.device
                && current.inode == link.inode,
            "Linked folder changed during indexing"
        );
        let tx = self.conn.unchecked_transaction()?;
        let existing = self.project_files(&link.folder_id)?;
        let mut retained = std::collections::HashSet::new();
        for old in existing.iter().filter(|file| file.linked_path.is_some()) {
            if files.iter().any(|(file, _)| {
                file.linked_path == old.linked_path && file.revision == old.revision
            }) {
                retained.insert(old.linked_path.clone());
            } else {
                tx.execute("DELETE FROM project_files WHERE id=?1", [&old.id])?;
            }
        }
        for (file, passages) in files {
            if retained.contains(&file.linked_path) {
                continue;
            }
            self.add_project_file(file)?;
            tx.execute(
                "INSERT INTO project_file_locations(file_id,relative_path) VALUES(?1,?2)",
                params![
                    file.id,
                    file.linked_path.as_ref().context("Missing relative path")?
                ],
            )?;
            for passage in passages {
                tx.execute("INSERT INTO project_chunks(id,file_id,page,first_line,last_line,content) VALUES(?1,?2,?3,?4,?5,?6)",params![Uuid::new_v4().to_string(),file.id,passage.page,passage.first_line,passage.last_line,passage.text])?;
            }
        }
        tx.execute(
            "UPDATE project_links SET status=?1 WHERE folder_id=?2",
            params![status, link.folder_id],
        )?;
        tx.commit()?;
        for file in self.project_files(&link.folder_id)? {
            if file.linked_path.is_some() {
                let _ = self.index_semantic_file(&file.id);
            }
        }
        Ok(())
    }
    pub fn project_read(&self, file: &ProjectFile) -> Result<ProjectRead> {
        let (root, identity, relative) = if let Some(relative) = &file.linked_path {
            let link = self
                .project_link(&file.folder_id)?
                .context("Linked folder was removed")?;
            (
                link.path,
                Some((link.device, link.inode)),
                PathBuf::from(relative),
            )
        } else {
            (
                self.project_root()?,
                None,
                PathBuf::from(&file.folder_id)
                    .join(&file.id)
                    .join(&file.name),
            )
        };
        Ok(ProjectRead {
            root,
            identity,
            relative,
            revision: file.revision.clone(),
        })
    }
    pub fn read_project_file(&self, file: &ProjectFile) -> Result<Vec<u8>> {
        self.project_read(file)?.read()
    }
    pub(crate) fn file_source_reads(
        &self,
        sources: &[Source],
    ) -> std::collections::HashMap<ProjectRead, Vec<String>> {
        let mut reads = std::collections::HashMap::<_, Vec<_>>::new();
        for source in sources.iter().filter(|source| source.kind == "file") {
            let read = self
                .file_source(&source.id)
                .and_then(|citation| self.project_file(&citation.file_id))
                .and_then(|file| self.project_read(&file));
            if let Ok(read) = read {
                reads.entry(read).or_default().push(source.id.clone());
            }
        }
        reads
    }
    pub fn verify_file_sources(&self, sources: &[Source]) -> Result<()> {
        let mut verified = std::collections::HashSet::new();
        for source in sources.iter().filter(|source| source.kind == "file") {
            let citation = self.file_source(&source.id)?;
            if verified.insert(citation.file_id.clone()) {
                self.read_project_file(&self.project_file(&citation.file_id)?)?;
            }
        }
        Ok(())
    }
}

#[derive(Eq, Hash, PartialEq)]
pub struct ProjectRead {
    root: PathBuf,
    identity: Option<(u64, u64)>,
    relative: PathBuf,
    revision: String,
}
impl ProjectRead {
    pub fn read(&self) -> Result<Vec<u8>> {
        let bytes = crate::linked::read_scoped(&self.root, self.identity, &self.relative)?;
        ensure!(
            crate::linked::revision(&bytes) == self.revision,
            "File changed since indexing. Refresh linked files or import the updated file again."
        );
        Ok(bytes)
    }
}
impl Store {
    pub fn create_project(&self, name: &str) -> Result<Folder> {
        let root = self.project_root()?;
        let folder = self.create_folder(name)?;
        if let Err(error) = project_directory(&root, &folder.id) {
            self.conn
                .execute("DELETE FROM folders WHERE id=?1", [&folder.id])?;
            return Err(error);
        }
        Ok(folder)
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

    #[cfg(unix)]
    pub(crate) fn mentioned_edit_files(
        &self,
        link: &ProjectLink,
        query: &str,
    ) -> Result<Vec<(String, String)>> {
        use std::path::Path;
        let query = query.to_lowercase();
        let mut results = Vec::new();
        let mut bytes = 0;
        for file in self.project_files(&link.folder_id)? {
            let Some(path) = &file.linked_path else {
                continue;
            };
            let relative = Path::new(path);
            let filename = relative
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path);
            if file.bytes > (24 * 1024) as u64 || !query.contains(&filename.to_lowercase()) {
                continue;
            }
            let Ok(raw) =
                crate::linked::read_scoped(&link.path, Some((link.device, link.inode)), relative)
            else {
                continue;
            };
            let Ok(text) = String::from_utf8(raw) else {
                continue;
            };
            if text.contains('\0') || bytes + text.len() > (24 * 1024) {
                continue;
            }
            bytes += text.len();
            results.push((path.clone(), text));
            if results.len() == 3 {
                break;
            }
        }
        Ok(results)
    }

    #[cfg(not(unix))]
    pub(crate) fn mentioned_edit_files(
        &self,
        _: &ProjectLink,
        _: &str,
    ) -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }
}
