use crate::{AppState, Reply, error};
use melly_core::usage::{ContextPreview, Rates, UsageSummary};
use tauri::State;
#[tauri::command]
pub async fn provider_usage(
    s: State<'_, AppState>,
    provider_id: String,
    model_id: String,
) -> Reply<UsageSummary> {
    let mut c = s.engine.connection(&provider_id).map_err(error)?;
    c.model_id = model_id;
    s.engine
        .store
        .lock()
        .unwrap()
        .usage_summary(&c)
        .map_err(error)
}
#[tauri::command]
pub async fn save_usage_rates(
    s: State<'_, AppState>,
    provider_id: String,
    model_id: String,
    rates: Rates,
) -> Reply<()> {
    let mut c = s.engine.connection(&provider_id).map_err(error)?;
    c.model_id = model_id;
    s.engine
        .store
        .lock()
        .unwrap()
        .save_rates(&c, rates)
        .map_err(error)
}
// Keep the existing named IPC arguments for compatibility with installed views.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn context_preview(
    s: State<'_, AppState>,
    chat_id: Option<String>,
    provider_id: String,
    model_id: String,
    draft: String,
    attachment_ids: Vec<String>,
    folder_context: bool,
    draft_context: Option<melly_core::context::ContextLimit>,
) -> Reply<ContextPreview> {
    let engine = s.engine.clone();
    tokio::task::spawn_blocking(move || {
        engine
            .context_preview(melly_core::engine::ContextPreviewRequest {
                chat_id: chat_id.as_deref(),
                provider_id: &provider_id,
                model_id: &model_id,
                draft: &draft,
                attachment_ids: &attachment_ids,
                folder_context,
                draft_context,
            })
            .map_err(error)
    })
    .await
    .map_err(|_| "Context preview interrupted")?
}

#[tauri::command]
pub async fn account_usage(
    s: State<'_, AppState>,
    provider_id: String,
) -> Reply<melly_core::providers::AccountUsage> {
    if provider_id != "openrouter" {
        return Err("Account usage is unavailable for this connection".into());
    }
    let connection = s.engine.connection(&provider_id).map_err(error)?;
    let key = s
        .engine
        .credential(&connection)
        .await
        .map_err(error)?
        .ok_or("Connect OpenRouter first")?;
    melly_core::providers::account_usage(&connection, &key)
        .await
        .map_err(error)
}
