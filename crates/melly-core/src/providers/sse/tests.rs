use super::*;
use crate::models::{ProviderEvent, ToolCall};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn parse(protocol: Protocol, bytes: &[u8], width: usize) -> (Result<()>, Vec<ProviderEvent>) {
    let capture = Arc::new(Mutex::new(vec![]));
    let output = capture.clone();
    let sink: ProviderSink = Arc::new(move |event| output.lock().unwrap().push(event));
    let mut decoder = Decoder::new(protocol, "synthetic");
    let result = (|| {
        for chunk in bytes.chunks(width) {
            if decoder.push(chunk, &sink)? {
                break;
            }
        }
        decoder.finish()
    })();
    let events = std::mem::take(&mut *capture.lock().unwrap());
    (result, events)
}

fn calls(events: &[ProviderEvent]) -> Vec<ToolCall> {
    events
        .iter()
        .filter_map(|e| {
            if let ProviderEvent::ToolCall(c) = e {
                Some(c.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn compatible_preserves_utf8_multiline_sse_and_argument_fragments() {
    let bytes = include_bytes!("../../../../../tests/fixtures/providers/compatible.sse");
    for width in 1..=13 {
        let (result, events) = parse(Protocol::Compatible, bytes, width);
        result.unwrap();
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
        assert_eq!(text, "Café 🌳");
        let calls = calls(&events);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_demo");
        assert_eq!(calls[0].name, "search_folder");
        assert_eq!(calls[0].arguments, json!({"query":"interview"}));
        assert!(matches!(events.last(), Some(ProviderEvent::Done)));
    }
}

#[test]
fn broken_or_error_stream_never_completes_or_dispatches_partial_tools() {
    let bytes = include_bytes!("../../../../../tests/fixtures/providers/compatible.sse");
    let without_done = &bytes[..bytes
        .windows(12)
        .position(|b| b == b"data: [DONE]")
        .unwrap()];
    let (result, events) = parse(Protocol::Compatible, without_done, 3);
    assert!(result.is_err());
    assert!(calls(&events).is_empty());
    assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
    for bytes in [
        b"data: {broken}\n\n".as_slice(),
        b"data: {\"error\":{\"message\":\"synthetic-secret\"}}\n\n".as_slice(),
    ] {
        let (result, _) = parse(Protocol::Compatible, bytes, 1);
        let error = result.unwrap_err().to_string();
        assert!(!error.contains("synthetic-secret"));
    }
}

#[test]
fn responses_require_response_completed_and_preserve_call_id() {
    let bytes = include_bytes!("../../../../../tests/fixtures/providers/responses.sse");
    let (result, events) = parse(Protocol::Responses, bytes, 1);
    result.unwrap();
    let calls = calls(&events);
    assert_eq!(calls[0].id, "call_response");
    assert_eq!(calls[0].arguments, json!({"query":"public news"}));
    let (result, _) = parse(Protocol::Responses, b"data: [DONE]\n\n", 1);
    assert!(result.is_err());
    let (result, _) = parse(Protocol::Responses, b"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"synthetic-secret\"}}}\n\n", 1);
    assert!(!result.unwrap_err().to_string().contains("synthetic-secret"));
}

#[test]
fn anthropic_content_blocks_and_terminal_are_native() {
    let bytes = include_bytes!("../../../../../tests/fixtures/providers/messages.sse");
    let (result, events) = parse(Protocol::Messages, bytes, 2);
    result.unwrap();
    let calls = calls(&events);
    assert_eq!(calls[0].id, "toolu_demo");
    assert_eq!(calls[0].arguments, json!({"query":"interview"}));
    let (result, _) = parse(Protocol::Messages, b"event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n", 1);
    assert!(result.is_err());
}

fn anthropic_mixed_tools(server_first: bool, malformed: bool) -> Vec<u8> {
    let (server_index, client_index) = if server_first { (0, 1) } else { (1, 0) };
    let mut server = vec![
        json!({"type":"content_block_start","index":server_index,"content_block":{"type":"server_tool_use","id":"srvtoolu_demo","name":"web_search","input":{}}}),
        json!({"type":"content_block_delta","index":server_index,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"current "}}),
    ];
    if !malformed {
        server.push(json!({"type":"content_block_delta","index":server_index,"delta":{"type":"input_json_delta","partial_json":"forecast\"}"}}));
    }
    server.push(json!({"type":"content_block_stop","index":server_index}));
    let client = vec![
        json!({"type":"content_block_start","index":client_index,"content_block":{"type":"tool_use","id":"toolu_demo","name":"search_folder","input":{}}}),
        json!({"type":"content_block_delta","index":client_index,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"interview\"}"}}),
        json!({"type":"content_block_stop","index":client_index}),
    ];
    let mut frames = if server_first {
        [server, client].concat()
    } else {
        [client, server].concat()
    };
    frames.extend([
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
        json!({"type":"message_stop"}),
    ]);
    frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect::<String>()
        .into_bytes()
}

#[test]
fn anthropic_mixed_search_preserves_server_arguments_for_continuation() {
    for server_first in [true, false] {
        for width in [1, 7, 64] {
            let (result, events) = parse(
                Protocol::Messages,
                &anthropic_mixed_tools(server_first, false),
                width,
            );
            result.unwrap();
            let calls = calls(&events);
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].name, "search_folder");
            let spec = crate::providers::catalog()
                .into_iter()
                .find(|p| p.id == "claude")
                .unwrap();
            let request = crate::models::ProviderRequest {
                connection: crate::models::Connection {
                    provider_id: spec.id,
                    base_url: spec.base_url,
                    model_id: spec.default_model,
                },
                system_prompt: "Synthetic".into(),
                messages: vec![
                    crate::models::WireMessage::text("user", "Synthetic question"),
                    crate::models::WireMessage {
                        role: "assistant".into(),
                        content: String::new(),
                        tool_call_id: None,
                        tool_calls: calls,
                        claude_continuation: None,
                    },
                    crate::models::WireMessage {
                        role: "tool".into(),
                        content: "{\"sources\":[]}".into(),
                        tool_call_id: Some("toolu_demo".into()),
                        tool_calls: vec![],
                        claude_continuation: None,
                    },
                ],
                tools: vec![json!({"name":"search_folder","parameters":{"type":"object"}})],
                web_search: true,
            };
            let (_, body, _) = super::super::protocol::request(&request).unwrap();
            let blocks = body["messages"][1]["content"].as_array().unwrap();
            let web = blocks
                .iter()
                .find(|block| block["type"] == "server_tool_use")
                .unwrap();
            assert_eq!(web["input"], json!({"query":"current forecast"}));
            assert_eq!(
                body["messages"][2]["content"][0]["tool_use_id"],
                "toolu_demo"
            );
        }
    }
}

#[test]
fn malformed_anthropic_server_tool_arguments_never_dispatch_client_tools() {
    let (result, events) = parse(Protocol::Messages, &anthropic_mixed_tools(true, true), 1);
    assert!(result.is_err());
    assert!(calls(&events).is_empty());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ProviderEvent::Done))
    );
}

#[test]
fn anthropic_paused_web_search_is_a_continuable_response() {
    let bytes = b"data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"server_tool_use\",\"id\":\"srvtoolu_demo\",\"name\":\"web_search\",\"input\":{}}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\":\\\"public sources\\\"}\"}}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"pause_turn\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n";
    let (result, events) = parse(Protocol::Messages, bytes, 1);
    assert!(
        result.is_ok(),
        "A paused server search needs a continuation, not an error"
    );
    assert!(
        calls(&events).is_empty(),
        "Server tools must never be dispatched as client tools"
    );
    let continuation = events
        .iter()
        .find_map(|event| match event {
            ProviderEvent::ClaudeSearchPaused(continuation) => Some(continuation),
            _ => None,
        })
        .expect("The run controller must receive the paused server content");
    assert_eq!(
        continuation.blocks[0]["input"],
        json!({"query":"public sources"})
    );
    assert!(!format!("{continuation:?}").contains("public sources"));
}

#[test]
fn anthropic_pause_with_a_client_tool_fails_before_dispatch() {
    let bytes = String::from_utf8(anthropic_mixed_tools(true, false))
        .unwrap()
        .replace(
            "\"stop_reason\":\"tool_use\"",
            "\"stop_reason\":\"pause_turn\"",
        );
    assert!(bytes.contains("pause_turn"));
    let (result, events) = parse(Protocol::Messages, bytes.as_bytes(), 1);
    assert!(result.is_err());
    assert!(calls(&events).is_empty());
    assert!(!events.iter().any(|event| matches!(
        event,
        ProviderEvent::Done | ProviderEvent::ClaudeSearchPaused(_)
    )));
}

#[test]
fn gemini_interactions_preserve_native_continuation_steps() {
    let bytes = include_bytes!("../../../../../tests/fixtures/providers/interactions.sse");
    let (result, events) = parse(Protocol::Interactions, bytes, 3);
    result.unwrap();
    let calls = calls(&events);
    assert_eq!(calls[0].id, "gemini_call");
    assert_eq!(calls[0].arguments, json!({"query":"interview"}));
    let metadata = calls[0].provider_data.as_ref().unwrap();
    assert_eq!(metadata["steps"][0]["signature"], "synthetic-signature");
    assert_eq!(
        metadata["steps"][1]["arguments"],
        json!({"query":"interview"})
    );
}

#[test]
fn only_valid_public_https_citations_become_sources() {
    let bytes = b"data: {\"type\":\"response.output_text.annotation.added\",\"annotation\":{\"type\":\"url_citation\",\"url\":\"https://example.com/research\",\"title\":\"Synthetic source\"}}\n\ndata: {\"type\":\"response.output_text.annotation.added\",\"annotation\":{\"type\":\"url_citation\",\"url\":\"https://127.0.0.1/private\",\"title\":\"Rejected\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    let (result, events) = parse(Protocol::Responses, bytes, 1);
    result.unwrap();
    let sources: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let ProviderEvent::Sources(s) = e {
                Some(s)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].url.as_deref(),
        Some("https://example.com/research")
    );
}

#[test]
fn native_search_activity_is_reported_without_client_dispatch() {
    let cases = [
        (
            Protocol::Responses,
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"web_search_call\",\"id\":\"search1\"}}\n\ndata: {\"type\":\"response.completed\"}\n\n",
        ),
        (
            Protocol::Messages,
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"server_tool_use\",\"name\":\"web_search\"}}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
        ),
        (
            Protocol::Interactions,
            "data: {\"event_type\":\"step.start\",\"index\":0,\"step\":{\"type\":\"google_search_call\"}}\n\ndata: {\"event_type\":\"interaction.completed\",\"interaction\":{\"status\":\"completed\"}}\n\n",
        ),
    ];
    for (protocol, bytes) in cases {
        let (result, events) = parse(protocol, bytes.as_bytes(), 1);
        result.unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ProviderEvent::Activity(_)))
        );
        assert!(calls(&events).is_empty());
    }
}

#[test]
fn malformed_native_blocks_return_errors_without_panics() {
    for (protocol, bytes) in [
        (
            Protocol::Responses,
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":5}\n\n",
        ),
        (
            Protocol::Messages,
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":5}\n\n",
        ),
        (
            Protocol::Interactions,
            "data: {\"event_type\":\"step.start\",\"index\":0,\"step\":5}\n\n",
        ),
    ] {
        let capture: ProviderSink = Arc::new(|_| {});
        let mut decoder = Decoder::new(protocol, "synthetic");
        assert!(decoder.push(bytes.as_bytes(), &capture).is_err());
    }
}

#[test]
fn native_search_tool_errors_cannot_be_reported_as_success() {
    let bytes = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"web_search_tool_result\",\"content\":{\"type\":\"web_search_tool_result_error\",\"error_code\":\"synthetic-secret\"}}}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n";
    let (result, events) = parse(Protocol::Messages, bytes.as_bytes(), 1);
    let error = result.unwrap_err().to_string();
    assert!(!error.contains("synthetic-secret"));
    assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
}

#[test]
fn named_sse_error_never_becomes_compatible_success() {
    let bytes = "data: {\"choices\":[{\"index\":0,\"finish_reason\":\"stop\",\"delta\":{}}]}\n\nevent: error\ndata: {\"message\":\"synthetic-secret\"}\n\ndata: [DONE]\n\n";
    let (result, events) = parse(Protocol::Compatible, bytes.as_bytes(), 1);
    assert!(!result.unwrap_err().to_string().contains("synthetic-secret"));
    assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
}

#[test]
fn openrouter_reasoning_detail_fragments_are_hidden_and_preserved() {
    let prefix = "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"index\":0,\"type\":\"reasoning.encrypted\",\"data\":\"synthetic-\"}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_details\":[{\"index\":0,\"type\":\"reasoning.encrypted\",\"data\":\"payload\"}]}}]}\n\n";
    let bytes = format!(
        "{prefix}{}",
        String::from_utf8_lossy(include_bytes!(
            "../../../../../tests/fixtures/providers/compatible.sse"
        ))
    );
    let (result, events) = parse(Protocol::Compatible, bytes.as_bytes(), 1);
    result.unwrap();
    let calls = calls(&events);
    assert_eq!(
        calls[0].provider_data.as_ref().unwrap()["reasoning_details"][0]["data"],
        "synthetic-payload"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e,ProviderEvent::Delta(t) if t.contains("synthetic-payload")))
    );
}

#[test]
fn openrouter_reasoning_details_accept_documented_optional_index() {
    let prefix = "data: {\"choices\":[{\"delta\":{\"reasoning_details\":[{\"id\":\"reason-demo\",\"type\":\"reasoning.encrypted\",\"data\":\"synthetic\"}]}}]}\n\n";
    let bytes = format!(
        "{prefix}{}",
        String::from_utf8_lossy(include_bytes!(
            "../../../../../tests/fixtures/providers/compatible.sse"
        ))
    );
    let (result, events) = parse(Protocol::Compatible, bytes.as_bytes(), 1);
    result.unwrap();
    assert_eq!(
        calls(&events)[0].provider_data.as_ref().unwrap()["reasoning_details"][0]["data"],
        "synthetic"
    );
}
