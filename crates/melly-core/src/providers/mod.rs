//! Fixed cloud routes, bounded streaming, and credential-isolated transport.
//! No provider credentials or raw request/response bodies are logged or exposed in errors.
use crate::models::{Connection, ModelInfo, ProviderRequest, ProviderSink, Source};
use anyhow::{Result, ensure};
use tokio_util::sync::CancellationToken;

mod catalog;
mod mock;
mod network;
mod protocol;
mod sse;
mod transport;
pub use catalog::{catalog, credential_id, supports_native_search, validate_connection};
pub use network::public_url;
pub async fn list_models(
    connection: &Connection,
    credential: Option<&str>,
    cancel: CancellationToken,
) -> Result<Vec<ModelInfo>> {
    transport::check_cancel(&cancel)?;
    validate_connection(connection)?;
    if connection.provider_id == "mock" {
        return Ok(vec![ModelInfo {
            id: "melly-demo".into(),
            name: "Melly demo · Mock".into(),
        }]);
    }
    let url = public_url(&format!(
        "{}/models",
        catalog::normalized_base(&connection.base_url)
    ))?;
    let request = authenticated(
        network::client()?.get(url),
        &connection.provider_id,
        credential,
    )?;
    let response = transport::send(request, &connection.provider_id, &cancel).await?;
    protocol::models(
        &connection.provider_id,
        transport::read_json(response, &connection.provider_id, &cancel).await?,
    )
}

pub async fn stream(
    request: ProviderRequest,
    credential: Option<String>,
    sink: ProviderSink,
    cancel: CancellationToken,
) -> Result<()> {
    transport::check_cancel(&cancel)?;
    validate_connection(&request.connection)?;
    let observer_cancel = cancel.clone();
    let guarded: ProviderSink = std::sync::Arc::new(move |event| {
        if !observer_cancel.is_cancelled() {
            sink(event);
        }
    });
    if request.connection.provider_id == "mock" {
        return mock::stream(request, guarded, cancel).await;
    }
    let (url, body, protocol) = protocol::request(&request)?;
    let provider = &request.connection.provider_id;
    let http = authenticated(
        network::client()?
            .post(url)
            .json(&body)
            .header("Accept", "text/event-stream"),
        provider,
        credential.as_deref(),
    )?;
    let response = transport::send(http, provider, &cancel).await?;
    transport::read_stream(response, protocol, provider, guarded, &cancel).await
}

fn authenticated(
    mut request: reqwest::RequestBuilder,
    provider: &str,
    credential: Option<&str>,
) -> Result<reqwest::RequestBuilder> {
    if provider != "custom" {
        ensure!(
            credential.is_some_and(|key| !key.trim().is_empty()),
            "{provider} credential is not configured"
        );
    }
    if let Some(key) = credential.filter(|key| !key.is_empty()) {
        request = match provider {
            "claude" => request.header("x-api-key", key),
            "gemini" => request.header("x-goog-api-key", key),
            _ => request.bearer_auth(key),
        };
    }
    if provider == "claude" {
        request = request.header("anthropic-version", "2026-10-21");
    }
    if provider == "gemini" {
        request = request.header("Api-Revision", "2026-05-20");
    }
    Ok(request)
}

/// Fixed Tavily search endpoint. This takes only the dedicated search credential.
pub async fn web_search(query: &str, key: &str, cancel: CancellationToken) -> Result<Vec<Source>> {
    transport::check_cancel(&cancel)?;
    ensure!(
        !key.trim().is_empty(),
        "Search credential is not configured"
    );
    ensure!(
        !query.trim().is_empty() && query.chars().count() <= 2000,
        "Search query must contain 1–2000 characters"
    );
    let body = serde_json::json!({"query":query,"search_depth":"basic","topic":"general","max_results":5,"include_answer":false,"include_raw_content":false,"include_images":false});
    let request = network::client()?
        .post("https://api.tavily.com/search")
        .bearer_auth(key)
        .json(&body);
    let response = transport::send(request, "Tavily search", &cancel).await?;
    let value = transport::read_json(response, "Tavily search", &cancel).await?;
    let results = value["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Tavily search returned invalid results"))?;
    Ok(results
        .iter()
        .take(5)
        .filter_map(|result| {
            sse::source(
                result["url"].as_str()?,
                result["title"].as_str().unwrap_or("Web source"),
                result["content"].as_str().unwrap_or(""),
            )
        })
        .collect())
}
