use melly_core::{models::*, providers};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn connection(id: &str) -> Connection {
    let p = providers::catalog()
        .into_iter()
        .find(|p| p.id == id)
        .expect("provider in catalog");
    Connection {
        provider_id: p.id,
        base_url: p.base_url,
        model_id: p.default_model,
    }
}

#[test]
fn catalog_has_explicit_isolated_billing_routes() {
    let c = providers::catalog();
    let ids: Vec<_> = c.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "mock",
            "chatgpt",
            "muse",
            "claude",
            "gemini",
            "grok",
            "openrouter",
            "deepseek",
            "qwen-standard",
            "qwen-token-plan",
            "moonshot",
            "ollama",
            "custom"
        ]
    );
    let standard = connection("qwen-standard");
    let plan = connection("qwen-token-plan");
    assert_eq!(
        standard.base_url,
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
    );
    assert_eq!(
        plan.base_url,
        "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"
    );
    assert_ne!(
        providers::credential_id(&standard),
        providers::credential_id(&plan)
    );
    assert!(
        c.iter()
            .find(|p| p.id == "muse")
            .unwrap()
            .disabled_reason
            .is_some()
    );
    assert!(providers::validate_connection(&connection("muse")).is_err());
}

#[test]
fn ollama_cloud_is_a_fixed_key_route_not_a_local_runtime_or_plan_fallback() {
    let provider = providers::catalog()
        .into_iter()
        .find(|entry| entry.id == "ollama")
        .expect("Ollama Cloud provider");
    let mut connection = connection("ollama");
    assert_eq!(connection.base_url, "https://ollama.com/v1");
    assert_eq!(provider.protocol, "chat-completions");
    assert!(provider.billing_route.contains("API key"));
    assert!(!provider.native_search);
    providers::validate_connection(&connection).unwrap();
    connection.base_url = "http://localhost:11434/v1".into();
    assert!(providers::validate_connection(&connection).is_err());
}

#[test]
fn fixed_endpoints_cannot_be_replaced() {
    for p in providers::catalog()
        .into_iter()
        .filter(|p| !["custom", "muse"].contains(&p.id.as_str()))
    {
        let mut c = connection(&p.id);
        assert!(providers::validate_connection(&c).is_ok());
        c.base_url = "https://example.com/v1".into();
        assert!(
            providers::validate_connection(&c).is_err(),
            "{} must be immutable",
            p.id
        );
    }
}

#[test]
fn public_urls_reject_private_reserved_and_non_https_destinations() {
    for url in [
        "http://example.com",
        "file:///tmp/example",
        "https://user:pass@example.com",
        "https://localhost",
        "https://localhost.",
        "https://a.localhost",
        "https://nas.local",
        "https://127.0.0.1",
        "https://127.1",
        "https://2130706433",
        "https://10.0.0.1",
        "https://172.31.0.1",
        "https://192.168.1.1",
        "https://169.254.169.254",
        "https://100.64.0.1",
        "https://192.0.2.1",
        "https://198.18.0.1",
        "https://224.0.0.1",
        "https://240.1.1.1",
        "https://[::1]",
        "https://[::ffff:127.0.0.1]",
        "https://[fc00::1]",
        "https://[fe80::1]",
        "https://[2001:db8::1]",
        "https://[2002:7f00:1::1]",
    ] {
        assert!(providers::public_url(url).is_err(), "accepted {url}");
    }
    assert!(providers::public_url("https://example.com/reference?q=test#section").is_ok());
    assert!(providers::public_url("https://8.8.8.8/").is_ok());
}

#[test]
fn custom_credentials_are_bound_to_exact_normalized_endpoint() {
    let mut a = Connection {
        provider_id: "custom".into(),
        base_url: "https://EXAMPLE.com:443/v1/".into(),
        model_id: "example".into(),
    };
    let mut b = a.clone();
    b.base_url = "https://example.com/v1".into();
    assert_eq!(providers::credential_id(&a), providers::credential_id(&b));
    a.base_url = "https://example.com/v2".into();
    assert_ne!(providers::credential_id(&a), providers::credential_id(&b));
    a.base_url = "https://example.org/v1".into();
    assert_ne!(providers::credential_id(&a), providers::credential_id(&b));
    b.provider_id = "chatgpt".into();
    assert_ne!(providers::credential_id(&a), providers::credential_id(&b));
    for base in [
        "https://example.com/v1?api_key=synthetic",
        "https://example.com/v1#x",
    ] {
        a.base_url = base.into();
        assert!(providers::validate_connection(&a).is_err());
    }
}

fn mock_request() -> ProviderRequest {
    ProviderRequest {
        connection: connection("mock"),
        system_prompt: "Synthetic demo.".into(),
        messages: vec![WireMessage::text(
            "user",
            "What do we know about the interview?",
        )],
        tools: vec![
            json!({"type":"function","function":{"name":"search_folder","parameters":{"type":"object"}}}),
            json!({"type":"function","function":{"name":"search_web","parameters":{"type":"object"}}}),
        ],
        web_search: true,
    }
}

#[tokio::test]
async fn mock_requests_both_offered_tools_and_waits_for_real_results() {
    let request = mock_request();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    providers::stream(
        request,
        None,
        Arc::new(move |e| captured.lock().unwrap().push(e)),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let events = events.lock().unwrap();
    let calls: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let ProviderEvent::ToolCall(t) = e {
                Some(t.name.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls, ["search_folder", "search_web"]);
    assert!(matches!(events.last(), Some(ProviderEvent::Done)));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProviderEvent::Sources(_)))
    );
}

#[tokio::test]
async fn cancelled_request_emits_nothing() {
    let token = CancellationToken::new();
    token.cancel();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let result = providers::stream(
        mock_request(),
        None,
        Arc::new(move |e| captured.lock().unwrap().push(e)),
        token,
    )
    .await;
    assert!(result.is_err());
    assert!(events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mock_answer_uses_only_actual_tool_result_sources() {
    let mut request = mock_request();
    let source = Source {
        id: "chat-fact".into(),
        kind: "chat".into(),
        title: "Synthetic interview".into(),
        chat_id: Some("other-chat".into()),
        message_id: Some("fact-message".into()),
        snippet: "The synthetic interview is on Tuesday at 10 AM.".into(),
        ..Source::default()
    };
    request.messages.push(WireMessage {
        role: "tool".into(),
        content: json!({"sources":[source]}).to_string(),
        tool_call_id: Some("mock-search-folder".into()),
        tool_calls: vec![],
        claude_continuation: None,
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    providers::stream(
        request,
        None,
        Arc::new(move |e| captured.lock().unwrap().push(e)),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let events = events.lock().unwrap();
    let text: String = events
        .iter()
        .filter_map(|e| {
            if let ProviderEvent::Delta(t) = e {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(text.contains("Mock"));
    assert!(text.contains("Tuesday at 10 AM"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ProviderEvent::Sources(s) if s[0].id == "chat-fact"))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ProviderEvent::ToolCall(_)))
    );
}

#[tokio::test]
async fn stopping_mock_on_first_chunk_prevents_later_chunks_and_done() {
    let mut request = mock_request();
    request.tools.clear();
    request.web_search = false;
    let token = CancellationToken::new();
    let stopper = token.clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let result = providers::stream(
        request,
        None,
        Arc::new(move |e| {
            if matches!(e, ProviderEvent::Delta(_)) {
                stopper.cancel();
            }
            captured.lock().unwrap().push(e);
        }),
        token,
    )
    .await;
    assert!(result.is_err());
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
}

#[tokio::test]
async fn mock_external_search_uses_offered_tool_when_native_search_flag_is_false() {
    let mut request = mock_request();
    request.web_search = false;
    let events = Arc::new(Mutex::new(vec![]));
    let captured = events.clone();
    providers::stream(
        request,
        None,
        Arc::new(move |e| captured.lock().unwrap().push(e)),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let events = events.lock().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e,ProviderEvent::ToolCall(c) if c.name == "search_web"))
    );
}

#[tokio::test]
async fn explicit_mock_stop_demo_stays_active_until_stopped() {
    let mut request = mock_request();
    request.tools.clear();
    request.web_search = false;
    request.messages = vec![WireMessage::text("user", "Demonstrate streaming and Stop")];
    let events = Arc::new(Mutex::new(vec![]));
    let captured = events.clone();
    let token = CancellationToken::new();
    let stopped = token.clone();
    let task = tokio::spawn(async move {
        providers::stream(
            request,
            None,
            Arc::new(move |e| captured.lock().unwrap().push(e)),
            stopped,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !task.is_finished(),
        "the explicit Mock fixture must remain stoppable by a human"
    );
    token.cancel();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(300), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let events = events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(e, ProviderEvent::Delta(_))));
    assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
}
