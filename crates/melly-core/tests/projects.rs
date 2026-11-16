use melly_core::{projects::*, storage::Store};
#[test]
fn files_are_owned_by_existing_projects_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.sqlite3");
    let store = Store::open(&db).unwrap();
    let project = store.create_folder("Research").unwrap();
    let chat = store.create_chat(Some(&project.id)).unwrap();
    let file = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: project.id.clone(),
        name: "notes.txt".into(),
        bytes: 12,
        revision: "v1".into(),
        linked_path: None,
    };
    store.add_project_file(&file).unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(store.project_files(&project.id).unwrap()[0].id, file.id);
    assert_eq!(
        store.chat(&chat.id).unwrap().folder_id.as_deref(),
        Some(project.id.as_str())
    );
    assert!(store.project_files("missing").is_err());
    assert!(valid_name("../outside").is_err());
    assert!(project_directory(&dir.path().join("projects"), "../escape").is_err());
}

#[test]
fn retrieval_is_project_scoped_and_citations_keep_page_and_line_locations() {
    let store = Store::memory().unwrap();
    let project = store.create_folder("Research").unwrap();
    let other = store.create_folder("Other").unwrap();
    let a = store.create_chat(Some(&project.id)).unwrap();
    let b = store.create_chat(Some(&project.id)).unwrap();
    let outside = store.create_chat(Some(&other.id)).unwrap();
    let file = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: project.id.clone(),
        name: "study.pdf".into(),
        bytes: 10,
        revision: "v1".into(),
        linked_path: None,
    };
    store
        .index_project_file(
            &file,
            &text_passages("Transit ridership increased by eighteen percent.", Some(3)),
        )
        .unwrap();
    for chat in [a, b] {
        let sources = store.search_project_files(&chat.id, "ridership").unwrap();
        assert_eq!(sources.len(), 1);
        let citation = store.file_source(&sources[0].id).unwrap();
        assert_eq!(citation.page, Some(3));
        assert!(citation.text.contains("eighteen"));
    }
    assert!(
        store
            .search_project_files(&outside.id, "ridership")
            .unwrap()
            .is_empty()
    );
    let long = "é".repeat(6000);
    let parts = text_passages(&long, None);
    assert!(parts.iter().all(|part| part.text.len() <= 2400));
    assert_eq!(
        parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<String>()
            .trim_end(),
        long
    );
}

#[test]
fn archive_hides_search_and_delete_removes_chats_and_is_resumable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.sqlite3");
    let store = Store::open(&path).unwrap();
    let folder = store.create_folder("Research").unwrap();
    let chat = store.create_chat(Some(&folder.id)).unwrap();
    store.rename_chat(&chat.id, "Transit").unwrap();
    store
        .insert_message(
            &chat.id,
            "user",
            "Transit evidence",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store.set_project_archived(&folder.id, true).unwrap();
    assert!(store.folders().unwrap().is_empty());
    assert!(store.chats().unwrap().is_empty());
    assert!(store.search_chats("Transit").unwrap().is_empty());
    assert!(store.create_chat(Some(&folder.id)).is_err());
    store.set_project_archived(&folder.id, false).unwrap();
    assert_eq!(store.search_chats("Transit").unwrap().len(), 1);
    store.begin_project_deletion(&folder.id).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.archived_projects().unwrap()[0].deleting);
    assert!(store.set_project_archived(&folder.id, false).is_err());
    store.begin_project_deletion(&folder.id).unwrap();
    store.finish_project_deletion(&folder.id).unwrap();
    assert!(store.chat(&chat.id).is_err());
    assert!(store.search_chats("Transit").unwrap().is_empty());
}

#[test]
fn project_preferences_and_shared_retrieval_stay_scoped() {
    let store = Store::memory().unwrap();
    let folder = store.create_folder("Research").unwrap();
    let first = store.create_chat(Some(&folder.id)).unwrap();
    let second = store.create_chat(Some(&folder.id)).unwrap();
    store
        .insert_message(
            &first.id,
            "user",
            "Transit ridership 18 percent",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store.set_project_context(&folder.id, true).unwrap();
    assert!(store.folder(&folder.id).unwrap().context_enabled);
    assert!(store.folders().unwrap()[0].context_enabled);
    let refs = store.project_references(&second.id, "ridership").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].chat_id.as_deref(), Some(first.id.as_str()));
    assert!(
        store
            .project_references(&first.id, "ridership")
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn linked_reads_reject_escapes_symlinks_stale_revisions_and_preserve_unchanged_citations() {
    use melly_core::linked;
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("external");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("notes.md"), b"Transit research 18 percent").unwrap();
    std::fs::write(dir.path().join("secret.txt"), b"private").unwrap();
    symlink(dir.path(), root.join("escape")).unwrap();
    symlink(dir.path().join("secret.txt"), root.join("secret.md")).unwrap();
    let identity = linked::root_identity(&root).unwrap();
    assert!(
        linked::read_scoped(&root, Some(identity), std::path::Path::new("../secret.txt")).is_err()
    );
    assert!(
        linked::read_scoped(
            &root,
            Some(identity),
            std::path::Path::new("escape/secret.txt")
        )
        .is_err()
    );
    assert!(linked::read_scoped(&root, Some(identity), std::path::Path::new("secret.md")).is_err());
    let store = Store::open(&dir.path().join("db.sqlite3")).unwrap();
    let project = store.create_folder("Study").unwrap();
    let chat = store.create_chat(Some(&project.id)).unwrap();
    let link = store.set_project_link(&project.id, &root).unwrap();
    let bytes = std::fs::read(root.join("notes.md")).unwrap();
    let file = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: project.id.clone(),
        name: "notes.md".into(),
        bytes: bytes.len() as u64,
        revision: linked::revision(&bytes),
        linked_path: Some("notes.md".into()),
    };
    let indexed = vec![(
        file.clone(),
        text_passages(std::str::from_utf8(&bytes).unwrap(), None),
    )];
    store
        .replace_linked_index(&link, &indexed, "ready")
        .unwrap();
    let sources = store.search_project_files(&chat.id, "transit").unwrap();
    store.verify_file_sources(&sources).unwrap();
    store
        .replace_linked_index(&link, &indexed, "ready")
        .unwrap();
    assert!(store.file_source(&sources[0].id).is_ok());
    std::fs::write(root.join("notes.md"), b"Transit changed").unwrap();
    assert!(store.verify_file_sources(&sources).is_err());
    store.unlink_project(&project.id).unwrap();
    assert!(root.join("notes.md").exists());
    assert!(
        store
            .search_project_files(&chat.id, "transit")
            .unwrap()
            .is_empty()
    );
    assert!(store.file_source(&sources[0].id).is_err());
}

#[test]
fn legacy_folder_and_message_data_migrate_without_reorganization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    let folder_id = uuid::Uuid::new_v4().to_string();
    connection.execute_batch("CREATE TABLE folders(id TEXT PRIMARY KEY,title TEXT NOT NULL); CREATE TABLE chats(id TEXT PRIMARY KEY,title TEXT NOT NULL,folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,updated_at INTEGER NOT NULL);").unwrap();
    connection
        .execute(
            "INSERT INTO folders VALUES(?1,'Legacy project')",
            [&folder_id],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO chats VALUES('existing-chat','Existing chat',?1,1)",
            [&folder_id],
        )
        .unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.chat("existing-chat").unwrap().folder_id.as_deref(),
        Some(folder_id.as_str())
    );
    assert!(!store.folder(&folder_id).unwrap().context_enabled);
    assert!(store.archived_projects().unwrap().is_empty());
    let created = store.create_project("New project").unwrap();
    assert!(store.project_root().unwrap().join(created.id).is_dir());
}

#[test]
fn failed_trash_can_retry_and_deletion_keeps_external_files() {
    use melly_core::{credentials::MemorySecrets, engine::Engine};
    use std::sync::{Arc, Mutex};
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(&directory.path().join("db.sqlite3")).unwrap();
    let project = store.create_project("Synthetic").unwrap();
    let managed = store.project_root().unwrap().join(&project.id);
    std::fs::write(managed.join("synthetic.txt"), "managed").unwrap();
    let external = directory.path().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(external.join("keep.txt"), "external").unwrap();
    store.set_project_link(&project.id, &external).unwrap();
    let chat = store.create_chat(Some(&project.id)).unwrap();
    let engine = Engine::new(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
    );
    assert!(
        engine
            .delete_project(&project.id, |_| anyhow::bail!("Synthetic trash failure"))
            .is_err()
    );
    assert!(managed.exists());
    assert!(
        engine
            .store
            .lock()
            .unwrap()
            .folder(&project.id)
            .unwrap()
            .deleting
    );
    // Simulate a crash after Trash succeeded but before the database committed.
    std::fs::rename(&managed, directory.path().join("synthetic-trash")).unwrap();
    engine
        .delete_project(&project.id, |_| panic!("Already moved to Trash"))
        .unwrap();
    assert!(external.join("keep.txt").exists());
    assert!(engine.store.lock().unwrap().chat(&chat.id).is_err());
}

#[test]
fn replacing_and_removing_a_managed_file_invalidates_old_citations_atomically() {
    let store = Store::memory().unwrap();
    let project = store.create_folder("Files").unwrap();
    let other = store.create_folder("Other").unwrap();
    let chat = store.create_chat(Some(&project.id)).unwrap();
    let old = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: project.id.clone(),
        name: "notes.txt".into(),
        bytes: 10,
        revision: "a".into(),
        linked_path: None,
    };
    store
        .index_project_file(&old, &text_passages("Aurora port 4242", None))
        .unwrap();
    let citation = store.search_project_files(&chat.id, "Aurora").unwrap()[0]
        .id
        .clone();
    let mut replacement = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        revision: "b".into(),
        ..old.clone()
    };
    replacement.folder_id = other.id;
    assert!(
        store
            .replace_project_file(
                Some(&old.id),
                &replacement,
                &text_passages("Aurora port 4343", None)
            )
            .is_err()
    );
    assert!(store.file_source(&citation).is_ok());
    replacement.folder_id = project.id;
    store
        .replace_project_file(
            Some(&old.id),
            &replacement,
            &text_passages("Aurora port 4343", None),
        )
        .unwrap();
    assert!(store.file_source(&citation).is_err());
    assert!(store.project_file(&old.id).is_err());
    assert!(
        store.search_project_files(&chat.id, "4343").unwrap()[0]
            .snippet
            .contains("4343")
    );
    store.remove_managed_file(&replacement.id).unwrap();
    assert!(
        store
            .search_project_files(&chat.id, "Aurora")
            .unwrap()
            .is_empty()
    );
}
