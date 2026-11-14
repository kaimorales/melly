use super::sse::{Decoder, Protocol};
use crate::models::ProviderSink;
use anyhow::{Result, bail, ensure};
use futures_util::StreamExt;
use reqwest::{RequestBuilder, Response};
use tokio_util::sync::CancellationToken;

pub(crate) fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "Request cancelled");
    Ok(())
}

pub(crate) async fn send(
    request: RequestBuilder,
    provider: &str,
    cancel: &CancellationToken,
) -> Result<Response> {
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => bail!("Request cancelled"),
        response = request.send() => response.map_err(|error| if error.is_timeout() {
            anyhow::anyhow!("{provider} connection timed out")
        } else { anyhow::anyhow!("{provider} connection failed") })?,
    };
    let status = response.status();
    // Never read an error body or location: either can contain reflected credentials or conversation text.
    if !status.is_success() {
        let reason = match status.as_u16() {
            400 => "request rejected; check the model ID and settings",
            401 => "authorization rejected; check this connection in Settings",
            403 => "access denied; check model and account permissions",
            404 => "model or endpoint not found; check the model ID and settings",
            429 => "provider reported a rate or usage limit; check provider limits",
            300..=399 => "redirect refused",
            500..=599 => "provider error; try again later",
            _ => "request failed",
        };
        bail!("{provider} HTTP {}: {reason}", status.as_u16());
    }
    check_cancel(cancel)?;
    Ok(response)
}

pub(crate) async fn read_stream(
    response: Response,
    protocol: Protocol,
    provider: &str,
    sink: ProviderSink,
    cancel: &CancellationToken,
) -> Result<()> {
    let content_type = response.headers().get(reqwest::header::CONTENT_TYPE);
    // The official ChatGPT plan route can return valid Responses SSE without
    // this header. Validate its framing below; never accept an explicitly
    // different media type or extend this exception to arbitrary providers.
    let headerless_plan =
        content_type.is_none() && provider == "chatgpt" && matches!(protocol, Protocol::Responses);
    ensure!(
        headerless_plan
            || content_type.and_then(|v| v.to_str().ok()).is_some_and(|s| s
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")),
        "{provider} did not return an event stream"
    );
    let mut chunks = response.bytes_stream();
    let mut decoder = Decoder::new(protocol, provider);
    let mut prefix = headerless_plan.then(Vec::new);
    loop {
        let chunk = tokio::select! { biased; _ = cancel.cancelled() => bail!("Request cancelled"), chunk = chunks.next() => chunk };
        match chunk {
            Some(Ok(bytes)) => {
                check_cancel(cancel)?;
                if let Some(pending) = &mut prefix
                    && validate_sse_start(pending, &bytes, provider)?
                {
                    prefix = None;
                }
                if decoder.push(&bytes, &sink)? {
                    check_cancel(cancel)?;
                    return decoder.finish();
                }
            }
            Some(Err(_)) => bail!("{provider} stream interrupted"),
            None => return decoder.finish(),
        }
    }
}

/// Check a bounded prefix across arbitrary network chunk boundaries before
/// trusting a headerless stream. JSON/HTML cannot hide a completion event after
/// a non-SSE prefix. The normal decoder still enforces size and terminal events.
fn validate_sse_start(prefix: &mut Vec<u8>, bytes: &[u8], provider: &str) -> Result<bool> {
    const STARTS: [&[u8]; 10] = [
        b"event:",
        b"data:",
        b":",
        b"id:",
        b"retry:",
        b"\xef\xbb\xbfevent:",
        b"\xef\xbb\xbfdata:",
        b"\xef\xbb\xbf:",
        b"\xef\xbb\xbfid:",
        b"\xef\xbb\xbfretry:",
    ];
    for byte in bytes.iter().take(9) {
        prefix.push(*byte);
        if STARTS.iter().any(|start| prefix.starts_with(start)) {
            return Ok(true);
        }
        ensure!(
            STARTS.iter().any(|start| start.starts_with(prefix)),
            "{provider} did not return an event stream"
        );
    }
    Ok(false)
}

pub(crate) async fn read_json(
    response: Response,
    provider: &str,
    cancel: &CancellationToken,
) -> Result<serde_json::Value> {
    let mut stream = response.bytes_stream();
    let mut bytes = vec![];
    loop {
        let chunk = tokio::select! { biased; _ = cancel.cancelled() => bail!("Request cancelled"), chunk = stream.next() => chunk };
        match chunk {
            Some(Ok(chunk)) => {
                ensure!(
                    bytes.len() + chunk.len() <= 2 * 1024 * 1024,
                    "{provider} response exceeds size limit"
                );
                bytes.extend_from_slice(&chunk);
            }
            Some(Err(_)) => bail!("{provider} response interrupted"),
            None => break,
        }
    }
    check_cancel(cancel)?;
    let result: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("{provider} returned invalid JSON"))?;
    ensure!(result["error"].is_null(), "{provider} request failed");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderEvent;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::timeout,
    };

    async fn server(
        response: Option<String>,
    ) -> (String, tokio::task::JoinHandle<()>, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(String::new()));
        let captured = seen.clone();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let count = socket.read(&mut buffer).await.unwrap();
            *captured.lock().unwrap() = String::from_utf8_lossy(&buffer[..count]).to_string();
            if let Some(response) = response {
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            std::future::pending::<()>().await;
        });
        (format!("http://{address}"), task, seen)
    }
    fn test_client() -> reqwest::Client {
        // Test-only loopback exception; production calls never accept HTTP/private destinations.
        super::super::network::client_builder()
            .https_only(false)
            .build()
            .unwrap()
    }
    async fn read_response(
        headers: &str,
        body: &str,
        provider: &str,
        protocol: Protocol,
    ) -> (Result<()>, Vec<ProviderEvent>) {
        let (url, task, _) = server(Some(format!(
            "HTTP/1.1 200 OK\r\n{headers}Content-Length: {}\r\n\r\n{body}",
            body.len()
        )))
        .await;
        let response = test_client().get(url).send().await.unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let captured = events.clone();
        let result = read_stream(
            response,
            protocol,
            provider,
            Arc::new(move |e| captured.lock().unwrap().push(e)),
            &CancellationToken::new(),
        )
        .await;
        task.abort();
        let events = events.lock().unwrap().clone();
        (result, events)
    }
    const PLAN_REPLY: &str = concat!(
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Synthetic reply\"}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n"
    );
    #[tokio::test]
    async fn chatgpt_plan_stream_allows_missing_content_type() {
        // Observed on the official plan Responses endpoint: HTTP 200 with SSE
        // framing and no Content-Type. Completion must still be native/explicit.
        let (result, events) = read_response("", PLAN_REPLY, "chatgpt", Protocol::Responses).await;
        result.unwrap();
        assert!(
            matches!(events.first(), Some(ProviderEvent::Delta(text)) if text == "Synthetic reply")
        );
        assert!(events.iter().any(|e| matches!(e, ProviderEvent::Done)));
    }
    #[tokio::test]
    async fn missing_content_type_is_not_a_generic_provider_exception() {
        for (provider, protocol) in [
            ("custom", Protocol::Responses),
            ("chatgpt", Protocol::Compatible),
        ] {
            let (result, events) = read_response("", PLAN_REPLY, provider, protocol).await;
            assert!(result.is_err());
            assert!(events.is_empty());
        }
    }
    #[tokio::test]
    async fn chatgpt_rejects_explicit_non_sse_content_types() {
        for mime in [
            "application/json",
            "text/html",
            "text/plain",
            "application/synthetic-secret",
        ] {
            let (result, events) = read_response(
                &format!("Content-Type: {mime}\r\n"),
                PLAN_REPLY,
                "chatgpt",
                Protocol::Responses,
            )
            .await;
            let error = result.unwrap_err().to_string();
            assert!(!error.contains("synthetic-secret"));
            assert!(events.is_empty());
        }
    }
    #[tokio::test]
    async fn headerless_plan_response_rejects_non_sse_and_false_completion() {
        for body in [
            "{\"error\":\"synthetic-secret\"}",
            "<html>synthetic-secret</html>\nevent: response.completed\ndata: {\"type\":\"response.completed\"}\n\n",
            "data: [DONE]\n\n",
            "event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"synthetic-secret\"}}}\n\n",
        ] {
            let (result, events) = read_response("", body, "chatgpt", Protocol::Responses).await;
            assert!(!result.unwrap_err().to_string().contains("synthetic-secret"));
            assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
        }
    }
    #[tokio::test]
    async fn headerless_plan_stream_preserves_partial_reply_on_disconnect() {
        let (result, events) = read_response(
            "",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Partial\"}\n\n",
            "chatgpt",
            Protocol::Responses,
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("interrupted"));
        assert!(matches!(events.first(), Some(ProviderEvent::Delta(text)) if text == "Partial"));
        assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
    }
    #[test]
    fn headerless_sse_prefix_handles_split_bom_fields_and_comments() {
        for start in [
            "event:",
            "data:",
            ":",
            "id:",
            "retry:",
            "\u{feff}event:",
            "\u{feff}data:",
            "\u{feff}:",
        ] {
            let mut prefix = Vec::new();
            for (index, byte) in start.as_bytes().iter().enumerate() {
                assert_eq!(
                    validate_sse_start(&mut prefix, &[*byte], "chatgpt").unwrap(),
                    index + 1 == start.len()
                );
            }
            assert!(prefix.len() <= 9);
        }
        for bad in [
            "<html>",
            "{\"error\":true}",
            "invalid",
            "eventual",
            "data=",
            "\u{feff}bad",
        ] {
            assert!(validate_sse_start(&mut Vec::new(), bad.as_bytes(), "chatgpt").is_err());
        }
    }
    #[tokio::test]
    async fn cancellation_interrupts_headerless_plan_stream_and_partial_prefix() {
        for chunk in ["", "1\r\ne\r\n"] {
            let (url, task, _) = server(Some(format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{chunk}"
            )))
            .await;
            let response = test_client().get(url).send().await.unwrap();
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            let stopper = tokio::spawn(async move {
                tokio::task::yield_now().await;
                trigger.cancel();
            });
            let events = Arc::new(Mutex::new(vec![]));
            let captured = events.clone();
            let result = timeout(
                Duration::from_millis(500),
                read_stream(
                    response,
                    Protocol::Responses,
                    "chatgpt",
                    Arc::new(move |e| captured.lock().unwrap().push(e)),
                    &cancel,
                ),
            )
            .await
            .unwrap();
            task.abort();
            stopper.abort();
            assert!(result.unwrap_err().to_string().contains("cancelled"));
            assert!(events.lock().unwrap().is_empty());
        }
    }
    #[tokio::test]
    async fn http_429_is_sanitized_and_retains_status() {
        let response = "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 33\r\n\r\nsynthetic-secret echoed in payload";
        let (url, task, _) = server(Some(response.into())).await;
        let error = send(
            test_client().get(url).bearer_auth("synthetic-secret"),
            "mock-service",
            &CancellationToken::new(),
        )
        .await
        .unwrap_err()
        .to_string();
        task.abort();
        assert!(error.contains("429"));
        assert!(error.contains("mock-service"));
        assert!(!error.contains("synthetic-secret"));
    }
    #[tokio::test]
    async fn provider_errors_offer_safe_status_specific_guidance() {
        for (status, hint) in [
            (400, "check the model ID and settings"),
            (401, "check this connection in Settings"),
            (403, "check model and account permissions"),
            (404, "check the model ID and settings"),
            (429, "check provider limits"),
            (500, "try again later"),
            (529, "try again later"),
        ] {
            let body = "synthetic-secret and synthetic-private-chat";
            let response = format!(
                "HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\nLocation: https://example.com/synthetic-secret\r\n\r\n{body}",
                body.len()
            );
            let (url, task, seen) = server(Some(response)).await;
            let error = send(
                test_client().get(url).bearer_auth("synthetic-secret"),
                "synthetic-provider",
                &CancellationToken::new(),
            )
            .await
            .unwrap_err()
            .to_string();
            task.abort();
            assert!(error.contains(&status.to_string()) && error.contains("synthetic-provider"));
            assert!(error.contains(hint), "{status}: {error}");
            assert!(
                !error.contains("synthetic-secret") && !error.contains("synthetic-private-chat")
            );
            assert!(seen.lock().unwrap().contains("synthetic-secret"));
        }
    }
    #[tokio::test]
    async fn redirect_is_rejected_without_contacting_target() {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let response = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/capture\r\nContent-Length: 0\r\n\r\n",
            target.local_addr().unwrap()
        );
        let (url, task, seen) = server(Some(response)).await;
        let result = send(
            test_client().get(url).bearer_auth("synthetic-secret"),
            "mock-service",
            &CancellationToken::new(),
        )
        .await;
        task.abort();
        assert!(result.unwrap_err().to_string().contains("307"));
        assert!(seen.lock().unwrap().contains("synthetic-secret"));
        assert!(
            timeout(Duration::from_millis(30), target.accept())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn cancellation_interrupts_pending_response_headers() {
        let (url, task, seen) = server(None).await;
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let stopper = tokio::spawn(async move {
            while seen.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
            trigger.cancel();
        });
        let result = timeout(
            Duration::from_millis(500),
            send(test_client().get(url), "mock-service", &cancel),
        )
        .await
        .unwrap();
        task.abort();
        stopper.abort();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
    #[tokio::test]
    async fn cancellation_interrupts_pending_stream_read() {
        let (url, task, _) = server(Some("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n".into())).await;
        let response = test_client().get(url).send().await.unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let stopper = tokio::spawn(async move {
            tokio::task::yield_now().await;
            trigger.cancel();
        });
        let events = Arc::new(Mutex::new(vec![]));
        let captured = events.clone();
        let result = timeout(
            Duration::from_millis(500),
            read_stream(
                response,
                Protocol::Compatible,
                "mock-service",
                Arc::new(move |e| captured.lock().unwrap().push(e)),
                &cancel,
            ),
        )
        .await
        .unwrap();
        task.abort();
        stopper.abort();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, ProviderEvent::Done))
        );
    }
    #[tokio::test]
    async fn cancellation_interrupts_blocked_tls_connect() {
        let (url, task, seen) = server(None).await;
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let stopper = tokio::spawn(async move {
            while seen.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
            trigger.cancel();
        });
        let result = timeout(
            Duration::from_millis(500),
            send(
                test_client().get(url.replacen("http:", "https:", 1)),
                "mock-service",
                &cancel,
            ),
        )
        .await
        .unwrap();
        task.abort();
        stopper.abort();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
    #[tokio::test]
    async fn http_disconnect_preserves_partial_text_but_never_done() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Synthetic partial\"}}]}\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (url, task, _) = server(Some(response)).await;
        let response = test_client().get(url).send().await.unwrap();
        let events = Arc::new(Mutex::new(vec![]));
        let captured = events.clone();
        let result = read_stream(
            response,
            Protocol::Compatible,
            "mock-service",
            Arc::new(move |e| captured.lock().unwrap().push(e)),
            &CancellationToken::new(),
        )
        .await;
        task.abort();
        assert!(result.is_err());
        let events = events.lock().unwrap();
        assert!(matches!(events.first(), Some(ProviderEvent::Delta(_))));
        assert!(!events.iter().any(|e| matches!(e, ProviderEvent::Done)));
    }
}
