use crate::{AppState, Reply, error};
use melly_core::{drafts::Draft, models::Message};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct Composer {
    content: String,
    attachments: Vec<serde_json::Value>,
    editing: Option<Message>,
}
#[tauri::command]
pub async fn composer_draft(s: State<'_, AppState>, chat_id: Option<String>) -> Reply<Composer> {
    let store = s.engine.store.lock().unwrap();
    let draft = store.composer_draft(chat_id.as_deref()).map_err(error)?;
    let attachments = vec![];
    let editing = draft
        .editing_message_id
        .as_deref()
        .map(|id| store.message(id))
        .transpose()
        .map_err(error)?;
    Ok(Composer {
        content: draft.content,
        attachments,
        editing,
    })
}
#[tauri::command]
pub async fn save_composer_draft(
    s: State<'_, AppState>,
    chat_id: Option<String>,
    draft: Draft,
) -> Reply<()> {
    let store = s.engine.store.lock().unwrap();
    if !draft.attachment_ids.is_empty() {
        return Err("This build does not support saved attachments".into());
    }
    store
        .save_composer_draft(chat_id.as_deref(), &draft)
        .map_err(error)
}
