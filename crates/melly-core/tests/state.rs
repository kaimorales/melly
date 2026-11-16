use melly_core::{models::Settings, storage::Store};

#[test]
fn folder_scope_cannot_cross_boundaries_even_with_matching_query() {
    let db = Store::memory().unwrap();
    let a = db.create_folder("Job search").unwrap();
    let b = db.create_folder("Private").unwrap();
    let active = db.create_chat(Some(&a.id)).unwrap();
    let allowed = db.create_chat(Some(&a.id)).unwrap();
    let denied = db.create_chat(Some(&b.id)).unwrap();
    db.insert_message(
        &allowed.id,
        "user",
        "Transit project improved accuracy by 18%",
        "complete",
        None,
        None,
        None,
    )
    .unwrap();
    db.insert_message(
        &denied.id,
        "user",
        "Transit secret forbidden value 999",
        "complete",
        None,
        None,
        None,
    )
    .unwrap();
    db.insert_message(
        &active.id,
        "user",
        "Transit current conversation excluded",
        "complete",
        None,
        None,
        None,
    )
    .unwrap();
    let found = db.search_folder(&active.id, "Transit", 50).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].chat_id.as_deref(), Some(allowed.id.as_str()));
    assert!(!found[0].snippet.contains("999"));
    db.move_chat(&allowed.id, Some(&b.id)).unwrap();
    assert!(
        db.search_folder(&active.id, "Transit", 5)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn unfiled_chat_has_no_implicit_global_memory() {
    let db = Store::memory().unwrap();
    let c = db.create_chat(None).unwrap();
    assert!(db.search_folder(&c.id, "anything", 5).is_err());
}

#[test]
fn deleting_a_chat_removes_messages_and_both_search_results() {
    let db = Store::memory().unwrap();
    let folder = db.create_folder("Synthetic research").unwrap();
    let reader = db.create_chat(Some(&folder.id)).unwrap();
    let source = db.create_chat(Some(&folder.id)).unwrap();
    db.rename_chat(&source.id, "Transit notes").unwrap();
    let message = db
        .insert_message(
            &source.id,
            "user",
            "Transit forecast improved by 18%",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(db.search_folder(&reader.id, "Transit", 5).unwrap().len(), 1);
    assert_eq!(db.search_chats("Transit").unwrap().len(), 1);
    db.delete_chat(&source.id).unwrap();
    assert!(db.chat(&source.id).is_err());
    assert!(db.message(&message.id).is_err());
    assert!(
        db.search_folder(&reader.id, "Transit", 5)
            .unwrap()
            .is_empty()
    );
    assert!(db.search_chats("Transit").unwrap().is_empty());
    assert!(db.chat(&reader.id).is_ok());
}
#[test]
fn search_is_literal_bounded_and_sources_open_exact_messages() {
    let db = Store::memory().unwrap();
    let f = db.create_folder("Jobs").unwrap();
    let active = db.create_chat(Some(&f.id)).unwrap();
    for i in 0..10 {
        let c = db.create_chat(Some(&f.id)).unwrap();
        db.rename_chat(&c.id, &format!("Example {i}")).unwrap();
        db.insert_message(
            &c.id,
            "user",
            &format!("project {}", "行".repeat(2000)),
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    }
    let found = db.search_folder(&active.id, "project", 100).unwrap();
    assert_eq!(found.len(), 5);
    for s in found {
        assert!(s.snippet.chars().count() <= 600);
        assert!(s.title.starts_with("Example"));
        let m = db.message(s.message_id.as_ref().unwrap()).unwrap();
        assert_eq!(Some(m.chat_id), s.chat_id);
    }
    assert!(db.search_folder(&active.id, "\" OR * --", 5).is_ok());
}
#[test]
fn global_chat_search_matches_titles_and_messages_and_escapes_wildcards() {
    let db = Store::memory().unwrap();
    let titled = db.create_chat(None).unwrap();
    db.rename_chat(&titled.id, "Interview prep_100%!").unwrap();
    let other = db.create_chat(None).unwrap();
    db.rename_chat(&other.id, "Unrelated").unwrap();
    db.insert_message(
        &other.id,
        "user",
        "Synthetic forecast result",
        "complete",
        None,
        None,
        None,
    )
    .unwrap();
    for query in ["Interview prep", "_", "%", "!"] {
        let results = db.search_chats(query).unwrap();
        assert_eq!(results.len(), 1, "literal query {query}");
        assert_eq!(results[0].id, titled.id);
    }
    assert_eq!(db.search_chats("forecast").unwrap()[0].id, other.id);
    assert!(db.search_chats("' OR 1=1 --").unwrap().is_empty());
    assert!(db.search_chats(&"a".repeat(501)).is_err());
}

#[test]
fn history_persists_and_pages_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat.db");
    let id;
    {
        let db = Store::open(&path).unwrap();
        let c = db.create_chat(None).unwrap();
        id = c.id;
        db.rename_chat(&id, "Persistent").unwrap();
        for i in 0..80 {
            db.insert_message(
                &id,
                "user",
                &format!("Message {i}"),
                "complete",
                None,
                None,
                None,
            )
            .unwrap();
        }
    }
    let db = Store::open(&path).unwrap();
    let last = db.messages(&id, None).unwrap();
    assert_eq!(last.len(), 60);
    let older = db.messages(&id, Some(last[0].created_at)).unwrap();
    assert_eq!(older.len(), 20);
    assert!(older.last().unwrap().created_at < last[0].created_at);
    assert_eq!(db.chat(&id).unwrap().title, "Persistent");
}
#[test]
fn settings_are_validated_and_interrupted_runs_recover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let mid;
    {
        let db = Store::open(&path).unwrap();
        let c = db.create_chat(None).unwrap();
        mid = db
            .insert_message(&c.id, "assistant", "partial", "streaming", None, None, None)
            .unwrap()
            .id;
        let mut s = Settings {
            theme: "invalid".into(),
            ..Settings::default()
        };
        assert!(db.save_settings(&s).is_err());
        s.theme = "light".into();
        db.save_settings(&s).unwrap();
    }
    let db = Store::open(&path).unwrap();
    assert_eq!(db.message(&mid).unwrap().status, "interrupted");
    assert_eq!(db.settings().unwrap().theme, "light");
}

#[test]
fn reasoning_survives_reopen_without_entering_search_or_context() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("history.sqlite3");
    let (chat, message) = {
        let store = Store::open(&path).unwrap();
        let chat = store.create_chat(None).unwrap();
        let message = store
            .insert_message(
                &chat.id,
                "assistant",
                "",
                "streaming",
                Some("ollama-local"),
                Some("example"),
                Some("Local"),
            )
            .unwrap();
        store
            .finish_message_with_reasoning(
                &message.id,
                "A visible answer",
                "reasoning_canary",
                "complete",
                &[],
            )
            .unwrap();
        (chat.id, message.id)
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.message(&message).unwrap().reasoning,
        "reasoning_canary"
    );
    assert_eq!(store.context(&chat).unwrap()[0].content, "A visible answer");
    assert!(store.search_chats("reasoning_canary").unwrap().is_empty());
}

#[test]
fn branching_preserves_the_original_and_respects_the_selected_boundary() {
    let store = melly_core::storage::Store::memory().unwrap();
    let original = store.create_chat(None).unwrap();
    let first = store
        .insert_message(
            &original.id,
            "user",
            "First question",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    let answer = store
        .insert_message(
            &original.id,
            "assistant",
            "First answer",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .finish_message_with_reasoning(
            &answer.id,
            "First answer",
            "Separate thought",
            "complete",
            &[],
        )
        .unwrap();
    let later = store
        .insert_message(
            &original.id,
            "user",
            "Second question",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    let branch = store.branch_chat(&original.id, &answer.id, true).unwrap();
    let copied = store.messages(&branch.id, None).unwrap();
    assert_eq!(copied.len(), 2);
    assert_ne!(copied[0].id, first.id);
    assert_eq!(copied[1].reasoning, "Separate thought");
    let edit = store.branch_chat(&original.id, &later.id, false).unwrap();
    assert_eq!(store.messages(&edit.id, None).unwrap().len(), 2);
    assert_eq!(store.messages(&original.id, None).unwrap().len(), 3);
    assert!(
        store
            .branch_chat(&original.id, &copied[0].id, true)
            .is_err()
    );
    assert!(store.branch_chat(&original.id, &answer.id, false).is_err());
}

#[test]
fn earlier_chat_references_find_old_facts_without_crossing_chat_scope() {
    let store = Store::memory().unwrap();
    let chat = store.create_chat(None).unwrap();
    let foreign = store.create_chat(None).unwrap();
    let old = store
        .insert_message(
            &chat.id,
            "user",
            "Synthetic project codename is Juniper",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .insert_message(
            &foreign.id,
            "user",
            "Synthetic project codename is Secret",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    for i in 0..65 {
        store
            .insert_message(
                &chat.id,
                if i % 2 == 0 { "user" } else { "assistant" },
                &format!("Unrelated synthetic sentence {i}"),
                "complete",
                None,
                None,
                None,
            )
            .unwrap();
    }
    assert!(
        !store
            .context(&chat.id)
            .unwrap()
            .iter()
            .any(|m| m.content.contains("Juniper"))
    );
    let refs = store
        .earlier_references(&chat.id, "project codename")
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].message_id, Some(old.id));
    assert!(!refs[0].snippet.contains("Secret"));
}
