use crate::{
    AppState, Reply, error,
    file_edits::{file_edit_target::Target, same_link, workspace_for_chat},
};
use anyhow::{Context, Result, ensure};
use melly_core::{file_history::Change, storage::Store};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};
use tauri::State;

#[derive(Serialize)]
pub struct ChangeInfo {
    id: String,
    path: String,
    state: String,
}

#[tauri::command]
pub async fn file_changes(
    s: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Reply<Vec<ChangeInfo>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .file_changes(&chat_id, &message_id)
        .map(|items| {
            items
                .into_iter()
                .filter(|c| c.state != "failed")
                .map(|c| ChangeInfo {
                    id: c.id,
                    path: c.path,
                    state: c.state,
                })
                .collect()
        })
        .map_err(error)
}

#[tauri::command]
pub async fn undo_file_changes(
    s: State<'_, AppState>,
    chat_id: String,
    message_id: String,
    change_id: Option<String>,
) -> Reply<()> {
    let engine = s.engine.clone();
    let operations = s.project_operations.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        // Serialize with turn admission and project writes, including other chats.
        engine.with_idle_store(|store| undo(store, &chat_id, &message_id, change_id.as_deref()))
    })
    .await
    .map_err(error)?
    .map_err(error)
}

type UndoPlan<'a> = (Target, Option<String>, Option<u32>, Vec<&'a Change>);

fn undo(store: &Store, chat: &str, message: &str, selected: Option<&str>) -> Result<()> {
    let workspace = workspace_for_chat(store, chat)
        .map_err(anyhow::Error::msg)?
        .context("Reconnect this chat's linked project before undoing")?;
    let all = store.file_changes(chat, message)?;
    if let Some(id) = selected {
        ensure!(
            all.iter().any(|c| c.id == id),
            "File change not found in this response"
        );
    }
    let changes: Vec<_> = all
        .iter()
        .filter(|c| {
            !matches!(c.state.as_str(), "undone" | "failed") && selected.is_none_or(|id| c.id == id)
        })
        .collect();
    let mut plans: BTreeMap<&str, UndoPlan<'_>> = BTreeMap::new();
    for change in &changes {
        let expected = melly_core::projects::ProjectLink {
            folder_id: change.folder.clone(),
            name: String::new(),
            status: String::new(),
            path: change.root.clone(),
            device: change.device,
            inode: change.inode,
        };
        ensure!(
            same_link(&workspace, &expected),
            "The linked project changed; the original files were left unchanged"
        );
        ensure!(
            !store.has_later_file_change(change)?,
            "Undo the newer change to {} first",
            change.path
        );
        if let Some(id) = selected {
            ensure!(
                !all.iter()
                    .take_while(|c| c.id != id)
                    .any(|c| c.path == change.path
                        && !matches!(c.state.as_str(), "undone" | "failed")),
                "Undo the newer change to {} first",
                change.path
            );
        }
        if !plans.contains_key(change.path.as_str()) {
            let target = Target::select(
                &workspace.path,
                (workspace.device, workspace.inode),
                Path::new(&change.path),
                true,
            )?;
            let same_path: Vec<_> = changes.iter().filter(|c| c.path == change.path).collect();
            let recovered = same_path.iter().all(|c| c.state == "undoing")
                && target.original() == same_path.last().unwrap().before.as_deref();
            let current = if recovered {
                change.after.clone()
            } else {
                target.original().map(str::to_owned)
            };
            plans.insert(
                &change.path,
                (target.clone(), current, target.original_mode(), vec![]),
            );
        }
        let (_, current, mode, entries) = plans.get_mut(change.path.as_str()).unwrap();
        // A crash can leave a journal entry on either side of the filesystem write.
        let not_written =
            matches!(change.state.as_str(), "prepared" | "undoing") && *current == change.before;
        ensure!(
            not_written || *current == change.after,
            "{} changed after Melly edited it. Your current file was left unchanged.",
            change.path
        );
        *current = change.before.clone();
        *mode = change.mode;
        entries.push(change);
    }
    // Check all files before restoring any. Each target also rechecks before mutation.
    for (_, (target, before, mode, entries)) in plans {
        let ids: Vec<_> = entries.iter().map(|entry| entry.id.as_str()).collect();
        store.set_file_change_states(&ids, "undoing")?;
        if target.original() != before.as_deref() {
            if let Some(text) = before {
                target.write_restoring_mode(&text, mode)?;
            } else {
                target.delete()?;
            }
        }
        store.set_file_change_states(&ids, "undone")?;
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn restores_a_turn_preserves_user_edits_and_recovers_after_restart() -> Result<()> {
        let root = tempfile::tempdir()?;
        let profile = tempfile::tempdir()?;
        let database = profile.path().join("history.db");
        let store = Store::open(&database)?;
        let folder = store.create_folder("Synthetic project")?;
        store.set_project_link(&folder.id, root.path())?;
        let link = store.project_link(&folder.id)?.unwrap();
        let chat = store.create_chat(Some(&folder.id))?;
        let message = store.insert_message(
            &chat.id,
            "assistant",
            "Synthetic turn",
            "complete",
            None,
            None,
            None,
        )?;
        let apply = |name: &str, before: Option<&str>, after: Option<&str>| -> Result<String> {
            if let Some(text) = before {
                std::fs::write(root.path().join(name), text)?;
            }
            let target = Target::select(
                root.path(),
                (link.device, link.inode),
                Path::new(name),
                true,
            )?;
            let id = store.checkpoint_file(
                &message.id,
                &link,
                name,
                target.original(),
                after,
                target.original_mode(),
            )?;
            if let Some(text) = after {
                target.write(text)?;
            } else {
                target.delete()?;
            }
            store.set_file_change_state(&id, "applied")?;
            Ok(id)
        };
        let first = apply("edit.py", Some("old"), Some("first"))?;
        let second = apply("edit.py", None, Some("second"))?;
        apply("new.py", None, Some("created"))?;
        std::fs::write(root.path().join("deleted.py"), "deleted")?;
        std::fs::set_permissions(
            root.path().join("deleted.py"),
            std::fs::Permissions::from_mode(0o755),
        )?;
        apply("deleted.py", None, None)?;
        assert!(undo(&store, &chat.id, &message.id, Some(&first)).is_err());
        std::fs::write(root.path().join("edit.py"), "user's later edit")?;
        assert!(undo(&store, &chat.id, &message.id, None).is_err());
        assert!(root.path().join("new.py").exists());
        assert!(!root.path().join("deleted.py").exists());
        std::fs::write(root.path().join("edit.py"), "second")?;
        undo(&store, &chat.id, &message.id, Some(&second))?;
        assert_eq!(
            std::fs::read_to_string(root.path().join("edit.py"))?,
            "first"
        );
        drop(store);
        let store = Store::open(&database)?;
        undo(&store, &chat.id, &message.id, None)?;
        assert_eq!(std::fs::read_to_string(root.path().join("edit.py"))?, "old");
        assert!(!root.path().join("new.py").exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("deleted.py"))?,
            "deleted"
        );
        assert_eq!(
            std::fs::metadata(root.path().join("deleted.py"))?
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        undo(&store, &chat.id, &message.id, None)?;
        Ok(())
    }

    #[test]
    fn interrupted_group_undo_is_idempotent() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = Store::memory()?;
        let folder = store.create_folder("Synthetic")?;
        store.set_project_link(&folder.id, root.path())?;
        let link = store.project_link(&folder.id)?.unwrap();
        let chat = store.create_chat(Some(&folder.id))?;
        let reply =
            store.insert_message(&chat.id, "assistant", "Test", "complete", None, None, None)?;
        let first = store.checkpoint_file(
            &reply.id,
            &link,
            "a.txt",
            Some("old"),
            Some("middle"),
            Some(0o600),
        )?;
        let second = store.checkpoint_file(
            &reply.id,
            &link,
            "a.txt",
            Some("middle"),
            Some("new"),
            Some(0o600),
        )?;
        store.set_file_change_states(&[&first, &second], "undoing")?;
        std::fs::write(root.path().join("a.txt"), "old")?;
        undo(&store, &chat.id, &reply.id, None)?;
        assert!(
            store
                .file_changes(&chat.id, &reply.id)?
                .iter()
                .all(|c| c.state == "undone")
        );
        assert_eq!(std::fs::read_to_string(root.path().join("a.txt"))?, "old");
        Ok(())
    }

    #[test]
    fn rejects_other_chats_and_relinked_projects() -> Result<()> {
        let root = tempfile::tempdir()?;
        let other = tempfile::tempdir()?;
        let store = Store::memory()?;
        let folder = store.create_folder("Synthetic")?;
        store.set_project_link(&folder.id, root.path())?;
        let link = store.project_link(&folder.id)?.unwrap();
        let chat = store.create_chat(Some(&folder.id))?;
        let another = store.create_chat(Some(&folder.id))?;
        let reply =
            store.insert_message(&chat.id, "assistant", "Test", "complete", None, None, None)?;
        let id = store.checkpoint_file(&reply.id, &link, "a.txt", None, Some("new"), None)?;
        std::fs::write(root.path().join("a.txt"), "new")?;
        assert!(undo(&store, &another.id, &reply.id, Some(&id)).is_err());
        store.set_project_link(&folder.id, other.path())?;
        assert!(undo(&store, &chat.id, &reply.id, None).is_err());
        assert_eq!(std::fs::read_to_string(root.path().join("a.txt"))?, "new");
        Ok(())
    }
}
