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
            401 | 403 => "authorization unavailable",
            429 => "request limit reached",
            300..=399 => "redirect refused",
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
    ensure!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")),
        "{provider} did not return an event stream"
    );
    let mut chunks = response.bytes_stream();
    let mut decoder = Decoder::new(protocol, provider);
    loop {
        let chunk = tokio::select! { biased; _ = cancel.cancelled() => bail!("Request cancelled"), chunk = chunks.next() => chunk };
        match chunk {
            Some(Ok(bytes)) => {
                check_cancel(cancel)?;
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
