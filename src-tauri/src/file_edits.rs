use crate::{AppState, Reply, error};
use melly_core::{
    file_edits::{DiffLine, Proposal, diff_lines},
    models::StreamUpdate,
    projects::ProjectLink,
    storage::Store,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

#[path = "file_edit_target.rs"]
pub(crate) mod file_edit_target;
use file_edit_target::Target;

const REVIEW_WINDOW: Duration = Duration::from_secs(900);

#[derive(Default)]
pub struct FileEdits {
    pending: HashMap<String, Pending>,
    grants: HashMap<String, Grant>,
}

#[derive(Clone)]
pub(crate) struct Grant {
    ticket: Uuid,
    workspace: ProjectLink,
}

impl Grant {
    pub(crate) fn matches(&self, other: &Self, workspace: &ProjectLink) -> bool {
        self.ticket == other.ticket && same_link(workspace, &self.workspace)
    }
}

impl FileEdits {
    pub(crate) fn revoke_workspace(&mut self, folder_id: &str) {
        self.grants
            .retain(|_, grant| grant.workspace.folder_id != folder_id);
    }

    pub(crate) fn revoke_chat(&mut self, chat_id: &str) {
        self.grants.remove(chat_id);
    }

    pub(crate) fn grant(&self, chat_id: &str) -> Option<Grant> {
        self.grants.get(chat_id).cloned()
    }
}

struct Pending {
    ticket: String,
    chat_id: String,
    workspace: ProjectLink,
    path: String,
    target: Target,
    after: String,
    diff: Vec<DiffLine>,
    reviewed: Instant,
}

impl Pending {
    fn prepare(chat_id: String, workspace: ProjectLink, proposal: Proposal) -> Reply<Self> {
        let target = Target::select(
            &workspace.path,
            (workspace.device, workspace.inode),
            Path::new(&proposal.path),
            proposal.permits_new_file(),
        )
        .map_err(error)?;
        let after = proposal.apply_to(target.original()).map_err(error)?;
        let diff = diff_lines(target.original(), &after);
        Ok(Self {
            ticket: Uuid::new_v4().to_string(),
            chat_id,
            workspace,
            path: proposal.path,
            target,
            after,
            diff,
            reviewed: Instant::now(),
        })
    }

    fn apply(&self, store: &Store, message_id: &str) -> Reply<()> {
        let (link, applied) = store
            .file_edit_scope(&self.chat_id, message_id)
            .map_err(error)?;
        if applied || !same_link(&link, &self.workspace) {
            return Err("The project changed after review. Review the proposal again.".into());
        }
        let id = store
            .checkpoint_file(
                message_id,
                &self.workspace,
                &self.path,
                self.target.original(),
                Some(&self.after),
                self.target.original_mode(),
            )
            .map_err(error)?;
        if let Err(reason) = self.target.write(&self.after) {
            let _ = store.set_file_change_state(&id, "failed");
            return Err(error(reason));
        }
        store.set_file_change_state(&id, "applied").map_err(error)?;
        store
            .mark_file_edit_applied(message_id, &self.diff)
            .map_err(error)
    }
}

pub(crate) fn same_link(left: &ProjectLink, right: &ProjectLink) -> bool {
    left.folder_id == right.folder_id
        && left.device == right.device
        && left.inode == right.inode
        && left.path == right.path
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum Preview {
    Applied {
        path: String,
        diff: Vec<DiffLine>,
    },
    Review {
        ticket: String,
        path: String,
        diff: Vec<DiffLine>,
    },
}

pub(crate) fn workspace_for_chat(store: &Store, chat_id: &str) -> Reply<Option<ProjectLink>> {
    if let Some(folder) = store.chat(chat_id).map_err(error)?.folder_id {
        store.ensure_active_project(&folder).map_err(error)?;
        return store.project_link(&folder).map_err(error);
    }
    Ok(None)
}

#[tauri::command]
pub async fn file_edit_mode(s: State<'_, AppState>, chat_id: String) -> Reply<Option<bool>> {
    let workspace = workspace_for_chat(&s.engine.store.lock().unwrap(), &chat_id)?;
    let edits = s.file_edits.lock().unwrap();
    Ok(workspace.map(|link| {
        edits
            .grant(&chat_id)
            .is_some_and(|grant| same_link(&link, &grant.workspace))
    }))
}

#[tauri::command]
pub async fn set_file_edit_mode(
    s: State<'_, AppState>,
    chat_id: String,
    automatic: bool,
) -> Reply<()> {
    if automatic {
        let workspace = workspace_for_chat(&s.engine.store.lock().unwrap(), &chat_id)?
            .ok_or("Link a project folder before enabling automatic edits")?;
        let mut edits = s.file_edits.lock().unwrap();
        edits.grants.insert(
            chat_id,
            Grant {
                ticket: Uuid::new_v4(),
                workspace,
            },
        );
    } else {
        s.file_edits.lock().unwrap().grants.remove(&chat_id);
    }
    Ok(())
}

fn applied_preview(store: &Store, chat_id: &str, message_id: &str) -> Reply<Option<Preview>> {
    let Some(diff) = store
        .saved_file_edit_diff(chat_id, message_id)
        .map_err(error)?
    else {
        return Ok(None);
    };
    let proposal =
        Proposal::parse(&store.message(message_id).map_err(error)?.content).map_err(error)?;
    Ok(Some(Preview::Applied {
        path: proposal.path,
        diff,
    }))
}

#[tauri::command]
pub async fn applied_file_edit(
    s: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Reply<Option<Preview>> {
    applied_preview(&s.engine.store.lock().unwrap(), &chat_id, &message_id)
}

#[tauri::command]
pub async fn review_file_edit(
    s: State<'_, AppState>,
    chat_id: String,
    message_id: String,
) -> Reply<Preview> {
    let (workspace, proposal) = {
        let store = s.engine.store.lock().unwrap();
        if let Some(applied) = applied_preview(&store, &chat_id, &message_id)? {
            return Ok(applied);
        }
        let (workspace, _) = store
            .file_edit_scope(&chat_id, &message_id)
            .map_err(error)?;
        let proposal =
            Proposal::parse(&store.message(&message_id).map_err(error)?.content).map_err(error)?;
        (workspace, proposal)
    };
    let pending =
        tokio::task::spawn_blocking(move || Pending::prepare(chat_id, workspace, proposal))
            .await
            .map_err(|_| "Could not review the file change")??;
    let preview = Preview::Review {
        ticket: pending.ticket.clone(),
        path: pending.path.clone(),
        diff: pending.diff.clone(),
    };
    let mut edits = s.file_edits.lock().unwrap();
    edits
        .pending
        .retain(|_, entry| entry.reviewed.elapsed() < REVIEW_WINDOW);
    if edits.pending.len() >= 32
        && !edits.pending.contains_key(&message_id)
        && let Some(oldest) = edits
            .pending
            .iter()
            .min_by_key(|(_, edit)| edit.reviewed)
            .map(|(key, _)| key.clone())
    {
        edits.pending.remove(&oldest);
    }
    edits.pending.insert(message_id, pending);
    Ok(preview)
}

#[tauri::command]
pub async fn apply_file_edit(
    s: State<'_, AppState>,
    message_id: String,
    ticket: String,
) -> Reply<()> {
    let pending =
        {
            let mut edits = s.file_edits.lock().unwrap();
            if !edits.pending.get(&message_id).is_some_and(|edit| {
                edit.ticket == ticket && edit.reviewed.elapsed() < REVIEW_WINDOW
            }) {
                return Err("This review expired. Review the change again.".into());
            }
            edits.pending.remove(&message_id).unwrap()
        };
    let operations = s.project_operations.clone();
    let store = s.engine.store.clone();
    tokio::task::spawn_blocking(move || {
        let _operation = operations.lock().unwrap();
        pending.apply(&store.lock().unwrap(), &message_id)
    })
    .await
    .map_err(|_| "Could not apply the file change")?
}

pub(crate) fn finish(app: tauri::AppHandle, update: StreamUpdate, grant: Grant) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut update = update;
        if let Err(reason) = auto_apply(
            &state.file_edits,
            &state.project_operations,
            &state.engine.store,
            &update,
            &grant,
        ) {
            update.text = format!("Automatic edit could not be saved. {reason}");
        }
        let _ = app.emit("chat-stream", update);
    });
}

fn auto_apply(
    access: &Mutex<FileEdits>,
    operations: &Mutex<()>,
    store: &Mutex<Store>,
    update: &StreamUpdate,
    grant: &Grant,
) -> Reply<()> {
    let message = store
        .lock()
        .unwrap()
        .message(&update.message_id)
        .map_err(error)?;
    if !message
        .content
        .lines()
        .any(|line| line == "```melly-file-edit")
    {
        return Ok(());
    }
    let _operation = operations.lock().unwrap();
    let edits = access.lock().unwrap();
    if !edits
        .grants
        .get(&update.chat_id)
        .is_some_and(|current| current.ticket == grant.ticket)
    {
        return Ok(());
    }
    let store = store.lock().unwrap();
    let (link, applied) = store
        .file_edit_scope(&update.chat_id, &update.message_id)
        .map_err(error)?;
    if applied || !same_link(&link, &grant.workspace) {
        return Ok(());
    }
    let proposal = Proposal::parse(&message.content).map_err(error)?;
    Pending::prepare(update.chat_id.clone(), link, proposal)?.apply(&store, &update.message_id)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn automatic_edits_require_the_same_live_grant_and_keep_the_diff() -> anyhow::Result<()> {
        let profile = test_directory();
        let project = test_directory();
        let store = Store::open(&profile.join("test.sqlite3"))?;
        let folder = store.create_folder("Test project")?;
        store.set_project_link(&folder.id, &project)?;
        let workspace = store.project_link(&folder.id)?.unwrap();
        let chat = store.create_chat(Some(&folder.id))?;
        let reply = "Proposed.\n\n```melly-file-edit\n{\"path\":\"sample.txt\",\"edits\":[{\"old\":\"red\",\"new\":\"green\"}]}\n```";
        let message =
            store.insert_message(&chat.id, "assistant", reply, "complete", None, None, None)?;
        store.record_file_edit_scope(&message.id, &workspace)?;
        let update = StreamUpdate {
            chat_id: chat.id.clone(),
            message_id: message.id.clone(),
            kind: "done".into(),
            text: String::new(),
            sources: vec![],
        };
        let path = project.join("sample.txt");
        std::fs::write(&path, "red\n")?;
        let grant = Grant {
            ticket: Uuid::new_v4(),
            workspace,
        };
        let access = Mutex::new(FileEdits::default());
        let operations = Mutex::new(());
        let store = Mutex::new(store);
        auto_apply(&access, &operations, &store, &update, &grant).map_err(anyhow::Error::msg)?;
        assert_eq!(std::fs::read_to_string(&path)?, "red\n");
        access
            .lock()
            .unwrap()
            .grants
            .insert(chat.id.clone(), grant.clone());
        let stale = Grant {
            ticket: Uuid::new_v4(),
            workspace: grant.workspace.clone(),
        };
        auto_apply(&access, &operations, &store, &update, &stale).map_err(anyhow::Error::msg)?;
        assert_eq!(std::fs::read_to_string(&path)?, "red\n");
        auto_apply(&access, &operations, &store, &update, &grant).map_err(anyhow::Error::msg)?;
        assert_eq!(std::fs::read_to_string(&path)?, "green\n");
        let store = store.lock().unwrap();
        assert!(store.file_edit_scope(&chat.id, &message.id)?.1);
        assert_eq!(
            store
                .saved_file_edit_diff(&chat.id, &message.id)?
                .unwrap()
                .iter()
                .map(|row| row.kind)
                .collect::<String>(),
            "-+"
        );
        drop(store);
        std::fs::remove_dir_all(profile)?;
        std::fs::remove_dir_all(project)?;
        Ok(())
    }

    fn test_directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("melly-edit-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        path
    }
}
