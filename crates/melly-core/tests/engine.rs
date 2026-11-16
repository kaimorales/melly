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
        let round = {
            let mut calls = self.calls.lock().unwrap();
            let round = calls.len();
            calls.push(request);
            round
        };
        match self.mode {
            "usage" | "usage-stop" => {
                sink(ProviderEvent::Usage(melly_core::usage::Usage {
                    input: Some(25),
                    output: Some(1),
                    ..Default::default()
                }));
                sink(ProviderEvent::Usage(melly_core::usage::Usage {
                    output: Some(5),
                    ..Default::default()
                }));
                sink(ProviderEvent::Delta("Synthetic answer".into()));
                if self.mode == "usage-stop" {
                    cancel.cancelled().await;
                    return Ok(());
                }
            }
            "reasoning" | "reasoning-stop" | "reasoning-error" => {
                sink(ProviderEvent::Reasoning(
                    "Private-display-canary: compare the alternatives.".into(),
                ));
                if self.mode == "reasoning-stop" {
                    cancel.cancelled().await;
                    sink(ProviderEvent::Reasoning("LATE_CANARY".into()));
                    return Ok(());
                }
                if self.mode == "reasoning-error" {
                    anyhow::bail!("Synthetic interrupted inference");
                }
                sink(ProviderEvent::Delta("A concise answer.".into()));
            }
            "claude-pause-stoppable" if round > 0 => {
                cancel.cancelled().await;
                return Ok(());
            }
            "claude-pause" if round < 2 => {
                sink(ProviderEvent::ClaudeSearchPaused(ClaudeContinuation {
                    blocks: vec![
                        json!({"type":"server_tool_use","id":format!("srvtoolu_{round}"),"name":"web_search","input":{"query":"synthetic private marker"}}),
                    ],
                }));
            }
            "claude-pause-forever" | "claude-pause-stoppable" | "unexpected-pause" => {
                sink(ProviderEvent::ClaudeSearchPaused(ClaudeContinuation {
                    blocks: vec![
                        json!({"type":"server_tool_use","id":"srvtoolu_demo","name":"web_search","input":{"query":"synthetic private marker"}}),
                    ],
                }));
            }
            "create-file" => sink(ProviderEvent::Delta(
                "Proposed a new file.\n\n```melly-file-edit\n{\"path\":\"example.py\",\"content\":\"print(42)\\n\"}\n```".into(),
            )),
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
        query: &str,
        key: &str,
        _: CancellationToken,
    ) -> anyhow::Result<Vec<Source>> {
        if self.mode == "muse-api" {
            assert_eq!(query, "Find a source for my question");
            assert_eq!(key, "synthetic-search-key");
            return Ok(vec![Source {
                id: "web:example".into(),
                kind: "web".into(),
                url: Some("https://example.com/source".into()),
                title: "Synthetic reference".into(),
                snippet: "Search result text".into(),
                ..Default::default()
            }]);
        }
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

fn claude_request(engine: &Engine, chat: &str, web_search: bool) -> SendInput {
    let connection = engine.connection("claude").unwrap();
    engine
        .vault
        .set(
            &melly_core::providers::credential_id(&connection),
            "synthetic-provider-key",
        )
        .unwrap();
    let mut request = input(chat);
    request.provider_id = connection.provider_id;
    request.model_id = connection.model_id;
    request.web_search = web_search;
    request.folder_context = false;
    request
}

#[tokio::test]
async fn claude_paused_search_continues_with_the_same_route_and_without_persisting_blocks() {
    let (engine, transport, chat) = fixture("claude-pause");
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let reply = engine
        .start(
            claude_request(&engine, &chat, true),
            Arc::new(move |event| observed.lock().unwrap().push(event)),
        )
        .unwrap();
    finish(&engine, &chat).await;
    let calls = transport.calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        3,
        "Two pauses must resume twice before the final answer"
    );
    assert!(
        calls
            .iter()
            .all(|call| call.connection.provider_id == "claude" && call.web_search)
    );
    assert!(calls.iter().all(
        |call| call.connection.base_url == calls[0].connection.base_url
            && call.connection.model_id == calls[0].connection.model_id
    ));
    assert_eq!(
        calls[1]
            .messages
            .iter()
            .filter(|m| m.claude_continuation.is_some())
            .count(),
        1
    );
    let continuations: Vec<_> = calls[2]
        .messages
        .iter()
        .filter_map(|m| m.claude_continuation.as_ref())
        .collect();
    assert_eq!(continuations.len(), 2);
    assert_eq!(continuations[0].blocks[0]["id"], "srvtoolu_0");
    assert_eq!(continuations[1].blocks[0]["id"], "srvtoolu_1");
    assert!(
        !serde_json::to_string(&calls[2].messages)
            .unwrap()
            .contains("synthetic private marker")
    );
    assert!(!format!("{:?}", calls[2]).contains("synthetic private marker"));
    let saved = engine.store.lock().unwrap().message(&reply.id).unwrap();
    assert_eq!(saved.status, "complete");
    assert!(
        !serde_json::to_string(&saved)
            .unwrap()
            .contains("synthetic private marker")
    );
    assert!(
        !serde_json::to_string(&*events.lock().unwrap())
            .unwrap()
            .contains("synthetic private marker")
    );
}

#[tokio::test]
async fn stopping_a_continued_claude_search_cancels_the_active_stream() {
    let (engine, transport, chat) = fixture("claude-pause-stoppable");
    let reply = engine
        .start(claude_request(&engine, &chat, true), Arc::new(|_| {}))
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while transport.calls.lock().unwrap().len() < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    engine.stop(&chat);
    finish(&engine, &chat).await;
    assert_eq!(transport.calls.lock().unwrap().len(), 2);
    assert_eq!(
        engine
            .store
            .lock()
            .unwrap()
            .message(&reply.id)
            .unwrap()
            .status,
        "stopped"
    );
}

#[tokio::test]
async fn claude_paused_search_has_a_bounded_number_of_rounds() {
    let (engine, transport, chat) = fixture("claude-pause-forever");
    let reply = engine
        .start(claude_request(&engine, &chat, true), Arc::new(|_| {}))
        .unwrap();
    finish(&engine, &chat).await;
    assert_eq!(transport.calls.lock().unwrap().len(), 4);
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
}

#[tokio::test]
async fn unexpected_claude_search_pause_does_not_start_an_unauthorized_search() {
    let (engine, transport, chat) = fixture("unexpected-pause");
    let reply = engine
        .start(claude_request(&engine, &chat, false), Arc::new(|_| {}))
        .unwrap();
    finish(&engine, &chat).await;
    assert_eq!(transport.calls.lock().unwrap().len(), 1);
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
    let store = e.store.lock().unwrap();
    let folder = store.chat(&c).unwrap().folder_id;
    for source in store.message(&m.id).unwrap().sources {
        assert!(!source.snippet.contains("CERULEAN"));
        assert_eq!(
            store
                .chat(source.chat_id.as_deref().unwrap())
                .unwrap()
                .folder_id,
            folder
        );
    }
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
async fn retired_muse_plan_and_second_stream_are_rejected() {
    let (e, f, c) = fixture("slow");
    let api = e.connection("muse-api").unwrap();
    e.vault
        .set(
            &melly_core::providers::credential_id(&api),
            "synthetic-meta-key",
        )
        .unwrap();
    e.store
        .lock()
        .unwrap()
        .save_settings(&Settings {
            default_provider: "muse".into(),
            default_model: "host-default".into(),
            ..Settings::default()
        })
        .unwrap();
    let state = e.bootstrap().unwrap();
    assert!(state.settings.default_provider.is_empty());
    assert!(state.settings.default_model.is_empty());
    assert!(
        state
            .providers
            .iter()
            .any(|provider| provider.spec.id == "muse-api" && provider.configured)
    );
    let mut i = input(&c);
    i.provider_id = "muse".into();
    i.model_id = "host-default".into();
    i.folder_context = false;
    assert!(e.start(i, Arc::new(|_| {})).is_err());
    assert!(f.calls.lock().unwrap().is_empty());
    e.start(input(&c), Arc::new(|_| {})).unwrap();
    assert!(e.start(input(&c), Arc::new(|_| {})).is_err());
    e.stop(&c);
    finish(&e, &c).await;
}

#[tokio::test]
async fn muse_api_search_uses_only_the_typed_query_and_passes_text_without_tools() {
    let (engine, fake, chat) = fixture("muse-api");
    let connection = engine.connection("muse-api").unwrap();
    engine
        .vault
        .set(
            &melly_core::providers::credential_id(&connection),
            "synthetic-meta-key",
        )
        .unwrap();
    engine
        .vault
        .set(melly_core::engine::SEARCH_KEY_ID, "synthetic-search-key")
        .unwrap();
    let mut turn = input(&chat);
    turn.content = "Find a source for my question".into();
    turn.provider_id = connection.provider_id;
    turn.model_id = connection.model_id;
    turn.folder_context = false;
    turn.web_search = true;
    let file = melly_core::engine::TextAttachment {
        name: "sample.txt".into(),
        text: "Synthetic file payload".into(),
    };
    let result = engine
        .start_with_attachment(turn, Some(&file), Arc::new(|_| {}))
        .unwrap();
    finish(&engine, &chat).await;
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let request = &calls[0];
    assert!(request.tools.is_empty() && !request.web_search);
    let user = &request.messages.last().unwrap().content;
    assert!(user.contains(&file.text) && user.contains("Search result text"));
    assert!(!user.contains("synthetic-search-key"));
    let store = engine.store.lock().unwrap();
    assert_eq!(
        store.message(&result.id).unwrap().sources[0].id,
        "web:example"
    );
    assert!(
        !store
            .context(&chat)
            .unwrap()
            .iter()
            .any(|message| message.content.contains(&file.text))
    );
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

#[tokio::test]
async fn project_context_reaches_a_provider_without_tool_calling() {
    let (engine, fake, chat) = fixture("muse-api");
    let connection = engine.connection("muse-api").unwrap();
    engine
        .vault
        .set(
            &melly_core::providers::credential_id(&connection),
            "synthetic-key",
        )
        .unwrap();
    let mut request = input(&chat);
    request.provider_id = connection.provider_id;
    request.model_id = connection.model_id;
    let reply = engine.start(request, Arc::new(|_| {})).unwrap();
    finish(&engine, &chat).await;
    let calls = fake.calls.lock().unwrap();
    assert!(calls[0].tools.is_empty());
    assert!(calls[0].messages.last().unwrap().content.contains("18%"));
    assert!(
        !calls[0]
            .messages
            .last()
            .unwrap()
            .content
            .contains("CERULEAN")
    );
    assert!(
        !engine
            .store
            .lock()
            .unwrap()
            .message(&reply.id)
            .unwrap()
            .sources
            .is_empty()
    );
}

#[tokio::test]
async fn reasoning_is_separate_persists_on_stop_and_never_reenters_context_or_search() {
    for mode in ["reasoning", "reasoning-stop", "reasoning-error"] {
        let (engine, _, chat) = fixture(mode);
        let updates = Arc::new(Mutex::new(Vec::new()));
        let capture = updates.clone();
        let mut request = input(&chat);
        request.folder_context = false;
        let message = engine
            .start(
                request,
                Arc::new(move |event| capture.lock().unwrap().push(event)),
            )
            .unwrap();
        if mode == "reasoning-stop" {
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while updates
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|event| event.kind != "reasoning")
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            engine.stop(&chat);
        }
        finish(&engine, &chat).await;
        let store = engine.store.lock().unwrap();
        let saved = store.message(&message.id).unwrap();
        assert!(saved.reasoning.contains("Private-display-canary"));
        assert!(!saved.reasoning.contains("LATE_CANARY"));
        assert!(!saved.content.contains("Private-display-canary"));
        assert_eq!(
            saved.status,
            match mode {
                "reasoning-stop" => "stopped",
                "reasoning-error" => "error",
                _ => "complete",
            }
        );
        assert!(
            store
                .context(&chat)
                .unwrap()
                .iter()
                .all(|message| !message.content.contains("Private-display-canary"))
        );
        assert!(
            store
                .search_chats("Private-display-canary")
                .unwrap()
                .is_empty()
        );
        assert!(
            updates
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event.kind == "delta")
                .all(|event| !event.text.contains("Private-display-canary"))
        );
    }
}

#[tokio::test]
async fn usage_is_admitted_before_inference_and_preserved_on_cancel() {
    let (engine, transport, chat) = fixture("usage-stop");
    let connection = engine.connection("mock").unwrap();
    let reply = engine.start(input(&chat), Arc::new(|_| {})).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while transport.calls.lock().unwrap().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    {
        let store = engine.store.lock().unwrap();
        let summary = store.usage_summary(&connection).unwrap();
        assert_eq!(summary.requests, 1);
        assert_eq!(
            summary.missing_usage, 1,
            "An unfinished request must already be visible as unknown"
        );
    }
    engine.stop(&chat);
    finish(&engine, &chat).await;
    let store = engine.store.lock().unwrap();
    let summary = store.usage_summary(&connection).unwrap();
    assert_eq!(
        (summary.requests, summary.input, summary.output),
        (1, 25, 5)
    );
    assert_eq!(summary.unpriced_requests, 1);
    assert_eq!(store.message(&reply.id).unwrap().status, "stopped");
}

#[tokio::test]
async fn context_preflight_rejects_oversized_drafts_without_consuming_files_or_history() {
    let (engine, transport, chat) = fixture("usage");
    let connection = engine.connection("mock").unwrap();
    let (file, before) = {
        let store = engine.store.lock().unwrap();
        store.save_model_limit(&connection, 1024).unwrap();
        (
            store
                .save_draft_attachment(
                    "note.txt",
                    "text",
                    "Synthetic reference",
                    b"Synthetic reference",
                )
                .unwrap(),
            store.messages(&chat, None).unwrap().len(),
        )
    };
    let mut request = input(&chat);
    request.folder_context = false;
    request.content = "x".repeat(8000);
    let error = engine
        .start_with_files(
            request,
            None,
            std::slice::from_ref(&file.id),
            Arc::new(|_| {}),
        )
        .unwrap_err();
    assert!(error.to_string().contains("input budget"));
    let store = engine.store.lock().unwrap();
    assert_eq!(store.messages(&chat, None).unwrap().len(), before);
    store.validate_attachment_ids(&[file.id]).unwrap();
    assert!(transport.calls.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn unavailable_project_excerpts_do_not_interrupt_a_new_file_proposal() {
    use melly_core::{
        file_edits::Proposal,
        linked,
        projects::{ProjectFile, text_passages},
    };
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::open(&temporary.path().join("profile.sqlite3")).unwrap();
    let folder = store.create_folder("Synthetic linked project").unwrap();
    let root = temporary.path().join("linked");
    std::fs::create_dir(&root).unwrap();
    let link = store.set_project_link(&folder.id, &root).unwrap();
    let chat = store.create_chat(Some(&folder.id)).unwrap();
    let mut indexed = Vec::new();
    for (file, snippet) in [
        ("changed.md", "sample OUTDATED_CANARY"),
        ("missing.md", "sample MISSING_CANARY"),
        ("current.md", "sample CURRENT_CANARY"),
    ] {
        std::fs::write(root.join(file), snippet).unwrap();
        indexed.push((
            ProjectFile {
                id: uuid::Uuid::new_v4().to_string(),
                folder_id: folder.id.clone(),
                name: file.into(),
                bytes: snippet.len() as u64,
                revision: linked::revision(snippet.as_bytes()),
                linked_path: Some(file.into()),
            },
            text_passages(snippet, None),
        ));
    }
    store
        .replace_linked_index(&link, &indexed, "ready")
        .unwrap();
    assert_eq!(
        store
            .search_project_files(&chat.id, "sample")
            .unwrap()
            .len(),
        3
    );
    std::fs::write(root.join("changed.md"), "sample modified on disk").unwrap();
    std::fs::remove_file(root.join("missing.md")).unwrap();

    let fake = Arc::new(Fake {
        mode: "create-file",
        calls: Mutex::new(vec![]),
    });
    let engine = Arc::new(Engine::with_transport(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
        fake.clone(),
    ));
    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = updates.clone();
    let mut request = input(&chat.id);
    request.content = "Create a new Python file for a sample.".into();
    let reply = engine
        .start(
            request,
            Arc::new(move |update| captured.lock().unwrap().push(update)),
        )
        .unwrap();
    finish(&engine, &chat.id).await;
    let reply = engine.store.lock().unwrap().message(&reply.id).unwrap();
    assert_eq!(reply.status, "complete", "{}", reply.content);
    let proposal = Proposal::parse(&reply.content).unwrap();
    assert_eq!(proposal.path, "example.py");
    assert_eq!(proposal.apply_to(None).unwrap(), "print(42)\n");
    assert!(!root.join("example.py").exists());
    assert_eq!(reply.sources.len(), 1);
    assert!(reply.sources[0].snippet.contains("CURRENT_CANARY"));

    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let forwarded = &calls[0].messages.last().unwrap().content;
    assert!(forwarded.contains("CURRENT_CANARY"));
    assert!(!forwarded.contains("OUTDATED_CANARY"));
    assert!(!forwarded.contains("MISSING_CANARY"));
    assert!(
        updates
            .lock()
            .unwrap()
            .iter()
            .flat_map(|update| &update.sources)
            .all(|source| source.snippet.contains("CURRENT_CANARY"))
    );
}
