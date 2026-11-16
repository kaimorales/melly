use crate::{AppState, Reply, error};
use melly_core::{
    linked,
    projects::{FilePassage, ProjectFile, ProjectLink},
};
use std::path::{Path, PathBuf};
use tauri::State;
use tokio_util::sync::CancellationToken;

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
) -> Reply<()> {
    s.engine
        .store
        .lock()
        .unwrap()
        .ensure_active_project(&folder_id)
        .map_err(error)?;
    let Some(path) = crate::attachments::pick_path(&app, true).await? else {
        return Ok(());
    };
    s.file_edits.lock().unwrap().revoke_workspace(&folder_id);
    scan(s, folder_id, Some(path)).await
}
#[tauri::command]
pub async fn refresh_linked_project(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    scan(s, folder_id, None).await
}
#[tauri::command]
pub async fn unlink_project(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    s.file_edits.lock().unwrap().revoke_workspace(&folder_id);
    let store = s.engine.store.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        store
            .lock()
            .unwrap()
            .unlink_project(&folder_id)
            .map_err(error)
    })
    .await
    .map_err(|_| "Could not unlink folder")?
}
#[tauri::command]
pub async fn cancel_project_scan(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    if let Some(cancel) = s.project_scans.lock().unwrap().get(&folder_id) {
        cancel.cancel();
    }
    Ok(())
}
async fn scan(s: State<'_, AppState>, folder_id: String, selected: Option<PathBuf>) -> Reply<()> {
    let cancel = CancellationToken::new();
    {
        let mut scans = s.project_scans.lock().unwrap();
        if scans.contains_key(&folder_id) {
            return Err("This project is already indexing".into());
        }
        scans.insert(folder_id.clone(), cancel.clone());
    }
    let store = s.engine.store.clone();
    let operations = s.project_operations.clone();
    let id = folder_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        if cancel.is_cancelled() {
            return Err("Indexing cancelled".into());
        }
        let link = {
            let store = store.lock().unwrap();
            store.ensure_active_project(&id).map_err(error)?;
            if let Some(path) = selected {
                let meta = std::fs::symlink_metadata(&path).map_err(|_| "Folder unavailable")?;
                if !meta.is_dir() || meta.file_type().is_symlink() {
                    return Err("Choose a real folder".into());
                }
                let path = path.canonicalize().map_err(|_| "Folder unavailable")?;
                store.set_project_link(&id, &path).map_err(error)?
            } else {
                store
                    .project_link(&id)
                    .map_err(error)?
                    .ok_or("Link a folder first")?
            }
        };
        let (files, skipped) = scan_files(&link, &cancel)?;
        if cancel.is_cancelled() {
            return Err("Indexing cancelled".into());
        }
        let status = format!(
            "{} files indexed · {} skipped · refresh after edits",
            files.len(),
            skipped
        );
        store
            .lock()
            .unwrap()
            .replace_linked_index(&link, &files, &status)
            .map_err(error)
    })
    .await
    .map_err(|_| "Folder indexing interrupted");
    s.project_scans.lock().unwrap().remove(&folder_id);
    result?
}

fn supported(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "markdown"
            | "pdf"
            | "rs"
            | "py"
            | "go"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "csv"
            | "html"
            | "css"
            | "swift"
            | "c"
            | "cpp"
            | "h"
            | "java"
            | "sql"
            | "xml"
            | "sh"
    )
}
pub(crate) fn excluded(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with('.')
        || name.contains("secret")
        || name.contains("credential")
        || matches!(
            name.as_str(),
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | "venv"
                | "__pycache__"
                | "coverage"
                | "package-lock.json"
                | "cargo.lock"
        )
}
type IndexedFiles = Vec<(ProjectFile, Vec<FilePassage>)>;
fn scan_files(link: &ProjectLink, cancel: &CancellationToken) -> Reply<(IndexedFiles, usize)> {
    if linked::root_identity(&link.path).map_err(error)? != (link.device, link.inode) {
        return Err("Linked folder moved or was replaced. Reconnect it.".into());
    }
    let mut queue = vec![PathBuf::new()];
    let mut files = Vec::new();
    let mut skipped = 0;
    let mut visited = 0;
    let mut total_bytes = 0;
    while let Some(relative) = queue.pop() {
        if cancel.is_cancelled() {
            return Err("Indexing cancelled".into());
        }
        let entries = std::fs::read_dir(link.path.join(&relative))
            .map_err(|_| "Linked folder is inaccessible. Reconnect or refresh it.")?;
        for entry in entries {
            if cancel.is_cancelled() {
                return Err("Indexing cancelled".into());
            }
            visited += 1;
            if visited > 20_000 {
                return Err("Choose a smaller folder with fewer than 20,000 entries".into());
            }
            let entry = entry.map_err(|_| "Could not inspect a linked file")?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                skipped += 1;
                continue;
            };
            if excluded(name) {
                skipped += 1;
                continue;
            }
            let kind = entry
                .file_type()
                .map_err(|_| "Could not inspect a linked file")?;
            let child = relative.join(name);
            if kind.is_dir() {
                if child.components().count() < 16 {
                    queue.push(child);
                } else {
                    skipped += 1;
                }
                continue;
            }
            if !kind.is_file() || !supported(&child) {
                skipped += 1;
                continue;
            }
            let bytes =
                match linked::read_scoped(&link.path, Some((link.device, link.inode)), &child) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
            total_bytes += bytes.len();
            if total_bytes > 50 * 1024 * 1024 || files.len() >= 400 {
                return Err(
                    "Choose a smaller folder with up to 400 readable files and 50 MiB total".into(),
                );
            }
            let passages = match crate::pdf::extract(name, &bytes) {
                Ok(parts) => parts,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            files.push((
                ProjectFile {
                    id: uuid::Uuid::new_v4().to_string(),
                    folder_id: link.folder_id.clone(),
                    name: name.into(),
                    bytes: bytes.len() as u64,
                    revision: linked::revision(&bytes),
                    linked_path: Some(child.to_string_lossy().into_owned()),
                },
                passages,
            ));
        }
    }
    Ok((files, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanning_excludes_hidden_secrets_dependencies_and_obeys_cancel() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("notes.md"), "Synthetic transit result").unwrap();
        std::fs::write(root.path().join(".env"), "synthetic excluded").unwrap();
        std::fs::write(root.path().join("credentials.json"), "synthetic excluded").unwrap();
        std::fs::create_dir(root.path().join("node_modules")).unwrap();
        std::fs::write(
            root.path().join("node_modules/excluded.js"),
            "synthetic excluded",
        )
        .unwrap();
        let (device, inode) = linked::root_identity(root.path()).unwrap();
        let link = ProjectLink {
            folder_id: uuid::Uuid::new_v4().to_string(),
            name: "Synthetic".into(),
            status: "".into(),
            path: root.path().into(),
            device,
            inode,
        };
        let cancel = CancellationToken::new();
        let (files, skipped) = scan_files(&link, &cancel).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0.name, "notes.md");
        assert_eq!(skipped, 3);
        cancel.cancel();
        assert!(scan_files(&link, &cancel).is_err());
    }
}
