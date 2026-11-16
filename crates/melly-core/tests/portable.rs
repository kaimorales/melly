use melly_core::{
    portable::Backup,
    projects::{ProjectFile, project_directory, text_passages},
    storage::Store,
};
#[test]
fn backup_restores_new_ids_files_attachments_and_citations_without_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("source.db")).unwrap();
    let folder = store.create_folder("Synthetic project").unwrap();
    store.set_project_context(&folder.id, true).unwrap();
    let chat = store.create_chat(Some(&folder.id)).unwrap();
    let file = ProjectFile {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: folder.id.clone(),
        name: "sample.txt".into(),
        bytes: 21,
        revision: melly_core::linked::revision(b"The test code is 4343."),
        linked_path: None,
    };
    let path = project_directory(&store.project_root().unwrap(), &folder.id)
        .unwrap()
        .join(&file.id);
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join(&file.name), b"The test code is 4343.").unwrap();
    store
        .index_project_file(&file, &text_passages("The test code is 4343.", None))
        .unwrap();
    let user = store
        .insert_message(
            &chat.id,
            "user",
            "What is the code?",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    let attached = store
        .save_draft_attachment("note.txt", "text", "A synthetic note", b"A synthetic note")
        .unwrap();
    store.attach_to_message(&user.id, &[attached.id]).unwrap();
    let source = store
        .search_project_files(&chat.id, "4343")
        .unwrap()
        .remove(0);
    let answer = store
        .insert_message(&chat.id, "assistant", "", "streaming", None, None, None)
        .unwrap();
    store
        .finish_message(
            &answer.id,
            &format!("4343 [file](melly-source:{})", source.id),
            "complete",
            &[source],
        )
        .unwrap();
    let bytes = store
        .backup(None, |p| Ok(std::fs::read(p)?))
        .unwrap()
        .bytes()
        .unwrap();
    let target = tempfile::tempdir().unwrap();
    let restored = Store::open(&target.path().join("restore.db")).unwrap();
    let counts = restored
        .restore_backup(Backup::from_bytes(&bytes).unwrap())
        .unwrap();
    assert_eq!(counts.messages, 2);
    assert_eq!(counts.files, 1);
    let new_folder = restored.folders().unwrap().remove(0);
    assert_ne!(new_folder.id, folder.id);
    assert!(!new_folder.context_enabled);
    let new_chat = restored.chats().unwrap().remove(0);
    assert_ne!(new_chat.id, chat.id);
    let messages = restored.messages(&new_chat.id, None).unwrap();
    assert_eq!(messages[0].attachments[0].name, "note.txt");
    let citation = &messages[1].sources[0];
    assert!(messages[1].content.contains(&citation.id));
    assert!(
        restored
            .file_source(&citation.id)
            .unwrap()
            .text
            .contains("4343")
    );
    let files = restored.project_files(&new_folder.id).unwrap();
    assert_eq!(
        std::fs::read(restored.managed_file_path(&files[0].id).unwrap()).unwrap(),
        b"The test code is 4343."
    );
    restored
        .restore_backup(Backup::from_bytes(&bytes).unwrap())
        .unwrap();
    assert_eq!(restored.chats().unwrap().len(), 2);
    assert!(
        store
            .export_chat_markdown(&chat.id)
            .unwrap()
            .contains("Attached files: note.txt")
    );
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    invalid["files"][0]["name"] = "../escape".into();
    assert!(Backup::from_bytes(&serde_json::to_vec(&invalid).unwrap()).is_err());
}
#[test]
fn linked_directories_and_settings_are_not_in_portable_data() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("db")).unwrap();
    let folder = store.create_folder("References").unwrap();
    let external = tempfile::tempdir().unwrap();
    store.set_project_link(&folder.id, external.path()).unwrap();
    let mut settings = store.settings().unwrap();
    settings.system_prompt = "synthetic settings excluded".into();
    store.save_settings(&settings).unwrap();
    let backup = store
        .backup(None, |_| panic!("Linked files must not be read by backup"))
        .unwrap()
        .bytes()
        .unwrap();
    let text = String::from_utf8(backup).unwrap();
    assert!(!text.contains("synthetic settings excluded"));
    assert!(!text.contains(external.path().to_str().unwrap()));
    assert!(external.path().exists());
}
