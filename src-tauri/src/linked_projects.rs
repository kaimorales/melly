use crate::{AppState, Reply, attachments, error};
use melly_core::projects::ProjectLink;
use tauri::State;

#[tauri::command]
pub async fn project_link(s: State<'_, AppState>, folder_id: String) -> Reply<Option<ProjectLink>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .project_link(&folder_id)
        .map_err(error)
}

#[tauri::command]
pub async fn link_project_directory(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    folder_id: String,
) -> Reply<bool> {
    s.engine
        .store
        .lock()
        .unwrap()
        .ensure_active_project(&folder_id)
        .map_err(error)?;
    let Some(path) = attachments::pick_path(&app, true).await? else {
        return Ok(false);
    };
    let operations = s.project_operations.clone();
    let store = s.engine.store.clone();
    s.file_edits.lock().unwrap().revoke_workspace(&folder_id);
    tauri::async_runtime::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        store
            .lock()
            .unwrap()
            .set_project_link(&folder_id, &path)
            .map_err(error)
    })
    .await
    .map_err(|_| "Could not link the folder")??;
    Ok(true)
}

#[tauri::command]
pub async fn unlink_project(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    let _operation = s.project_operations.lock().unwrap();
    s.file_edits.lock().unwrap().revoke_workspace(&folder_id);
    s.engine
        .store
        .lock()
        .unwrap()
        .unlink_project(&folder_id)
        .map_err(error)
}
