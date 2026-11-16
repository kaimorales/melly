use crate::{AppState, Reply, error};
use melly_core::portable::{Backup, BackupSummary};
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::Path,
};
use tauri::State;
#[derive(Serialize)]
pub struct PreparedRestore {
    ticket: String,
    #[serde(flatten)]
    summary: BackupSummary,
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Reply<()> {
    let parent = path.parent().ok_or("Choose a destination folder")?;
    let temporary = parent.join(format!(".melly-export-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result.map_err(|_| "Could not save the export. Check the destination and free space.".into())
}
#[tauri::command]
pub async fn export_data(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    chat_id: Option<String>,
    folder_id: Option<String>,
) -> Reply<bool> {
    if chat_id.is_some() && folder_id.is_some() {
        return Err("Choose one export scope".into());
    }
    let name = if chat_id.is_some() {
        "Melly conversation.md"
    } else if folder_id.is_some() {
        "Melly project.melly.json"
    } else {
        "Melly backup.melly.json"
    };
    let Some(path) = crate::attachments::save_path(&app, name).await? else {
        return Ok(false);
    };
    let engine = s.engine.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        let bytes = engine
            .with_idle_store(|store| {
                if let Some(chat) = chat_id {
                    Ok(store.export_chat_markdown(&chat)?.into_bytes())
                } else {
                    store
                        .backup(folder_id.as_deref(), crate::project_files::read_file)?
                        .bytes()
                }
            })
            .map_err(error)?;
        write_atomic(&path, &bytes)?;
        Ok(true)
    })
    .await
    .map_err(|_| "Export interrupted")?
}
#[tauri::command]
pub async fn prepare_restore(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
) -> Reply<Option<PreparedRestore>> {
    s.pending_restore.lock().unwrap().take();
    let Some(path) = crate::attachments::pick_path(&app, false).await? else {
        return Ok(None);
    };
    let backup = tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "Could not read the backup")?;
        let meta = file.metadata().map_err(|_| "Could not read the backup")?;
        if !meta.is_file() || meta.len() > 96 * 1024 * 1024 {
            return Err("Choose a Melly backup smaller than 96 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(96 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read the backup")?;
        Backup::from_bytes(&bytes).map_err(error)
    })
    .await
    .map_err(|_| "Backup validation interrupted")??;
    let summary = backup.summary();
    let ticket = uuid::Uuid::new_v4().to_string();
    *s.pending_restore.lock().unwrap() = Some((ticket.clone(), backup));
    Ok(Some(PreparedRestore { ticket, summary }))
}
#[tauri::command]
pub async fn finish_restore(
    s: State<'_, AppState>,
    ticket: String,
    confirm: bool,
) -> Reply<Option<BackupSummary>> {
    let backup = {
        let mut pending = s.pending_restore.lock().unwrap();
        if !pending.as_ref().is_some_and(|(id, _)| id == &ticket) {
            return Err("Choose the backup again".into());
        }
        pending.take().unwrap().1
    };
    if !confirm {
        return Ok(None);
    }
    let engine = s.engine.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        engine
            .with_idle_store(|store| store.restore_backup(backup))
            .map(Some)
            .map_err(error)
    })
    .await
    .map_err(|_| "Restore interrupted")?
}
