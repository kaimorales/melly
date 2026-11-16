//! Optional, on-device sentence embeddings. Missing assets retain keyword retrieval.
use crate::{models::Source, storage::Store};
use anyhow::Result;
use rusqlite::params;
fn similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.;
    }
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    let magnitude =
        (a.iter().map(|v| v * v).sum::<f32>() * b.iter().map(|v| v * v).sum::<f32>()).sqrt();
    if magnitude > 0. { dot / magnitude } else { 0. }
}
#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
fn vector(text: &str) -> Option<(usize, Vec<f32>)> {
    use cocoa::{
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    #[link(name = "NaturalLanguage", kind = "framework")]
    unsafe extern "C" {}
    // Framework instances are not thread safe. Give each worker its own retained instance.
    struct Embedding(id);
    impl Drop for Embedding {
        fn drop(&mut self) {
            unsafe {
                if self.0 != nil {
                    let _: () = msg_send![self.0, release];
                }
            }
        }
    }
    thread_local! {static MODEL:Embedding=unsafe{
        let pool=NSAutoreleasePool::new(nil);let language=NSString::alloc(nil).init_str("en");
        let model:id=msg_send![class!(NLEmbedding),sentenceEmbeddingForLanguage:language];
        if model!=nil{let _:id=msg_send![model,retain];}let _:()=msg_send![language,release];pool.drain();Embedding(model)
    };}
    MODEL.with(|model| unsafe {
        if model.0 == nil {
            return None;
        }
        let pool = NSAutoreleasePool::new(nil);
        let clipped: String = text.chars().take(600).collect();
        let string = NSString::alloc(nil).init_str(&clipped);
        let values: id = msg_send![model.0,vectorForString:string];
        let _: () = msg_send![string, release];
        let revision: usize = msg_send![model.0, revision];
        let count: usize = if values == nil {
            0
        } else {
            msg_send![values, count]
        };
        let mut result = Vec::new();
        if (1..=1024).contains(&count) {
            for i in 0..count {
                let number: id = msg_send![values,objectAtIndex:i];
                let value: f32 = msg_send![number, floatValue];
                result.push(value);
            }
        }
        pool.drain();
        (!result.is_empty() && result.iter().all(|v| v.is_finite())).then_some((revision, result))
    })
}
#[cfg(not(target_os = "macos"))]
fn vector(_: &str) -> Option<(usize, Vec<f32>)> {
    None
}
impl Store {
    pub(crate) fn initialize_semantic(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS passage_vectors(id TEXT PRIMARY KEY REFERENCES project_chunks(id) ON DELETE CASCADE,revision INTEGER NOT NULL,vector BLOB NOT NULL);")?;
        Ok(())
    }
    pub fn index_semantic_file(&self, file_id: &str) -> Result<()> {
        let Some((revision, _)) = vector("Reference text") else {
            return Ok(());
        };
        let chunks = self
            .conn
            .prepare(
                "SELECT id,content FROM project_chunks WHERE id IN(SELECT id FROM project_chunks WHERE file_id=?1 ORDER BY rowid LIMIT 128) AND NOT EXISTS(SELECT 1 FROM passage_vectors v WHERE v.id=project_chunks.id AND v.revision=?2)",
            )?
            .query_map(params![file_id,revision], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in chunks {
            if let Some((model, values)) = vector(&text) {
                if model != revision {
                    break;
                }
                let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
                self.conn.execute("INSERT INTO passage_vectors VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,vector=excluded.vector",params![id,revision,bytes])?;
            }
        }
        Ok(())
    }
    pub fn hybrid_file_references(&self, chat_id: &str, query: &str) -> Result<Vec<Source>> {
        let keyword = self.search_project_files(chat_id, query)?;
        let Some((revision, query_vector)) = vector(query) else {
            return Ok(keyword);
        };
        self.merge_semantic(chat_id, keyword, revision, &query_vector)
    }
    fn merge_semantic(
        &self,
        chat_id: &str,
        keyword: Vec<Source>,
        revision: usize,
        query: &[f32],
    ) -> Result<Vec<Source>> {
        let folder = self.chat(chat_id)?.folder_id;
        let Some(folder) = folder else {
            return Ok(keyword);
        };
        self.ensure_active_project(&folder)?;
        let mut statement=self.conn.prepare("SELECT v.id,v.vector,c.content FROM passage_vectors v JOIN project_chunks c ON c.id=v.id JOIN project_files f ON f.id=c.file_id WHERE f.folder_id=?1 AND v.revision=?2 ORDER BY c.rowid DESC LIMIT 2048")?;
        let mut scored = Vec::new();
        for row in statement.query_map(params![folder, revision], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (id, bytes, text) = row?;
            if bytes.len() > 4096 || bytes.len() % 4 != 0 {
                continue;
            }
            let values: Vec<f32> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect();
            let score = similarity(query, &values);
            if score >= 0.5 && score.is_finite() {
                scored.push((score, id, text));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut result: Vec<Source> = keyword.iter().take(3).cloned().collect();
        for (_, id, text) in scored.into_iter().take(5) {
            let id = format!("file:{id}");
            if result.iter().any(|s| s.id == id) {
                continue;
            }
            let file = self.file_source(&id)?;
            result.push(Source {
                id,
                kind: "file".into(),
                title: file.title,
                snippet: text,
                ..Source::default()
            });
            if result.len() == 5 {
                break;
            }
        }
        for source in keyword {
            if result.len() == 5 {
                break;
            }
            if !result.iter().any(|s| s.id == source.id) {
                result.push(source);
            }
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_candidates_are_ranked_and_scoped_to_the_current_project() {
        use crate::projects::{ProjectFile, text_passages};
        let store = Store::memory().unwrap();
        let folder = store.create_folder("Current").unwrap();
        let foreign = store.create_folder("Private").unwrap();
        let chat = store.create_chat(Some(&folder.id)).unwrap();
        for (folder, text, values) in [
            (&folder, "Refunds arrive within seven days", [1f32, 0.]),
            (&folder, "Orders arrive by courier", [0f32, 1.]),
            (&foreign, "An unrelated private purchase", [1f32, 0.]),
        ] {
            let file = ProjectFile {
                id: uuid::Uuid::new_v4().to_string(),
                folder_id: folder.id.clone(),
                name: "reference.txt".into(),
                bytes: text.len() as u64,
                revision: "test".into(),
                linked_path: None,
            };
            store
                .index_project_file(&file, &text_passages(text, None))
                .unwrap();
            let id: String = store
                .conn
                .query_row(
                    "SELECT id FROM project_chunks WHERE file_id=?1",
                    [file.id],
                    |r| r.get(0),
                )
                .unwrap();
            let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            store
                .conn
                .execute(
                    "INSERT OR REPLACE INTO passage_vectors VALUES(?1,0,?2)",
                    params![id, bytes],
                )
                .unwrap();
        }
        let result = store
            .merge_semantic(&chat.id, vec![], 0, &[1., 0.])
            .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].snippet.contains("Refunds"));
        assert_eq!(similarity(&[0., 0.], &[1., 0.]), 0.);
    }
}
