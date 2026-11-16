use crate::{AppState, Reply, error};
use melly_core::{
    drafts::Draft,
    models::{AttachmentInfo, Message},
};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct Composer {
    content: String,
    attachments: Vec<AttachmentInfo>,
    editing: Option<Message>,
}
#[tauri::command]
pub async fn composer_draft(s: State<'_, AppState>, chat_id: Option<String>) -> Reply<Composer> {
    let store = s.engine.store.lock().unwrap();
    let draft = store.composer_draft(chat_id.as_deref()).map_err(error)?;
    // A missing attachment must not hide otherwise recoverable draft text.
    let attachments = draft
        .attachment_ids
        .iter()
        .flat_map(|id| {
            store
                .validate_attachment_ids(std::slice::from_ref(id))
                .unwrap_or_default()
        })
        .collect();
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
    store
        .validate_attachment_ids(&draft.attachment_ids)
        .map_err(error)?;
    store
        .save_composer_draft(chat_id.as_deref(), &draft)
        .map_err(error)
}
