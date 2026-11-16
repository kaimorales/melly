use crate::{AppState, Reply, error};
use melly_core::projects::{self, MAX_FILE_BYTES, ProjectFile};
use std::{io::Read, path::Path};
use tauri::State;

#[tauri::command]
pub async fn project_files(s: State<'_, AppState>, folder_id: String) -> Reply<Vec<ProjectFile>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .project_files(&folder_id)
        .map_err(error)
}

#[tauri::command]
pub async fn import_project_file(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    folder_id: String,
    replace_file_id: Option<String>,
) -> Reply<Option<ProjectFile>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .folder(&folder_id)
        .map_err(error)?;
    let Some(path) = crate::attachments::pick_path(&app, false).await? else {
        return Ok(None);
    };
    let engine = s.engine.clone();
    let store = engine.store.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        let bytes = read_file(&path).map_err(error)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Choose a file with a plain filename")?
            .to_string();
        projects::valid_name(&name).map_err(error)?;
        let passages = crate::pdf::extract(&name, &bytes)?;
        let file = ProjectFile {
            id: uuid::Uuid::new_v4().to_string(),
            folder_id,
            name,
            bytes: bytes.len() as u64,
            revision: melly_core::linked::revision(&bytes),
            linked_path: None,
        };
        let root = {
            let store = store.lock().unwrap();
            store
                .ensure_active_project(&file.folder_id)
                .map_err(error)?;
            store.project_root().map_err(error)?
        };
        let directory = projects::project_directory(&root, &file.folder_id)
            .map_err(error)?
            .join(&file.id);
        std::fs::create_dir(&directory).map_err(|_| "Could not create the file directory")?;
        let result = (|| {
            std::fs::write(directory.join(&file.name), &bytes)
                .map_err(|_| "Could not copy the file. Check available disk space.")?;
            let retired = engine.with_idle_project(&file.folder_id, |store| {
                let retired = replace_file_id.as_deref().map(|id| {
                    let previous = store.project_file(id)?;
                    anyhow::ensure!(previous.folder_id == file.folder_id && previous.linked_path.is_none(), "Only managed files in this project can be replaced");
                    Ok::<_, anyhow::Error>(store.managed_file_path(id)?.parent().unwrap().to_path_buf())
                }).transpose()?;
                store.replace_project_file(replace_file_id.as_deref(), &file, &passages)?;
                Ok(retired)
            }).map_err(error)?;
            if let Some(path) = retired {
                // The new copy is already committed. Keep it if Trash is unavailable.
                if trash_directory(&path).is_err() {
                    return Err("Replacement saved. The previous copy could not be moved to Trash and remains in the project folder.".into());
                }
            }
            Ok(Some(file.clone()))
        })();
        if result.is_err() && store.lock().unwrap().project_file(&file.id).is_err() {
            let _ = std::fs::remove_dir_all(directory);
        }
        result
    })
    .await
    .map_err(|_| "File import interrupted")?
}

pub fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| anyhow::anyhow!("Could not open the selected file"))?;
    let meta = file.metadata()?;
    anyhow::ensure!(
        meta.is_file() && meta.len() <= MAX_FILE_BYTES,
        "Choose a regular file up to 20 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= MAX_FILE_BYTES, "File exceeds 20 MiB");
    Ok(bytes)
}

#[tauri::command]
pub async fn show_project_directory(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    let store = s.engine.store.lock().unwrap();
    store.folder(&folder_id).map_err(error)?;
    let directory = projects::project_directory(&store.project_root().map_err(error)?, &folder_id)
        .map_err(error)?;
    open::that(directory).map_err(|_| "Could not open the project in Finder".into())
}

#[tauri::command]
pub async fn open_project_file(s: State<'_, AppState>, file_id: String) -> Reply<()> {
    let store = s.engine.store.clone();
    let path =
        tokio::task::spawn_blocking(move || store.lock().unwrap().managed_file_path(&file_id))
            .await
            .map_err(|_| "Could not locate file")?
            .map_err(error)?;
    open::that(path).map_err(|_| "Could not open the file".into())
}

#[tauri::command]
pub async fn file_source(
    s: State<'_, AppState>,
    source_id: String,
) -> Reply<melly_core::projects::FileSource> {
    let (source, read) = {
        let store = s.engine.store.lock().unwrap();
        let source = store.file_source(&source_id).map_err(error)?;
        let read = store
            .project_read(&store.project_file(&source.file_id).map_err(error)?)
            .map_err(error)?;
        (source, read)
    };
    tokio::task::spawn_blocking(move || read.read())
        .await
        .map_err(|_| "Could not read citation")?
        .map_err(error)?;
    Ok(source)
}
#[tauri::command]
pub async fn open_pdf_source(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    source_id: String,
) -> Reply<()> {
    let store = s.engine.store.clone();
    let (bytes, page) = tokio::task::spawn_blocking(move || {
        let store = store.lock().unwrap();
        let source = store.file_source(&source_id).map_err(error)?;
        let read = store
            .project_read(&store.project_file(&source.file_id).map_err(error)?)
            .map_err(error)?;
        drop(store);
        let bytes = read.read().map_err(error)?;
        Ok::<_, String>((bytes, source.page.ok_or("This citation is a text file")?))
    })
    .await
    .map_err(|_| "Could not read PDF")??;
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = send.send(crate::pdf::show(&bytes, page));
    })
    .map_err(|_| "Could not open PDF")?;
    receive.await.map_err(|_| "Could not open PDF")?
}

#[tauri::command]
pub async fn archived_projects(s: State<'_, AppState>) -> Reply<Vec<melly_core::models::Folder>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .archived_projects()
        .map_err(error)
}
#[tauri::command]
pub async fn archive_project(
    s: State<'_, AppState>,
    folder_id: String,
    archived: bool,
) -> Reply<()> {
    s.engine
        .archive_project(&folder_id, archived)
        .map_err(error)
}
#[tauri::command]
pub async fn delete_project(s: State<'_, AppState>, folder_id: String) -> Reply<()> {
    let engine = s.engine.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        engine
            .delete_project(&folder_id, |path| {
                trash_directory(path).map_err(anyhow::Error::msg)
            })
            .map_err(error)
    })
    .await
    .map_err(|_| "Project deletion interrupted. Retry from Archived projects.")?
}

#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
pub(crate) fn trash_directory(path: &Path) -> Reply<()> {
    use cocoa::{
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let string =
            NSString::alloc(nil).init_str(path.to_str().ok_or("Invalid project directory")?);
        let url: id = msg_send![class!(NSURL), fileURLWithPath:string];
        let manager: id = msg_send![class!(NSFileManager), defaultManager];
        let mut resulting: id = nil;
        let mut failure: id = nil;
        let success: bool = msg_send![manager, trashItemAtURL:url resultingItemURL:&mut resulting error:&mut failure];
        let _: () = msg_send![string, release];
        pool.drain();
        if success {
            Ok(())
        } else {
            Err(
                "Could not move project files to Trash. Retry deletion from Archived projects."
                    .into(),
            )
        }
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn trash_directory(_: &Path) -> Reply<()> {
    Err("Moving project files to Trash requires macOS".into())
}

#[tauri::command]
pub async fn remove_project_file(s: State<'_, AppState>, file_id: String) -> Reply<()> {
    let engine = s.engine.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        let file = engine
            .store
            .lock()
            .unwrap()
            .project_file(&file_id)
            .map_err(error)?;
        engine
            .with_idle_project(&file.folder_id, |store| {
                anyhow::ensure!(
                    file.linked_path.is_none(),
                    "Linked files stay in their original folder"
                );
                projects::valid_id(&file.id)?;
                let directory =
                    projects::project_directory(&store.project_root()?, &file.folder_id)?
                        .join(&file.id);
                if let Some(meta) = match std::fs::symlink_metadata(&directory) {
                    Ok(meta) => Some(meta),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                } {
                    anyhow::ensure!(
                        meta.is_dir() && !meta.file_type().is_symlink(),
                        "File directory changed"
                    );
                    trash_directory(&directory).map_err(anyhow::Error::msg)?;
                }
                store.remove_managed_file(&file_id)
            })
            .map_err(error)
    })
    .await
    .map_err(|_| "File removal interrupted. Try again.")?
}

#[tauri::command]
pub async fn set_project_context(
    s: State<'_, AppState>,
    folder_id: String,
    enabled: bool,
) -> Reply<()> {
    s.engine
        .store
        .lock()
        .unwrap()
        .set_project_context(&folder_id, enabled)
        .map_err(error)
}
