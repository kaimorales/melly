use melly_core::storage::Store;

#[test]
fn new_chat_reuses_only_empty_chats_in_the_requested_location() {
    let store = Store::memory().unwrap();
    let first = store.open_new_chat(None, None).unwrap();
    assert_eq!(store.open_new_chat(None, None).unwrap().id, first.id);
    store
        .insert_message(
            &first.id,
            "user",
            "A synthetic message",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    let empty = store.open_new_chat(None, Some(&first.id)).unwrap();
    assert_ne!(empty.id, first.id);
    store.rename_chat(&empty.id, "Named draft").unwrap();
    assert_eq!(
        store.open_new_chat(None, Some(&empty.id)).unwrap().id,
        empty.id
    );
    assert_ne!(store.open_new_chat(None, None).unwrap().id, empty.id);
}
