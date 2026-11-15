use crate::{
    AppState, Reply,
    coding_tests::Suite,
    error,
    file_edits::{Grant, file_edit_target::Target, same_link, workspace_for_chat},
};
use anyhow::{Context, Result, bail, ensure};
use melly_core::{
    coding::{Call, CodingTools},
    file_edits::{DiffLine, MAX_FILE_BYTES, Proposal, diff_lines, validate_path},
    linked,
    models::StreamUpdate,
    projects::ProjectLink,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct Approvals(HashMap<Uuid, oneshot::Sender<bool>>);

#[tauri::command]
pub async fn confirm_coding_action(
    s: State<'_, AppState>,
    id: String,
    approved: bool,
) -> Reply<()> {
    let id = Uuid::parse_str(&id).map_err(error)?;
    let sender = s
        .coding_approvals
        .lock()
        .unwrap()
        .0
        .remove(&id)
        .ok_or("The action expired")?;
    sender
        .send(approved)
        .map_err(|_| "The action expired".into())
}

#[tauri::command]
pub async fn coding_activity(
    s: State<'_, AppState>,
    chat_id: String,
) -> Reply<HashMap<String, Vec<Value>>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .coding_activity(&chat_id)
        .map_err(error)
}

pub(crate) struct ProjectTools {
    pub app: tauri::AppHandle,
    pub workspace: ProjectLink,
    pub grant: Option<Grant>,
    pub reads: std::sync::Mutex<HashMap<String, String>>,
}

enum Prepared {
    Change {
        path: String,
        label: &'static str,
        target: Target,
        updated: Option<String>,
        diff: Vec<DiffLine>,
    },
    Tests(Suite),
}

#[derive(Serialize)]
struct Approval<'a> {
    id: String,
    label: &'a str,
    path: Option<&'a str>,
    diff: &'a [DiffLine],
}

impl ProjectTools {
    fn check_workspace(&self, chat: &str) -> Result<()> {
        let state = self.app.state::<AppState>();
        let store = state.engine.store.lock().unwrap();
        let current = workspace_for_chat(&store, chat)
            .map_err(anyhow::Error::msg)?
            .context("The chat no longer has a linked folder")?;
        ensure!(
            same_link(&current, &self.workspace),
            "The linked folder changed. Start a new turn."
        );
        Ok(())
    }

    fn has_grant(&self, chat: &str) -> bool {
        let state = self.app.state::<AppState>();
        let current = state.file_edits.lock().unwrap().grant(chat);
        self.grant
            .as_ref()
            .zip(current.as_ref())
            .is_some_and(|(turn, current)| turn.matches(current, &self.workspace))
    }

    fn emit(&self, chat: &str, message: &str, kind: &str, text: String) {
        let _ = self.app.emit(
            "chat-stream",
            StreamUpdate {
                chat_id: chat.into(),
                message_id: message.into(),
                kind: kind.into(),
                text,
                sources: vec![],
            },
        );
    }

    async fn ask(
        &self,
        chat: &str,
        message: &str,
        action: &Prepared,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let state = self.app.state::<AppState>();
        let id = Uuid::new_v4();
        let (sender, receiver) = oneshot::channel();
        state.coding_approvals.lock().unwrap().0.insert(id, sender);
        let waiting = Waiting {
            app: self.app.clone(),
            id,
            chat: chat.into(),
            message: message.into(),
        };
        let preview = match action {
            Prepared::Change {
                path, diff, label, ..
            } => Approval {
                id: id.to_string(),
                path: Some(path),
                diff,
                label,
            },
            Prepared::Tests(suite) => Approval {
                id: id.to_string(),
                path: None,
                diff: &[],
                label: suite.label(),
            },
        };
        self.emit(
            chat,
            message,
            "coding-approval",
            serde_json::to_string(&preview)?,
        );
        let decision = tokio::select! {
            _ = cancel.cancelled() => bail!("Action cancelled"),
            decision = tokio::time::timeout(Duration::from_secs(900), receiver) => decision.context("Approval timed out")??,
        };
        drop(waiting);
        ensure!(decision, "The user declined the action");
        Ok(())
    }
}

struct Waiting {
    app: tauri::AppHandle,
    id: Uuid,
    chat: String,
    message: String,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.app
            .state::<AppState>()
            .coding_approvals
            .lock()
            .unwrap()
            .0
            .remove(&self.id);
        let _ = self.app.emit(
            "chat-stream",
            StreamUpdate {
                chat_id: self.chat.clone(),
                message_id: self.message.clone(),
                kind: "coding-approval-closed".into(),
                text: self.id.to_string(),
                sources: vec![],
            },
        );
    }
}

#[async_trait::async_trait]
impl CodingTools for ProjectTools {
    async fn execute(
        &self,
        chat: &str,
        message: &str,
        call: Call,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.check_workspace(chat)?;
        let operation = linked::revision(&serde_json::to_vec(&call)?);
        if matches!(
            call,
            Call::CreateFile { .. } | Call::EditFile { .. } | Call::DeleteFile { .. }
        ) {
            let state = self.app.state::<AppState>();
            let receipt = state
                .engine
                .store
                .lock()
                .unwrap()
                .file_operation_receipt(message, &operation)?;
            if let Some(receipt) = receipt
                && let Some(result) = reused_change(&self.workspace, &receipt)?
            {
                state
                    .engine
                    .store
                    .lock()
                    .unwrap()
                    .set_file_change_state(&receipt.id, "applied")?;
                state
                    .engine
                    .store
                    .lock()
                    .unwrap()
                    .record_coding_activity(message, &result)?;
                self.emit(
                    chat,
                    message,
                    "coding-result",
                    serde_json::to_string(&result)?,
                );
                return Ok(result);
            }
        }
        if let Call::SearchRepo { query } = call {
            let workspace = self.workspace.clone();
            self.emit(chat, message, "tool", "Searching project files".into());
            return tokio::task::spawn_blocking(move || search(&workspace, &query, &cancel))
                .await?;
        }
        if let Call::ReadFile { path, start_line } = call {
            let workspace = self.workspace.clone();
            self.emit(chat, message, "tool", "Reading project file".into());
            let selected = path.clone();
            let (result, revision) = tokio::task::spawn_blocking(move || {
                read(&workspace, &selected, start_line.unwrap_or(1))
            })
            .await??;
            self.reads.lock().unwrap().insert(path, revision);
            return Ok(result);
        }
        let expected_revision =
            if let Call::DeleteFile { path } | Call::EditFile { path, .. } = &call {
                Some(
                    self.reads
                        .lock()
                        .unwrap()
                        .get(path)
                        .cloned()
                        .context("Read this file in the current turn before changing it")?,
                )
            } else {
                None
            };
        let workspace = self.workspace.clone();
        let action = tokio::task::spawn_blocking(move || {
            let event = prepare(&workspace, call)?;
            if let Some(expected) = expected_revision
                && let Prepared::Change { target, .. } = &event
            {
                ensure!(
                    target
                        .original()
                        .is_some_and(|text| linked::revision(text.as_bytes()) == expected),
                    "The file changed since it was read. Read it again."
                );
            }
            Ok::<_, anyhow::Error>(event)
        })
        .await??;
        let automatic = self.has_grant(chat);
        if !automatic {
            self.ask(chat, message, &action, &cancel).await?;
        }
        self.check_workspace(chat)?;
        ensure!(!cancel.is_cancelled(), "Action cancelled");
        let outcome = match action {
            Prepared::Change {
                path,
                label,
                target,
                updated,
                diff,
            } => {
                let state = self.app.state::<AppState>();
                let store = state.engine.store.clone();
                let operations = state.project_operations.clone();
                let workspace = self.workspace.clone();
                let app = self.app.clone();
                let grant = self.grant.clone();
                let chat = chat.to_owned();
                let message = message.to_owned();
                let checkpoint_path = path.clone();
                tokio::task::spawn_blocking(move || -> Result<()> {
                    let _operation = operations.lock().unwrap();
                    let state = app.state::<AppState>();
                    let permissions = state.file_edits.lock().unwrap();
                    ensure!(
                        !automatic
                            || grant
                                .as_ref()
                                .zip(permissions.grant(&chat).as_ref())
                                .is_some_and(|(a, b)| a.matches(b, &workspace)),
                        "Auto-apply was revoked"
                    );
                    let store = store.lock().unwrap();
                    let current = workspace_for_chat(&store, &chat)
                        .map_err(anyhow::Error::msg)?
                        .context("The linked project changed")?;
                    ensure!(
                        same_link(&current, &workspace),
                        "The linked project changed"
                    );
                    ensure!(!cancel.is_cancelled(), "Action cancelled");
                    let id = store.checkpoint_file(
                        &message,
                        &workspace,
                        &checkpoint_path,
                        target.original(),
                        updated.as_deref(),
                        target.original_mode(),
                    )?;
                    store.bind_file_operation(&message, &operation, &id)?;
                    let result = if let Some(updated) = updated {
                        target.write(&updated)
                    } else {
                        target.delete()
                    };
                    if result.is_err() {
                        store.set_file_change_state(&id, "failed")?;
                    }
                    result?;
                    store.set_file_change_state(&id, "applied")
                })
                .await??;
                json!({"ok":true,"action":label,"path":path,"diff":diff})
            }
            Prepared::Tests(suite) => {
                ensure!(!automatic || self.has_grant(chat), "Auto-apply was revoked");
                let current = linked::root_identity(&self.workspace.path)?;
                ensure!(
                    current == (self.workspace.device, self.workspace.inode),
                    "The linked project changed"
                );
                self.emit(chat, message, "tool", format!("Running {}", suite.label()));
                suite.run(&self.workspace.path, cancel).await?
            }
        };
        // A completed operation remains successful if saving its activity fails.
        let state = self.app.state::<AppState>();
        let _ = state
            .engine
            .store
            .lock()
            .unwrap()
            .record_coding_activity(message, &outcome);
        self.emit(
            chat,
            message,
            "coding-result",
            serde_json::to_string(&outcome)?,
        );
        let mut result = outcome;
        if let Some(object) = result.as_object_mut() {
            object.remove("diff");
        }
        Ok(result)
    }
}

fn reused_change(
    workspace: &ProjectLink,
    receipt: &melly_core::file_history::Change,
) -> Result<Option<Value>> {
    if !matches!(receipt.state.as_str(), "prepared" | "applied")
        || receipt.folder != workspace.folder_id
        || receipt.root != workspace.path
        || (receipt.device, receipt.inode) != (workspace.device, workspace.inode)
    {
        return Ok(None);
    }
    let target = Target::select(
        &workspace.path,
        (workspace.device, workspace.inode),
        relative(&receipt.path)?,
        true,
    )?;
    if target.original() != receipt.after.as_deref() {
        return Ok(None);
    }
    Ok(Some(
        json!({"ok":true,"reused":true,"action":if receipt.before.is_none(){"Create"}else if receipt.after.is_none(){"Delete"}else{"Edit"},"path":receipt.path,"diff":[],"note":"This action already completed. The current file matches its recorded result; no additional write was performed."}),
    ))
}

fn relative(path: &str) -> Result<&Path> {
    validate_path(path)?;
    let path = Path::new(path);
    ensure!(
        path.iter().all(|name| name
            .to_str()
            .is_some_and(|name| !melly_core::projects::excluded(name))),
        "This file is excluded from the linked project"
    );
    Ok(path)
}

fn read(workspace: &ProjectLink, path: &str, start_line: usize) -> Result<(Value, String)> {
    let relative = relative(path)?;
    ensure!(start_line > 0, "Lines are numbered from 1");
    let bytes = linked::read_scoped_with_limit(
        &workspace.path,
        Some((workspace.device, workspace.inode)),
        relative,
        1024 * 1024,
    )?;
    let text = String::from_utf8(bytes).context("Choose a UTF-8 text file")?;
    ensure!(!text.contains('\0'), "Choose a text file");
    let mut lines = text
        .split_inclusive('\n')
        .enumerate()
        .skip(start_line - 1)
        .peekable();
    let mut content = String::new();
    let mut next_line = None;
    for (index, line) in lines.by_ref() {
        if content.len() + line.len() > 12_000 || index + 1 >= start_line.saturating_add(160) {
            if content.is_empty() {
                content.push_str(&line[..line.floor_char_boundary(12_000)]);
            }
            next_line = Some(index + if index + 1 == start_line { 2 } else { 1 });
            break;
        }
        content.push_str(line);
    }
    Ok((
        json!({"path":path,"text":content,"start_line":start_line,"next_line":next_line}),
        linked::revision(text.as_bytes()),
    ))
}

fn search(workspace: &ProjectLink, query: &str, cancel: &CancellationToken) -> Result<Value> {
    let query = query.trim().to_lowercase();
    ensure!(
        !query.is_empty() && query.len() <= 160,
        "Enter a search term up to 160 bytes"
    );
    let identity = (workspace.device, workspace.inode);
    let mut queue = vec![PathBuf::new()];
    let mut inspected = 0;
    let mut hits = Vec::new();
    while let Some(parent) = queue.pop() {
        if inspected >= 1500 || hits.len() >= 15 || cancel.is_cancelled() {
            break;
        }
        for (name, directory) in
            Target::entries(&workspace.path, identity, &parent, 1500 - inspected)?
        {
            if hits.len() >= 15 || cancel.is_cancelled() {
                break;
            }
            inspected += 1;
            let path = parent.join(name);
            if directory {
                if path.components().count() < 12 {
                    queue.push(path);
                }
                continue;
            }
            let name = path.to_string_lossy();
            if name.to_lowercase().contains(&query) {
                hits.push(json!({"path":name}));
            }
            if hits.len() >= 15 {
                break;
            }
            // Read a bounded number of small files while matching content.
            if inspected > 300 {
                continue;
            }
            let Ok(bytes) =
                linked::read_scoped_with_limit(&workspace.path, Some(identity), &path, 64 * 1024)
            else {
                continue;
            };
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            if text.len() > 64 * 1024 || text.contains('\0') {
                continue;
            }
            for (line, item) in text.lines().enumerate() {
                if item.to_lowercase().contains(&query) {
                    hits.push(json!({"path":name,"line":line+1,"text":item.chars().take(200).collect::<String>()}));
                    if hits.len() >= 15 {
                        break;
                    }
                }
            }
        }
    }
    Ok(json!({"matches":hits,"bounded":true,"entries_checked":inspected}))
}

fn prepare(workspace: &ProjectLink, call: Call) -> Result<Prepared> {
    let (path, label, content, edits) = match call {
        Call::CreateFile { path, content } => (path, "Create", Some(content), None),
        Call::EditFile { path, edits } => (path, "Edit", None, Some(edits)),
        Call::DeleteFile { path } => (path, "Delete", None, None),
        Call::RunTests { check } => {
            return Ok(Prepared::Tests(Suite::detect(
                &workspace.path,
                check.as_deref(),
            )?));
        }
        _ => bail!("Invalid action"),
    };
    let target = Target::select(
        &workspace.path,
        (workspace.device, workspace.inode),
        relative(&path)?,
        content.is_some(),
    )?;
    let updated = if let Some(content) = content {
        ensure!(
            target.original().is_none(),
            "A file already exists at that path"
        );
        ensure!(
            content.len() <= MAX_FILE_BYTES && !content.contains('\0'),
            "The new file must be UTF-8 text of 24 KiB or less"
        );
        Some(content)
    } else if let Some(edits) = edits {
        Some(Proposal::edit(path.clone(), edits).apply_to(target.original())?)
    } else {
        None
    };
    let diff = diff_lines(target.original(), updated.as_deref().unwrap_or_default());
    Ok(Prepared::Change {
        path,
        target,
        updated,
        diff,
        label,
    })
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn project_tools_search_read_create_edit_and_delete_scoped_files() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let scope = dir.path().join("linked");
    std::fs::create_dir(&scope)?;
    let scope = scope.canonicalize()?;
    let (device, inode) = linked::root_identity(&scope)?;
    let workspace = ProjectLink {
        folder_id: "fixture".into(),
        name: "linked".into(),
        status: "ready".into(),
        path: scope.clone(),
        device,
        inode,
    };
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "PRIVATE_NEEDLE")?;
    std::os::unix::fs::symlink(&outside, scope.join("linked.txt"))?;
    std::fs::write(scope.join(".env"), "PRIVATE_NEEDLE")?;
    assert!(
        search(&workspace, "PRIVATE_NEEDLE", &CancellationToken::new())?["matches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(read(&workspace, "linked.txt", 1).is_err());
    assert!(read(&workspace, "../outside.txt", 1).is_err());
    let save = |call| -> Result<()> {
        let Prepared::Change {
            target, updated, ..
        } = prepare(&workspace, call)?
        else {
            bail!("Expected file change")
        };
        if let Some(updated) = updated {
            target.write(&updated)
        } else {
            target.delete()
        }
    };
    save(Call::CreateFile {
        path: "sample.py".into(),
        content: "print(41)\n".into(),
    })?;
    assert_eq!(read(&workspace, "sample.py", 1)?.0["text"], "print(41)\n");
    let edit: Call = serde_json::from_value(
        json!({"tool":"edit_file","path":"sample.py","edits":[{"old":"41","new":"42"}]}),
    )?;
    save(edit)?;
    assert_eq!(
        search(&workspace, "print(42)", &CancellationToken::new())?["matches"][0]["path"],
        "sample.py"
    );
    save(Call::DeleteFile {
        path: "sample.py".into(),
    })?;
    assert!(!scope.join("sample.py").exists());
    assert_eq!(std::fs::read_to_string(outside)?, "PRIVATE_NEEDLE");
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
#[test]
fn a_completed_write_is_reused_without_changing_the_file_again() -> Result<()> {
    use melly_core::storage::Store;
    let root = tempfile::tempdir()?;
    let store = Store::memory()?;
    let folder = store.create_folder("Synthetic")?;
    store.set_project_link(&folder.id, &root.path().canonicalize()?)?;
    let link = store.project_link(&folder.id)?.unwrap();
    let chat = store.create_chat(Some(&folder.id))?;
    let message = store.insert_message(
        &chat.id,
        "assistant",
        "Interrupted",
        "interrupted",
        None,
        None,
        None,
    )?;
    store.register_coding_turn(&message.id, None)?;
    let change = store.checkpoint_file(
        &message.id,
        &link,
        "a.py",
        Some("old"),
        Some("new"),
        Some(0o600),
    )?;
    store.bind_file_operation(&message.id, "operation", &change)?;
    let file = root.path().join("a.py");
    std::fs::write(&file, "new")?;
    let receipt = store
        .file_operation_receipt(&message.id, "operation")?
        .unwrap();
    let before = std::fs::metadata(&file)?.modified()?;
    let result = reused_change(&link, &receipt)?.unwrap();
    assert_eq!(result["reused"], true);
    assert_eq!(std::fs::metadata(&file)?.modified()?, before);
    std::fs::write(&file, "user's later edit")?;
    assert!(reused_change(&link, &receipt)?.is_none());
    assert_eq!(std::fs::read_to_string(file)?, "user's later edit");
    Ok(())
}
