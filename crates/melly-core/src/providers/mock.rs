use super::transport::check_cancel;
use crate::models::{ProviderEvent, ProviderRequest, ProviderSink, Source, ToolCall};
use anyhow::Result;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

pub(crate) async fn stream(
    input: ProviderRequest,
    sink: ProviderSink,
    cancel: CancellationToken,
) -> Result<()> {
    check_cancel(&cancel)?;
    let last_user = input
        .messages
        .iter()
        .rposition(|m| m.role == "user")
        .unwrap_or(0);
    let results: Vec<_> = input
        .messages
        .iter()
        .skip(last_user + 1)
        .filter(|m| m.role == "tool")
        .collect();
    let question = input
        .messages
        .get(last_user)
        .map(|m| m.content.as_str())
        .unwrap_or("");
    let lower = question.to_lowercase();
    let slow_demo =
        lower.contains("demonstrate streaming and stop") || lower.contains("write a longer answer");
    if results.is_empty() {
        let tools = super::protocol::flat_tools(&input.tools)?;
        let query = ["interview", "deadline", "budget", "application", "project"]
            .into_iter()
            .find(|word| lower.contains(word))
            .map(str::to_owned)
            .unwrap_or_else(|| question.chars().take(200).collect());
        for tool in tools {
            check_cancel(&cancel)?;
            let name = tool["name"].as_str().unwrap_or("");
            // Offered tools are already permission-checked by Rust's run controller.
            sink(ProviderEvent::ToolCall(ToolCall {
                id: format!("mock-{}", name.replace('_', "-")),
                name: name.into(),
                arguments: json!({"query":query}),
                provider_data: None,
            }));
        }
        if !input.tools.is_empty() {
            check_cancel(&cancel)?;
            sink(ProviderEvent::Done);
            return Ok(());
        }
    }
    let mut sources = vec![];
    for message in &results {
        if let Ok(value) = serde_json::from_str::<Value>(&message.content) {
            read_sources(&value, &mut sources);
        }
    }
    sources.truncate(5);
    let mut answer = if !sources.is_empty() {
        format!(
            "Mock reply — based on the retrieved references:\n\n{}",
            sources
                .iter()
                .enumerate()
                .map(|(i, source)| format!("{} [{}]", source.snippet, i + 1))
                .collect::<Vec<_>>()
                .join("\n\n")
        )
    } else if !results.is_empty() {
        "Mock reply — the requested search returned no matching references. I don't have a retrieved fact to cite.".into()
    } else {
        "Mock reply — this is a synthetic conversation for trying Melly. Enable folder context to retrieve a saved reference, or enable web search to exercise the search tool. No cloud model or account is being used.".into()
    };
    if slow_demo {
        answer.push_str("\n\nMock streaming and Stop demonstration — this synthetic reply is deliberately paced for about three seconds. Select Stop at any time to keep the text already received.\n\nImagine a quiet afternoon spent planning a small project. Start with one useful outcome and write it down in a sentence. Leave enough room to change the plan as you learn.\n\nNext, choose the smallest step you can finish today. A short outline, a question for a friend, or a list of materials may be enough to make progress.\n\nPause to notice what is already working. Keep the helpful parts, simplify the confusing parts, and give yourself a clear place to begin tomorrow.\n\nThis is still a Mock reply. It makes no live provider request and does not represent a cloud model's speed, capabilities, or account usage.");
    }
    if !sources.is_empty() {
        check_cancel(&cancel)?;
        sink(ProviderEvent::Sources(sources));
    }
    let characters: Vec<_> = answer.chars().collect();
    let demo_delay =
        std::time::Duration::from_millis(3000 / characters.len().div_ceil(48).max(1) as u64);
    for chunk in characters.chunks(48) {
        check_cancel(&cancel)?;
        sink(ProviderEvent::Delta(chunk.iter().collect()));
        if slow_demo {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => { check_cancel(&cancel)?; },
                _ = tokio::time::sleep(demo_delay) => {},
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
    check_cancel(&cancel)?;
    sink(ProviderEvent::Done);
    Ok(())
}

fn read_sources(value: &Value, output: &mut Vec<Source>) {
    if output.len() >= 5 {
        return;
    }
    if let Some(sources) = value
        .get("sources")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
    {
        for candidate in sources.iter().take(5) {
            if let Ok(mut source) = serde_json::from_value::<Source>(candidate.clone()) {
                if source.kind == "web"
                    && source
                        .url
                        .as_ref()
                        .is_none_or(|u| super::public_url(u).is_err())
                {
                    continue;
                }
                source.snippet = source.snippet.chars().take(600).collect();
                if !output.iter().any(|s| s.id == source.id) {
                    output.push(source);
                }
            }
        }
    }
}
