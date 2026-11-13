use async_trait::async_trait;
use melly_core::{
    credentials::MemorySecrets,
    engine::{Engine, Transport},
    models::*,
    storage::Store,
};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[test]
fn bootstrap_never_decrypts_credentials_or_requests_keychain_authorization() {
    struct MetadataOnly;
    impl melly_core::credentials::SecretStore for MetadataOnly {
        fn get(&self, _: &str) -> anyhow::Result<Option<String>> {
            panic!("Startup must not read secret bytes")
        }
        fn contains(&self, _: &str) -> anyhow::Result<bool> {
            Ok(false)
        }
        fn set(&self, _: &str, _: &str) -> anyhow::Result<()> {
            panic!("No startup writes")
        }
        fn delete(&self, _: &str) -> anyhow::Result<()> {
            panic!("No startup writes")
        }
    }
    let e = Engine::new(
        Arc::new(Mutex::new(Store::memory().unwrap())),
        Arc::new(MetadataOnly),
    );
    let state = e.bootstrap().unwrap();
    assert!(
        state
            .providers
            .iter()
            .find(|p| p.spec.id == "mock")
            .unwrap()
            .configured
    );
    assert!(
        state
            .providers
            .iter()
            .filter(|p| p.spec.id != "mock")
            .all(|p| !p.configured)
    );
}

#[test]
fn unavailable_vault_does_not_block_history_or_demo() {
    struct Unavailable;
    impl melly_core::credentials::SecretStore for Unavailable {
        fn get(&self, _: &str) -> anyhow::Result<Option<String>> {
            panic!("No secret reads")
        }
        fn contains(&self, _: &str) -> anyhow::Result<bool> {
            anyhow::bail!("Unavailable")
        }
        fn set(&self, _: &str, _: &str) -> anyhow::Result<()> {
            panic!("No writes")
        }
        fn delete(&self, _: &str) -> anyhow::Result<()> {
            panic!("No deletes")
        }
    }
    let store = Store::memory().unwrap();
    let chat = store.create_chat(None).unwrap();
    let engine = Engine::new(Arc::new(Mutex::new(store)), Arc::new(Unavailable));
    let state = engine.bootstrap().unwrap();
    assert!(state.chats.iter().any(|item| item.id == chat.id));
    assert!(
        state
            .providers
            .iter()
            .find(|item| item.spec.id == "mock")
            .unwrap()
            .configured
    );
    assert!(
        !state
            .providers
            .iter()
            .find(|item| item.spec.id == "claude")
            .unwrap()
            .configured
    );
    assert!(!state.search_configured);
}

struct Fake {
    mode: &'static str,
    calls: Mutex<Vec<ProviderRequest>>,
}
#[async_trait]
impl Transport for Fake {
    async fn stream(
        &self,
        request: ProviderRequest,
        _: Option<String>,
        sink: ProviderSink,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let second = request.messages.iter().any(|m| m.role == "tool");
        self.calls.lock().unwrap().push(request);
        match self.mode {
            "oversize" => {
                sink(ProviderEvent::Delta("x".repeat(1_048_577)));
                cancel.cancelled().await;
                return Ok(());
            }
            "slow" => {
                for _ in 0..100 {
                    tokio::select! {_ = cancel.cancelled()=>return Ok(()), _=tokio::time::sleep(std::time::Duration::from_millis(15))=>sink(ProviderEvent::Delta("word ".into()))}
                }
            }
            "broken" => {
                sink(ProviderEvent::Delta("Partial answer".into()));
                return Ok(());
            }
            "malicious" if !second => sink(ProviderEvent::ToolCall(ToolCall {
                id: "call1".into(),
                name: "search_folder".into(),
                arguments: json!({"query":"CERULEAN","folderId":"other"}),
                provider_data: None,
            })),
            "folder" if !second => sink(ProviderEvent::ToolCall(ToolCall {
                id: "call1".into(),
                name: "search_folder".into(),
                arguments: json!({"query":"forecast"}),
                provider_data: None,
            })),
            "web" | "web-failure" if !second => sink(ProviderEvent::ToolCall(ToolCall {
                id: "call1".into(),
                name: "search_web".into(),
                arguments: json!({"query":"transit"}),
                provider_data: None,
            })),
            _ => sink(ProviderEvent::Delta("A grounded answer.".into())),
        }
        sink(ProviderEvent::Done);
        Ok(())
    }
    async fn web_search(
        &self,
        _: &str,
        _: &str,
        _: CancellationToken,
    ) -> anyhow::Result<Vec<Source>> {
        if self.mode == "web-failure" {
            anyhow::bail!("Search request limit reached (HTTP 429)");
        }
        panic!("Mock must not make live web requests")
    }
}
fn fixture(mode: &'static str) -> (Arc<Engine>, Arc<Fake>, String) {
    let s = Store::memory().unwrap();
    s.seed_demo().unwrap();
    let c = s
        .chats()
        .unwrap()
        .into_iter()
        .find(|c| c.title == "Interview prep")
        .unwrap()
        .id;
    let fake = Arc::new(Fake {
        mode,
        calls: Mutex::new(vec![]),
    });
    let e = Engine::with_transport(
        Arc::new(Mutex::new(s)),
        Arc::new(MemorySecrets::default()),
        fake.clone(),
    );
    (Arc::new(e), fake, c)
}
fn input(chat: &str) -> SendInput {
    SendInput {
        chat_id: chat.into(),
        content: "Look in my other chats for the forecast result".into(),
        provider_id: "mock".into(),
        model_id: "melly-demo".into(),
        web_search: false,
        folder_context: true,
        retry: false,
    }
}
async fn finish(e: &Engine, chat: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while e.is_active(chat) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn failed_external_search_is_visible_and_cannot_finish_as_a_successful_reply() {
    let (engine, transport, chat) = fixture("web-failure");
    let connection = engine.connection("deepseek").unwrap();
    engine
        .vault
        .set(
            &melly_core::providers::credential_id(&connection),
            "synthetic-provider-key",
        )
        .unwrap();
    engine
        .vault
        .set(melly_core::engine::SEARCH_KEY_ID, "synthetic-search-key")
        .unwrap();
    let mut request = input(&chat);
    request.provider_id = connection.provider_id;
    request.model_id = connection.model_id;
    request.folder_context = false;
    request.web_search = true;
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let reply = engine
        .start(
            request,
            Arc::new(move |event| observed.lock().unwrap().push(event)),
        )
        .unwrap();
    finish(&engine, &chat).await;
    assert_eq!(
        engine
            .store
            .lock()
            .unwrap()
            .message(&reply.id)
            .unwrap()
            .status,
        "error"
    );
    assert_eq!(
        transport.calls.lock().unwrap().len(),
        1,
        "A failed search must not continue as though reference material was retrieved"
    );
    let events = events.lock().unwrap();
    let failure = events.iter().find(|event| event.kind == "error").unwrap();
    assert!(failure.text.contains("HTTP 429"));
    assert!(!failure.text.contains("synthetic-"));
    assert!(!events.iter().any(|event| event.kind == "done"));
}
#[tokio::test]
async fn oversized_reply_cancels_transport_promptly_as_error() {
    let (e, _, c) = fixture("oversize");
    let t = std::time::Instant::now();
    let m = e.start(input(&c), Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    assert!(t.elapsed() < std::time::Duration::from_millis(150));
    assert_eq!(
        e.store.lock().unwrap().message(&m.id).unwrap().status,
        "error"
    );
}
#[tokio::test]
async fn folder_tool_sends_only_bounded_current_folder_snippets() {
    let (e, f, c) = fixture("folder");
    let m = e.start(input(&c), Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    let calls = f.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let tool = calls[1].messages.iter().find(|m| m.role == "tool").unwrap();
    assert!(tool.content.contains("18%"));
    assert!(!tool.content.contains("CERULEAN"));
    assert!(tool.content.contains("untrusted"));
    let stored = e.store.lock().unwrap().message(&m.id).unwrap();
    assert_eq!(stored.status, "complete");
    assert_eq!(stored.sources[0].title, "My experience");
    assert!(stored.sources[0].message_id.is_some());
}
#[tokio::test]
async fn malicious_scope_argument_never_reaches_storage() {
    let (e, f, c) = fixture("malicious");
    let m = e.start(input(&c), Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    let calls = f.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let tool = calls[1].messages.iter().find(|m| m.role == "tool").unwrap();
    assert!(!tool.content.contains("CERULEAN"));
    assert!(tool.content.contains("denied"));
    assert!(
        e.store
            .lock()
            .unwrap()
            .message(&m.id)
            .unwrap()
            .sources
            .is_empty()
    );
}
#[tokio::test]
async fn folder_tool_requires_explicit_turn_permission() {
    let (e, f, c) = fixture("folder");
    let mut i = input(&c);
    i.folder_context = false;
    e.start(i, Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    let calls = f.calls.lock().unwrap();
    assert!(calls[0].tools.is_empty());
    let tool = calls[1].messages.iter().find(|m| m.role == "tool").unwrap();
    assert!(tool.content.contains("denied"));
    assert!(!tool.content.contains("18%"));
}
#[tokio::test]
async fn stop_is_prompt_preserves_partial_and_emits_no_late_delta() {
    let (e, _, c) = fixture("slow");
    let events = Arc::new(Mutex::new(vec![]));
    let out = events.clone();
    let m = e
        .start(input(&c), Arc::new(move |ev| out.lock().unwrap().push(ev)))
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(65)).await;
    let time = std::time::Instant::now();
    e.stop(&c);
    finish(&e, &c).await;
    assert!(time.elapsed() < std::time::Duration::from_millis(150));
    let n = events.lock().unwrap().len();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(events.lock().unwrap().len(), n);
    let stored = e.store.lock().unwrap().message(&m.id).unwrap();
    assert_eq!(stored.status, "stopped");
    assert!(!stored.content.is_empty());
    assert!(e.start(input(&c), Arc::new(|_| {})).is_ok());
    e.stop(&c);
    finish(&e, &c).await;
}
#[tokio::test]
async fn truncated_stream_is_error_not_success_and_retry_does_not_duplicate_user() {
    let (e, _, c) = fixture("broken");
    let m = e.start(input(&c), Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    assert_eq!(
        e.store.lock().unwrap().message(&m.id).unwrap().status,
        "error"
    );
    let mut i = input(&c);
    i.retry = true;
    let next = e.start(i, Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    let s = e.store.lock().unwrap();
    assert_eq!(
        s.messages(&c, None)
            .unwrap()
            .iter()
            .filter(|m| m.role == "user")
            .count(),
        1
    );
    assert_eq!(s.message(&next.id).unwrap().content, "Partial answer");
}
#[tokio::test]
async fn disabled_host_and_second_stream_are_rejected() {
    let (e, f, c) = fixture("slow");
    let mut i = input(&c);
    i.provider_id = "muse".into();
    assert!(e.start(i, Arc::new(|_| {})).is_err());
    assert!(f.calls.lock().unwrap().is_empty());
    e.start(input(&c), Arc::new(|_| {})).unwrap();
    assert!(e.start(input(&c), Arc::new(|_| {})).is_err());
    e.stop(&c);
    finish(&e, &c).await;
}
#[tokio::test]
async fn native_search_cannot_use_separately_billed_fallback_tool() {
    let (e, f, c) = fixture("web");
    let conn = e.connection("grok").unwrap();
    e.vault
        .set(
            &melly_core::providers::credential_id(&conn),
            "synthetic-test-credential",
        )
        .unwrap();
    e.vault
        .set(
            melly_core::engine::SEARCH_KEY_ID,
            "synthetic-search-credential",
        )
        .unwrap();
    let mut i = input(&c);
    i.provider_id = "grok".into();
    i.model_id = conn.model_id;
    i.web_search = true;
    i.folder_context = false;
    e.start(i, Arc::new(|_| {})).unwrap();
    finish(&e, &c).await;
    let calls = f.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls[0].web_search);
    assert!(calls[0].tools.is_empty());
    assert!(
        calls[1]
            .messages
            .iter()
            .find(|m| m.role == "tool")
            .unwrap()
            .content
            .contains("denied")
    );
}
#[tokio::test]
async fn active_chat_cannot_move_or_be_deleted_during_a_turn() {
    let (e, _, c) = fixture("slow");
    e.start(input(&c), Arc::new(|_| {})).unwrap();
    assert!(e.move_chat(&c, None).is_err());
    assert!(e.delete_chat(&c).is_err());
    e.stop(&c);
    finish(&e, &c).await;
    assert!(e.move_chat(&c, None).is_ok());
}
