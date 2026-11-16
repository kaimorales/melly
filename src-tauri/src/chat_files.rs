//! Native paths never cross IPC. A drop ticket authorizes one bounded import.
use crate::{AppState, Reply, error};
use melly_core::{models::AttachmentInfo, storage::Store};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State};
#[derive(Default)]
pub struct PendingDrop(pub Option<(String, Vec<PathBuf>)>);
impl PendingDrop {
    fn issue(&mut self, paths: &[PathBuf]) -> String {
        let ticket = uuid::Uuid::new_v4().to_string();
        self.0 = Some((ticket.clone(), paths.iter().take(9).cloned().collect()));
        ticket
    }
    fn take(&mut self, ticket: &str) -> Reply<Vec<PathBuf>> {
        if !self.0.as_ref().is_some_and(|(id, _)| id == ticket) {
            return Err("Drop the files again".into());
        }
        Ok(self.0.take().unwrap().1)
    }
}

pub fn handle_drop(window: &tauri::Window, event: &tauri::DragDropEvent) {
    match event {
        tauri::DragDropEvent::Enter { .. } => {
            let _ = window.emit("file-drag", true);
        }
        tauri::DragDropEvent::Leave => {
            let _ = window.emit("file-drag", false);
        }
        tauri::DragDropEvent::Drop { paths, .. } => {
            let _ = window.emit("file-drag", false);
            let id = window
                .state::<AppState>()
                .file_drop
                .lock()
                .unwrap()
                .issue(paths);
            let _ = window.emit("file-drop", id);
        }
        _ => {}
    }
}
async fn import(store: Arc<Mutex<Store>>, paths: Vec<PathBuf>) -> Reply<Vec<AttachmentInfo>> {
    tokio::task::spawn_blocking(move || {
        if paths.is_empty() || paths.len() > 8 {
            return Err("Drop up to eight files at a time".into());
        }
        let mut imported = Vec::new();
        let result = (|| {
            for path in paths {
                let bytes = crate::project_files::read_file(&path).map_err(error)?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("Choose a file with a plain filename")?;
                let passages = crate::pdf::extract(name, &bytes)?;
                let content = passages
                    .iter()
                    .map(|p| match p.page {
                        Some(page) => format!("[Page {page}]\n{}", p.text),
                        None => p.text.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let lower = name.to_ascii_lowercase();
                let kind = if lower.ends_with(".pdf") {
                    "pdf"
                } else if crate::ocr::is_image(&lower) {
                    "image-text"
                } else {
                    "text"
                };
                let file = store
                    .lock()
                    .unwrap()
                    .save_draft_attachment(name, kind, &content, &bytes)
                    .map_err(error)?;
                imported.push(file);
            }
            Ok(())
        })();
        if let Err(reason) = result {
            for file in &imported {
                let _ = store.lock().unwrap().discard_draft_attachment(&file.id);
            }
            return Err(reason);
        }
        Ok(imported)
    })
    .await
    .map_err(|_| "File import interrupted")?
}
#[tauri::command]
pub async fn pick_chat_files(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
) -> Reply<Vec<AttachmentInfo>> {
    let Some(path) = crate::attachments::pick_path(&app, false).await? else {
        return Ok(vec![]);
    };
    import(s.engine.store.clone(), vec![path]).await
}
#[tauri::command]
pub async fn accept_file_drop(
    s: State<'_, AppState>,
    ticket: String,
    accept: bool,
) -> Reply<Vec<AttachmentInfo>> {
    let paths = s.file_drop.lock().unwrap().take(&ticket)?;
    if !accept {
        return Ok(vec![]);
    }
    import(s.engine.store.clone(), paths).await
}
#[tauri::command]
pub async fn discard_chat_file(s: State<'_, AppState>, id: String) -> Reply<()> {
    s.engine
        .store
        .lock()
        .unwrap()
        .discard_draft_attachment(&id)
        .map_err(error)
}

#[tauri::command]
pub async fn copy_message_files(
    s: State<'_, AppState>,
    message_id: String,
) -> Reply<Vec<AttachmentInfo>> {
    let store = s.engine.store.clone();
    tokio::task::spawn_blocking(move || {
        store
            .lock()
            .unwrap()
            .copy_message_files(&message_id)
            .map_err(error)
    })
    .await
    .map_err(|_| "Could not prepare attachments for editing")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_drop_tickets_are_single_use_and_keep_the_over_limit_sentinel() {
        let mut pending = PendingDrop::default();
        let first = pending.issue(&[PathBuf::from("synthetic.txt")]);
        assert!(pending.take("unrecognized").is_err());
        assert_eq!(pending.take(&first).unwrap().len(), 1);
        assert!(pending.take(&first).is_err());
        let superseded = pending.issue(&[]);
        let latest = pending.issue(&vec![PathBuf::from("synthetic.txt"); 100]);
        assert!(pending.take(&superseded).is_err());
        assert_eq!(pending.take(&latest).unwrap().len(), 9);
    }
    #[tokio::test]
    async fn file_batches_reject_invalid_inputs_without_leaving_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let good = directory.path().join("synthetic.txt");
        let bad = directory.path().join("binary.txt");
        std::fs::write(&good, b"Synthetic code 4343").unwrap();
        std::fs::write(&bad, b"\0invalid").unwrap();
        let store = Arc::new(Mutex::new(Store::memory().unwrap()));
        assert!(
            import(store.clone(), vec![good.clone(), bad])
                .await
                .is_err()
        );
        let files = import(store.clone(), vec![good; 8]).await.unwrap();
        assert_eq!(
            files.len(),
            8,
            "Failed batches must release every draft slot"
        );
        assert!(files.iter().all(|file| file.kind == "text"));
        assert!(
            store
                .lock()
                .unwrap()
                .attachment_content(&files[0].id)
                .unwrap()
                .1
                == b"Synthetic code 4343"
        );
        assert!(
            import(store, vec![directory.path().to_owned()])
                .await
                .is_err()
        );
    }
}
