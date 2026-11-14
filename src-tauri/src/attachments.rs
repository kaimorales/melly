//! Native file selection for Meta Model API chat.
#![allow(deprecated, unexpected_cfgs)]
use crate::{AppState, Reply};
use melly_core::engine::TextAttachment;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

#[derive(Default)]
pub struct DraftAttachment(Option<(String, TextAttachment)>);

impl DraftAttachment {
    pub fn get(&self, id: &str) -> Reply<&TextAttachment> {
        self.0
            .as_ref()
            .filter(|(token, _)| token == id)
            .map(|(_, file)| file)
            .ok_or_else(|| "Attach the file again before sending".into())
    }
    pub fn remove(&mut self, id: &str) {
        if self.0.as_ref().is_some_and(|(token, _)| token == id) {
            self.0 = None;
        }
    }
}

#[derive(Serialize)]
pub struct PickedAttachment {
    id: String,
    name: String,
}

#[tauri::command]
pub async fn pick_muse_attachment(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
) -> Reply<Option<PickedAttachment>> {
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = send.send(pick_file());
    })
    .map_err(|_| "Could not open the file picker")?;
    let Some(path) = receive.await.map_err(|_| "File picker closed")? else {
        return Ok(None);
    };
    let attachment = tokio::task::spawn_blocking(move || read_selected_text(&path))
        .await
        .map_err(|_| "Could not read the selected file")??;
    let id = uuid::Uuid::new_v4().to_string();
    let name = attachment.name.clone();
    s.muse_attachment.lock().unwrap().0 = Some((id.clone(), attachment));
    Ok(Some(PickedAttachment { id, name }))
}

#[tauri::command]
pub async fn remove_muse_attachment(s: State<'_, AppState>, id: String) -> Reply<()> {
    s.muse_attachment.lock().unwrap().remove(&id);
    Ok(())
}

#[cfg(target_os = "macos")]
fn pick_file() -> Option<PathBuf> {
    use cocoa::{
        base::{id, nil},
        foundation::NSAutoreleasePool,
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    // AppKit owns the panel. Only its selection can reach the file reader.
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let panel: id = msg_send![class!(NSOpenPanel), openPanel];
        let _: () = msg_send![panel, setCanChooseDirectories: false];
        let _: () = msg_send![panel, setAllowsMultipleSelection: false];
        let _: () = msg_send![panel, setResolvesAliases: false];
        let answer: isize = msg_send![panel, runModal];
        let mut selected = None;
        if answer == 1 {
            let url: id = msg_send![panel, URL];
            let file: bool = msg_send![url, isFileURL];
            if file {
                let path: *const std::ffi::c_char = msg_send![url, fileSystemRepresentation];
                if !path.is_null() {
                    selected = Some(PathBuf::from(std::ffi::OsStr::from_bytes(
                        CStr::from_ptr(path).to_bytes(),
                    )));
                }
            }
        }
        pool.drain();
        selected
    }
}

#[cfg(not(target_os = "macos"))]
fn pick_file() -> Option<PathBuf> {
    None
}

fn read_selected_text(path: &Path) -> Reply<TextAttachment> {
    use std::io::Read;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "Could not open the selected file")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Could not read the selected file")?;
    let limit = TextAttachment::MAX_BYTES;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("Choose a UTF-8 text file of 24 KiB or less".into());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.chars().any(char::is_control) && !name.contains('\\'))
        .ok_or("Choose a file with a plain text name")?
        .to_owned();
    let mut text = String::new();
    file.take(limit as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|_| "Choose a valid UTF-8 text file")?;
    if text.len() > limit || text.trim().is_empty() || text.contains('\0') {
        return Err("Choose a nonempty UTF-8 text file of 24 KiB or less".into());
    }
    Ok(TextAttachment { name, text })
}
