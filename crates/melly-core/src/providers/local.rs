//! A fixed loopback-only Ollama connection with no credentials or redirects.
use crate::models::*;
use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{sync::OnceLock, time::Duration};
use tokio_util::sync::CancellationToken;

pub const BASE: &str = "http://127.0.0.1:11434";
fn client() -> Result<reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .read_timeout(Duration::from_secs(120))
        .timeout(Duration::from_secs(600))
        .build()?;
    Ok(CLIENT.get_or_init(|| client).clone())
}
async fn send(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<reqwest::Response> {
    super::transport::send(request, "Ollama Local", cancel)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}. Make sure Ollama is running on this Mac and the model is installed."
            )
        })
}
pub async fn models(cancel: CancellationToken) -> Result<Vec<ModelInfo>> {
    let response = send(client()?.get(format!("{BASE}/api/tags")), &cancel).await?;
    let json = super::transport::read_json(response, "Ollama Local", &cancel).await?;
    let models = json["models"]
        .as_array()
        .context("Ollama returned an invalid model list")?;
    Ok(models
        .iter()
        .filter(|model| model.get("remote_model").is_none() && model.get("remote_host").is_none())
        .filter_map(|model| model["name"].as_str())
        .filter(|name| !name.contains(":cloud") && !name.ends_with("-cloud") && name.len() <= 200)
        .take(500)
        .map(|name| ModelInfo {
            id: name.into(),
            name: name.into(),
            context_window: None,
            default_context_window: None,
        })
        .collect())
}
fn context_size(details: &Value) -> Result<usize> {
    ensure!(
        details.get("remote_model").is_none() && details.get("remote_host").is_none(),
        "Choose a downloaded local model for Ollama Local"
    );
    let info = details["model_info"]
        .as_object()
        .context("Ollama could not verify a downloaded local model")?;
    ensure!(
        !info.is_empty(),
        "Ollama could not verify a downloaded local model"
    );
    let size = info
        .iter()
        .find(|(key, _)| key.ends_with(".context_length"))
        .and_then(|(_, value)| value.as_u64())
        .unwrap_or(4096)
        .min(8192) as usize;
    ensure!(
        size >= 2048,
        "Choose a local model with a context window of at least 2,048 tokens"
    );
    if let Some(capabilities) = details["capabilities"].as_array() {
        ensure!(
            capabilities.iter().any(|value| value == "completion"),
            "Choose a text-generation model"
        );
    }
    Ok(size)
}
fn response_budget(context: usize) -> usize {
    (context / 4).min(2048)
}
fn history(request: &ProviderRequest, context: usize) -> Result<Vec<Value>> {
    // A byte-based upper bound reserves generation/template space without relying on a foreign tokenizer.
    let mut budget =
        context.saturating_sub(response_budget(context) + 256 + request.system_prompt.len());
    let mut retained = Vec::new();
    for message in request.messages.iter().rev() {
        let size = message.content.len() + 32;
        if size > budget {
            break;
        }
        budget -= size;
        retained.push(json!({"role":message.role,"content":message.content}));
    }
    ensure!(
        !retained.is_empty(),
        "This message and its project excerpts exceed the local model's context budget. Shorten the message or turn off Project context."
    );
    retained.reverse();
    while retained
        .first()
        .is_some_and(|message| message["role"] != "user")
    {
        retained.remove(0);
    }
    ensure!(
        !retained.is_empty(),
        "Write a message that fits the local model's context window"
    );
    retained.insert(0, json!({"role":"system","content":request.system_prompt}));
    Ok(retained)
}
pub async fn stream(
    request: ProviderRequest,
    sink: ProviderSink,
    cancel: CancellationToken,
    context_limit: Option<usize>,
) -> Result<()> {
    ensure!(
        request.tools.is_empty() && !request.web_search,
        "Ollama Local supports chat and project excerpts"
    );
    ensure!(
        !request.connection.model_id.contains(":cloud")
            && !request.connection.model_id.ends_with("-cloud"),
        "Choose a downloaded local model"
    );
    let response = send(
        client()?
            .post(format!("{BASE}/api/show"))
            .json(&json!({"model":request.connection.model_id})),
        &cancel,
    )
    .await?;
    let details = super::transport::read_json(response, "Ollama Local", &cancel).await?;
    let capacity = context_size(&details)?;
    sink(ProviderEvent::ContextCapacity(capacity));
    let context = context_limit.map_or(capacity, |limit| limit.min(capacity));
    let messages = history(&request, context)?;
    let body = json!({"model":request.connection.model_id,"messages":messages,"stream":true,"options":{"num_ctx":context,"num_predict":response_budget(context)}});
    let response = send(
        client()?.post(format!("{BASE}/api/chat")).json(&body),
        &cancel,
    )
    .await?;
    let mut stream = response.bytes_stream();
    let mut decoder = Decoder::default();
    loop {
        let next = tokio::select! { biased; _ = cancel.cancelled() => bail!("Request cancelled"), next = stream.next() => next };
        match next {
            Some(Ok(bytes)) => {
                if decoder.push(&bytes, &sink)? {
                    return Ok(());
                }
            }
            Some(Err(_)) => bail!("Ollama Local stream interrupted"),
            None => {
                if !decoder.pending.is_empty() && decoder.line(&sink)? {
                    return Ok(());
                }
                bail!("Ollama Local ended before completing the reply");
            }
        }
    }
}
#[derive(Default)]
struct Decoder {
    pending: Vec<u8>,
    output_bytes: usize,
}
impl Decoder {
    fn push(&mut self, bytes: &[u8], sink: &ProviderSink) -> Result<bool> {
        for byte in bytes {
            if *byte == b'\n' {
                if self.line(sink)? {
                    return Ok(true);
                }
            } else {
                ensure!(
                    self.pending.len() < 1024 * 1024,
                    "Ollama Local response chunk is too large"
                );
                self.pending.push(*byte);
            }
        }
        Ok(false)
    }
    fn line(&mut self, sink: &ProviderSink) -> Result<bool> {
        let line = std::mem::take(&mut self.pending);
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(false);
        }
        let value: Value =
            serde_json::from_slice(&line).context("Ollama Local returned an invalid stream")?;
        ensure!(
            value.get("error").is_none(),
            "Ollama Local could not generate this reply. Check the model in Ollama."
        );
        ensure!(
            value["message"]["tool_calls"]
                .as_array()
                .is_none_or(Vec::is_empty),
            "Ollama Local requested an unavailable tool"
        );
        if let Some(thinking) = value["message"]["thinking"]
            .as_str()
            .filter(|text| !text.is_empty())
        {
            self.output_bytes += thinking.len();
            ensure!(
                self.output_bytes <= 1024 * 1024,
                "Ollama Local reply is too large"
            );
            sink(ProviderEvent::Reasoning(thinking.into()));
        }
        if let Some(content) = value["message"]["content"].as_str() {
            self.output_bytes += content.len();
            ensure!(
                self.output_bytes <= 1024 * 1024,
                "Ollama Local reply is too large"
            );
            if !content.is_empty() {
                sink(ProviderEvent::Delta(content.into()));
            }
        }
        if value["done"].as_bool() == Some(true) {
            if let Some(usage) = crate::usage::Usage::from_value(&value) {
                sink(ProviderEvent::Usage(usage));
            }
            ensure!(
                value["done_reason"].as_str() != Some("length"),
                "Ollama Local reached its response limit. Ask it to continue."
            );
            sink(ProviderEvent::Done);
            return Ok(true);
        }
        Ok(false)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn local_stream_handles_fragmented_utf8_and_requires_completion() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let captured = output.clone();
        let sink: ProviderSink = Arc::new(move |event| captured.lock().unwrap().push(event));
        let mut decoder = Decoder::default();
        let bytes = "{\"message\":{\"content\":\"héllo\"},\"done\":false}\n".as_bytes();
        for byte in bytes {
            assert!(!decoder.push(&[*byte], &sink).unwrap());
        }
        assert_eq!(output.lock().unwrap().len(), 1);
        assert!(
            decoder
                .push(b"{\"done\":true,\"done_reason\":\"stop\"}\n", &sink)
                .unwrap()
        );
        assert!(
            Decoder::default()
                .push(b"{\"error\":\"private error\"}\n", &sink)
                .unwrap_err()
                .to_string()
                .find("private")
                .is_none()
        );
        assert!(
            context_size(&json!({"remote_model":"cloud","model_info":{"x.context_length":8192}}))
                .is_err()
        );
        assert_eq!(
            context_size(&json!({"model_info":{"x.context_length":32768}})).unwrap(),
            8192
        );
    }
    #[test]
    fn native_thinking_field_is_separate_even_when_answer_shares_the_chunk() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let sink: ProviderSink = Arc::new(move |event| captured.lock().unwrap().push(event));
        let bytes = "{\"message\":{\"thinking\":\"Compare café options.\",\"content\":\"Answer.\"},\"done\":true}\n".as_bytes();
        let mut decoder = Decoder::default();
        for byte in bytes {
            decoder.push(&[*byte], &sink).unwrap();
        }
        let events = events.lock().unwrap();
        assert!(
            matches!(&events[0], ProviderEvent::Reasoning(text) if text == "Compare café options.")
        );
        assert!(matches!(&events[1], ProviderEvent::Delta(text) if text == "Answer."));
        assert!(matches!(&events[2], ProviderEvent::Done));
        let oversized =
            serde_json::to_vec(&json!({"message":{"thinking":"x".repeat(1_048_577)}})).unwrap();
        assert!(Decoder::default().push(&oversized, &sink).is_err());
    }

    #[test]
    fn local_history_preserves_latest_turn_and_bounds_older_context() {
        let mut request = ProviderRequest {
            connection: Connection {
                provider_id: "ollama-local".into(),
                base_url: BASE.into(),
                model_id: "example".into(),
            },
            system_prompt: "Be helpful".into(),
            messages: vec![
                WireMessage::text("user", "x".repeat(10_000)),
                WireMessage::text("assistant", "Old reply"),
                WireMessage::text("user", "New question"),
            ],
            tools: vec![],
            web_search: false,
        };
        let kept = history(&request, 4096).unwrap();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1]["content"], "New question");
        request.system_prompt = "s".repeat(900);
        assert_eq!(
            history(&request, 2048).unwrap()[1]["content"],
            "New question"
        );
        assert_eq!(response_budget(2048), 512);
        assert_eq!(response_budget(8192), 2048);
        request.messages.last_mut().unwrap().content = "x".repeat(4096);
        assert!(history(&request, 2048).is_err());
    }
}
