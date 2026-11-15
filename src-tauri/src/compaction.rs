use crate::{AppState, Reply, error};
use melly_core::compaction::Summary;
use tauri::State;
#[tauri::command]
pub async fn context_summary(s: State<'_, AppState>, chat_id: String) -> Reply<Option<Summary>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .context_summary(&chat_id)
        .map_err(error)
}
#[tauri::command]
pub async fn compact_context(
    s: State<'_, AppState>,
    chat_id: String,
    provider_id: String,
    model_id: String,
) -> Reply<Summary> {
    s.engine
        .compact_context(&chat_id, &provider_id, &model_id)
        .await
        .map_err(error)
}
#[tauri::command]
pub async fn clear_context_summary(s: State<'_, AppState>, chat_id: String) -> Reply<()> {
    s.engine
        .with_idle_store(|store| store.clear_context_summary(&chat_id))
        .map_err(error)
}
