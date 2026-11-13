//! Exercises the real Rust coordinator + provider Mock + SQLite, not the browser shim.
use melly_core::{credentials::MemorySecrets, engine::Engine, models::*, storage::Store};
use std::sync::{Arc, Mutex};
async fn wait(e: &Engine, c: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while e.is_active(c) {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
fn input(c: &str, text: &str, folder: bool, web: bool) -> SendInput {
    SendInput {
        chat_id: c.into(),
        content: text.into(),
        provider_id: "mock".into(),
        model_id: "melly-demo".into(),
        folder_context: folder,
        web_search: web,
        retry: false,
    }
}
#[tokio::test]
async fn full_rust_demo_retrieves_fact_opens_exact_source_and_keeps_web_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("demo.db");
    let store = Store::open(&path).unwrap();
    let folder = store.create_folder("Job search").unwrap();
    let first = store.create_chat(Some(&folder.id)).unwrap();
    store.rename_chat(&first.id, "My experience").unwrap();
    let second = store.create_chat(Some(&folder.id)).unwrap();
    store.rename_chat(&second.id, "Interview prep").unwrap();
    let other = store.create_folder("Private").unwrap();
    let outsider = store.create_chat(Some(&other.id)).unwrap();
    store
        .insert_message(
            &outsider.id,
            "user",
            "The private project result is CERULEAN-999",
            "complete",
            None,
            None,
            None,
        )
        .unwrap();
    let engine = Arc::new(Engine::new(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
    ));
    engine
        .start(
            input(
                &first.id,
                "My transit project improved forecast accuracy by 18%.",
                false,
                false,
            ),
            Arc::new(|_| {}),
        )
        .unwrap();
    wait(&engine, &first.id).await;
    let reply = engine
        .start(
            input(
                &second.id,
                "Look in my other Job search chats for the project result",
                true,
                false,
            ),
            Arc::new(|_| {}),
        )
        .unwrap();
    wait(&engine, &second.id).await;
    {
        let s = engine.store.lock().unwrap();
        let message = s.message(&reply.id).unwrap();
        assert_eq!(message.status, "complete");
        assert!(message.content.contains("18%"));
        assert!(!message.content.contains("CERULEAN"));
        let source = message.sources.iter().find(|s| s.kind == "chat").unwrap();
        assert_eq!(source.chat_id.as_deref(), Some(first.id.as_str()));
        let cited = s.message(source.message_id.as_deref().unwrap()).unwrap();
        assert!(cited.content.contains("18%"));
    }
    let web = engine
        .start(
            input(
                &second.id,
                "Search the web for transit planning",
                false,
                true,
            ),
            Arc::new(|_| {}),
        )
        .unwrap();
    wait(&engine, &second.id).await;
    {
        let s = engine.store.lock().unwrap();
        let msg = s.message(&web.id).unwrap();
        assert_eq!(msg.status, "complete");
        assert!(msg.content.to_lowercase().contains("mock"));
        assert!(msg.sources.iter().any(|s| s.kind == "web"));
        assert!(msg.sources.iter().all(|s| s.kind != "chat"));
    }
    let user_count = engine
        .store
        .lock()
        .unwrap()
        .messages(&second.id, None)
        .unwrap()
        .iter()
        .filter(|m| m.role == "user")
        .count();
    drop(engine);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened
            .messages(&second.id, None)
            .unwrap()
            .iter()
            .filter(|m| m.role == "user")
            .count(),
        user_count
    );
    let bytes = std::fs::read(&path).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("access_token"));
    assert!(!text.contains("refresh_token"));
}
#[tokio::test]
async fn real_mock_stop_preserves_a_partial_reply() {
    let s = Store::memory().unwrap();
    let c = s.create_chat(None).unwrap();
    let engine = Arc::new(Engine::new(
        Arc::new(Mutex::new(s)),
        Arc::new(MemorySecrets::default()),
    ));
    let m = engine
        .start(
            input(&c.id, "Demonstrate streaming and Stop", false, false),
            Arc::new(|_| {}),
        )
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let t = std::time::Instant::now();
    engine.stop(&c.id);
    wait(&engine, &c.id).await;
    assert!(t.elapsed() < std::time::Duration::from_millis(150));
    let saved = engine.store.lock().unwrap().message(&m.id).unwrap();
    assert_eq!(saved.status, "stopped");
    assert!(!saved.content.is_empty());
}
