use melly_core::storage::Store;
#[test]
fn files_survive_retry_branch_and_reopen_but_are_collected_after_last_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let store = Store::open(&path).unwrap();
    let chat = store.create_chat(None).unwrap();
    let file = store
        .save_draft_attachment(
            "example.txt",
            "text",
            "The sample code is 4343.",
            b"The sample code is 4343.",
        )
        .unwrap();
    let user = store
        .insert_message(
            &chat.id,
            "user",
            "What is the sample code?",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .attach_to_message(&user.id, std::slice::from_ref(&file.id))
        .unwrap();
    assert_eq!(
        store.message(&user.id).unwrap().attachments[0].name,
        "example.txt"
    );
    assert!(
        store
            .validate_attachment_ids(std::slice::from_ref(&file.id))
            .is_err()
    );
    assert!(store.context(&chat.id).unwrap()[0].content.contains("4343"));
    store
        .insert_message(&chat.id, "assistant", "4343", "complete", None, None, None)
        .unwrap();
    store.retry_user_message(&chat.id).unwrap();
    assert!(store.context(&chat.id).unwrap()[0].content.contains("4343"));
    let branch = store.branch_chat(&chat.id, &user.id, true).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    store.delete_chat(&chat.id).unwrap();
    assert!(
        store.context(&branch.id).unwrap()[0]
            .content
            .contains("4343")
    );
    store.delete_chat(&branch.id).unwrap();
    assert!(store.attachment_content(&file.id).is_err());
}
#[test]
fn drafts_are_bounded_validated_and_never_shared_by_other_chats() {
    let store = Store::memory().unwrap();
    let first = store.create_chat(None).unwrap();
    let second = store.create_chat(None).unwrap();
    let file = store
        .save_draft_attachment(
            "reference.txt",
            "text",
            "synthetic reference",
            b"synthetic reference",
        )
        .unwrap();
    assert!(store.context(&second.id).unwrap().is_empty());
    assert!(
        store
            .validate_attachment_ids(&[file.id.clone(), file.id.clone()])
            .is_err()
    );
    assert!(
        store
            .save_draft_attachment("../escape", "text", "x", b"x")
            .is_err()
    );
    let message = store
        .insert_message(
            &first.id,
            "user",
            "Use the reference",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .attach_to_message(&message.id, std::slice::from_ref(&file.id))
        .unwrap();
    store.discard_draft_attachment(&file.id).unwrap();
    assert!(store.attachment_content(&file.id).is_ok());
    for _ in 0..8 {
        store
            .save_draft_attachment("a.txt", "text", "x", b"x")
            .unwrap();
    }
    assert!(
        store
            .save_draft_attachment("b.txt", "text", "x", b"x")
            .is_err()
    );
}

#[test]
fn composer_attachments_survive_restart_until_the_draft_is_discarded() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("drafts.db");
    let store = melly_core::storage::Store::open(&path)?;
    let file = store.save_draft_attachment(
        "synthetic.txt",
        "text",
        "Synthetic attachment",
        b"Synthetic attachment",
    )?;
    store.save_composer_draft(
        None,
        &melly_core::drafts::Draft {
            content: "Unsent".into(),
            attachment_ids: vec![file.id.clone()],
            editing_message_id: None,
        },
    )?;
    drop(store);
    let store = melly_core::storage::Store::open(&path)?;
    assert_eq!(store.composer_draft(None)?.content, "Unsent");
    assert_eq!(
        store
            .validate_attachment_ids(std::slice::from_ref(&file.id))?
            .len(),
        1
    );
    store.save_composer_draft(None, &Default::default())?;
    store.discard_draft_attachment(&file.id)?;
    assert!(store.attachment_content(&file.id).is_err());
    Ok(())
}
