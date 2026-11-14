use crate::models::{ProviderEvent, ProviderSink, Source, ToolCall};
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const MAX_EVENT: usize = 1024 * 1024;
const MAX_STREAM: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(crate) enum Protocol {
    Compatible,
    Responses,
    Messages,
    Interactions,
}
#[derive(Default)]
struct Frame {
    event: String,
    data: String,
}
#[derive(Default)]
struct Framer {
    line: Vec<u8>,
    frame: Frame,
    first_line: bool,
    skip_lf: bool,
}
impl Framer {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Frame>> {
        let mut frames = vec![];
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            if byte == b'\r' || byte == b'\n' {
                self.skip_lf = byte == b'\r';
                let line = std::str::from_utf8(&self.line)
                    .map_err(|_| anyhow::anyhow!("Invalid stream encoding"))?;
                let line = if !self.first_line {
                    self.first_line = true;
                    line.trim_start_matches('\u{feff}')
                } else {
                    line
                };
                if line.is_empty() {
                    if !self.frame.data.is_empty() {
                        self.frame.data.pop();
                        frames.push(std::mem::take(&mut self.frame));
                    } else {
                        self.frame = Frame::default();
                    }
                } else if !line.starts_with(':') {
                    let (name, value) = line.split_once(':').unwrap_or((line, ""));
                    let value = value.strip_prefix(' ').unwrap_or(value);
                    match name {
                        "data" => {
                            self.frame.data.push_str(value);
                            self.frame.data.push('\n');
                        }
                        "event" => self.frame.event = value.into(),
                        _ => {}
                    }
                }
                self.line.clear();
            } else {
                self.line.push(byte);
            }
            ensure!(
                self.line.len() + self.frame.data.len() <= MAX_EVENT,
                "Provider stream event exceeds size limit"
            );
        }
        Ok(frames)
    }
}

#[derive(Default)]
struct CallParts {
    id: String,
    name: String,
    arguments: String,
}
pub(crate) struct Decoder {
    protocol: Protocol,
    provider: String,
    framing: Framer,
    done: bool,
    stop: bool,
    bytes: usize,
    calls: BTreeMap<usize, CallParts>,
    native: BTreeMap<usize, Value>,
    server_inputs: BTreeMap<usize, String>,
    reasoning: String,
    reasoning_details: BTreeMap<usize, Value>,
    sources: HashSet<String>,
}
impl Decoder {
    pub(crate) fn new(protocol: Protocol, provider: &str) -> Self {
        Self {
            protocol,
            provider: provider.into(),
            framing: Framer::default(),
            done: false,
            stop: false,
            bytes: 0,
            calls: BTreeMap::new(),
            native: BTreeMap::new(),
            server_inputs: BTreeMap::new(),
            reasoning: String::new(),
            reasoning_details: BTreeMap::new(),
            sources: HashSet::new(),
        }
    }
    pub(crate) fn push(&mut self, bytes: &[u8], sink: &ProviderSink) -> Result<bool> {
        if self.done {
            return Ok(true);
        }
        self.bytes += bytes.len();
        ensure!(
            self.bytes <= MAX_STREAM,
            "{} stream exceeds size limit",
            self.provider
        );
        for frame in self.framing.push(bytes)? {
            if frame.data.trim() == "[DONE]" {
                if matches!(self.protocol, Protocol::Compatible) {
                    ensure!(
                        self.stop,
                        "{} stream ended without a finish reason",
                        self.provider
                    );
                    self.complete(sink)?;
                } else {
                    bail!(
                        "{} stream ended without its native completion event",
                        self.provider
                    );
                }
            } else {
                let value: Value = serde_json::from_str(&frame.data)
                    .map_err(|_| anyhow::anyhow!("{} sent malformed stream data", self.provider))?;
                ensure!(
                    value.is_object(),
                    "{} sent invalid stream data",
                    self.provider
                );
                if !value["error"].is_null() {
                    bail!("{} stream failed", self.provider);
                }
                let kind = value["type"]
                    .as_str()
                    .or_else(|| value["event_type"].as_str())
                    .unwrap_or(&frame.event)
                    .to_owned();
                ensure!(
                    kind != "error" && frame.event != "error",
                    "{} stream failed",
                    self.provider
                );
                self.citations(&value, sink);
                match self.protocol {
                    Protocol::Compatible => self.compatible(&value, sink)?,
                    Protocol::Responses => self.responses(&kind, &value, sink)?,
                    Protocol::Messages => self.messages(&kind, &value, sink)?,
                    Protocol::Interactions => self.interactions(&kind, &value, sink)?,
                }
            }
            if self.done {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn finish(&self) -> Result<()> {
        ensure!(
            self.done,
            "{} stream interrupted before completion",
            self.provider
        );
        Ok(())
    }
    fn compatible(&mut self, value: &Value, sink: &ProviderSink) -> Result<()> {
        if let Some(choices) = value["choices"].as_array() {
            for choice in choices {
                ensure!(
                    choice["index"].as_u64().unwrap_or(0) == 0,
                    "Unexpected multiple response choices"
                );
                let delta = &choice["delta"];
                if let Some(text) = delta["content"].as_str() {
                    sink(ProviderEvent::Delta(text.into()));
                }
                append(&mut self.reasoning, &delta["reasoning_content"]);
                if delta["reasoning_content"].is_null() {
                    append(&mut self.reasoning, &delta["reasoning"]);
                }
                if let Some(details) = delta["reasoning_details"].as_array() {
                    for detail in details {
                        let index = if detail.get("index").is_some_and(|v| !v.is_null()) {
                            index(detail, "index")?
                        } else {
                            let existing = detail["id"].as_str().and_then(|id| {
                                self.reasoning_details
                                    .iter()
                                    .find(|(_, prior)| prior["id"] == id)
                                    .map(|(index, _)| *index)
                            });
                            existing.unwrap_or_else(|| {
                                self.reasoning_details
                                    .last_key_value()
                                    .map_or(0, |(index, _)| index + 1)
                            })
                        };
                        ensure!(index < 128, "Too many reasoning continuation blocks");
                        let target = self
                            .reasoning_details
                            .entry(index)
                            .or_insert_with(|| json!({}));
                        let fields = detail
                            .as_object()
                            .ok_or_else(|| anyhow::anyhow!("Invalid reasoning continuation"))?;
                        for (key, value) in fields {
                            if matches!(key.as_str(), "text" | "data" | "summary" | "signature")
                                && value.is_string()
                            {
                                append_value(target, key, value.as_str().unwrap());
                            } else {
                                target[key] = value.clone();
                            }
                        }
                    }
                }
                if let Some(calls) = delta["tool_calls"].as_array() {
                    for call in calls {
                        let index = index(call, "index")?;
                        let parts = self.calls.entry(index).or_default();
                        append(&mut parts.id, &call["id"]);
                        append(&mut parts.name, &call["function"]["name"]);
                        append(&mut parts.arguments, &call["function"]["arguments"]);
                    }
                }
                if let Some(reason) = choice["finish_reason"].as_str() {
                    ensure!(
                        matches!(reason, "stop" | "tool_calls"),
                        "{} response did not complete",
                        self.provider
                    );
                    self.stop = true;
                }
            }
        }
        Ok(())
    }
    fn responses(&mut self, kind: &str, value: &Value, sink: &ProviderSink) -> Result<()> {
        match kind {
            "response.output_text.delta" => {
                if let Some(text) = value["delta"].as_str() {
                    sink(ProviderEvent::Delta(text.into()));
                }
            }
            "response.output_item.added" | "response.output_item.done" => {
                if kind == "response.output_item.added"
                    && value["item"]["type"] == "web_search_call"
                {
                    sink(ProviderEvent::Activity("Searching the web".into()));
                }
                self.response_item(index(value, "output_index")?, &value["item"])?;
            }
            "response.function_call_arguments.delta" => {
                append(
                    &mut self
                        .calls
                        .entry(index(value, "output_index")?)
                        .or_default()
                        .arguments,
                    &value["delta"],
                );
            }
            "response.function_call_arguments.done" => {
                replace(
                    &mut self
                        .calls
                        .entry(index(value, "output_index")?)
                        .or_default()
                        .arguments,
                    &value["arguments"],
                );
            }
            "response.completed" => {
                if let Some(status) = value["response"]["status"].as_str() {
                    ensure!(
                        status == "completed",
                        "{} response did not complete",
                        self.provider
                    );
                }
                if let Some(items) = value["response"]["output"].as_array() {
                    for (index, item) in items.iter().enumerate() {
                        self.response_item(index, item)?;
                    }
                }
                self.complete(sink)?;
            }
            "response.failed" | "response.incomplete" | "error" => {
                bail!("{} response did not complete", self.provider)
            }
            _ => {}
        }
        Ok(())
    }
    fn response_item(&mut self, index: usize, item: &Value) -> Result<()> {
        ensure!(index < 128, "Too many response items");
        ensure!(item.is_object(), "Invalid response item");
        if item["type"] == "function_call" {
            let call = self.calls.entry(index).or_default();
            replace(&mut call.id, &item["call_id"]);
            replace(&mut call.name, &item["name"]);
            if item["arguments"].as_str().is_some_and(|s| !s.is_empty()) {
                replace(&mut call.arguments, &item["arguments"]);
            }
        }
        self.native.insert(index, item.clone());
        Ok(())
    }
    fn messages(&mut self, kind: &str, value: &Value, sink: &ProviderSink) -> Result<()> {
        match kind {
            "content_block_start" => {
                let index = index(value, "index")?;
                let block = &value["content_block"];
                ensure!(block.is_object(), "Invalid content block");
                if block["type"] == "server_tool_use" {
                    ensure!(
                        block["name"] == "web_search",
                        "Unsupported provider server tool"
                    );
                    sink(ProviderEvent::Activity("Searching the web".into()));
                }
                ensure!(
                    block["content"]["type"] != "web_search_tool_result_error",
                    "{} native search failed",
                    self.provider
                );
                if block["type"] == "tool_use" {
                    self.calls.insert(
                        index,
                        CallParts {
                            id: required(block, "id")?,
                            name: required(block, "name")?,
                            arguments: String::new(),
                        },
                    );
                }
                if block["type"] == "text"
                    && let Some(text) = block["text"].as_str().filter(|s| !s.is_empty())
                {
                    sink(ProviderEvent::Delta(text.into()));
                }
                self.native.insert(index, block.clone());
            }
            "content_block_delta" => {
                let index = index(value, "index")?;
                let delta = &value["delta"];
                let block = self
                    .native
                    .get_mut(&index)
                    .ok_or_else(|| anyhow::anyhow!("Invalid content block order"))?;
                match delta["type"].as_str().unwrap_or("") {
                    "text_delta" => {
                        if let Some(text) = delta["text"].as_str() {
                            sink(ProviderEvent::Delta(text.into()));
                            append_value(block, "text", text);
                        }
                    }
                    "input_json_delta" => {
                        let partial = delta["partial_json"]
                            .as_str()
                            .ok_or_else(|| anyhow::anyhow!("Invalid streamed tool arguments"))?;
                        if let Some(call) = self.calls.get_mut(&index) {
                            call.arguments.push_str(partial);
                        } else if block["type"] == "server_tool_use" {
                            self.server_inputs
                                .entry(index)
                                .or_default()
                                .push_str(partial);
                        } else {
                            bail!("Invalid streamed tool arguments");
                        }
                    }
                    "thinking_delta" => {
                        if let Some(text) = delta["thinking"].as_str() {
                            append_value(block, "thinking", text);
                        }
                    }
                    "signature_delta" => {
                        if let Some(text) = delta["signature"].as_str() {
                            append_value(block, "signature", text);
                        }
                    }
                    "citations_delta" => {
                        if block["citations"].is_null() {
                            block["citations"] = json!([]);
                        }
                        if let Some(items) = block["citations"].as_array_mut() {
                            items.push(delta["citation"].clone());
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(reason) = value["delta"]["stop_reason"].as_str() {
                    ensure!(
                        matches!(reason, "end_turn" | "tool_use" | "stop_sequence"),
                        "{} response did not complete",
                        self.provider
                    );
                    self.stop = true;
                }
            }
            "message_stop" => {
                ensure!(self.stop, "{} stream lacks a stop reason", self.provider);
                self.complete(sink)?;
            }
            "error" => bail!("{} stream failed", self.provider),
            _ => {}
        }
        Ok(())
    }
    fn interactions(&mut self, kind: &str, value: &Value, sink: &ProviderSink) -> Result<()> {
        match kind {
            "step.start" => {
                let index = index(value, "index")?;
                let step = &value["step"];
                ensure!(step.is_object(), "Invalid interaction step");
                if step["type"] == "google_search_call" {
                    sink(ProviderEvent::Activity("Searching the web".into()));
                }
                if step["type"] == "function_call" {
                    self.calls.insert(
                        index,
                        CallParts {
                            id: required(step, "id")?,
                            name: required(step, "name")?,
                            arguments: String::new(),
                        },
                    );
                }
                self.native.insert(index, step.clone());
            }
            "step.delta" => {
                let index = index(value, "index")?;
                let delta = &value["delta"];
                let step = self
                    .native
                    .get_mut(&index)
                    .ok_or_else(|| anyhow::anyhow!("Invalid interaction step order"))?;
                match delta["type"].as_str().unwrap_or("") {
                    "text" => {
                        if let Some(text) = delta["text"].as_str()
                            && step["type"] == "model_output"
                        {
                            sink(ProviderEvent::Delta(text.into()));
                            if step["content"].is_null() {
                                step["content"] = json!([{ "type":"text", "text":"" }]);
                            }
                            if let Some(first) =
                                step["content"].as_array_mut().and_then(|a| a.last_mut())
                            {
                                append_value(first, "text", text);
                            }
                        }
                    }
                    "arguments_delta" => {
                        if let Some(call) = self.calls.get_mut(&index) {
                            append(&mut call.arguments, &delta["arguments"]);
                        }
                    }
                    "thought_signature" => {
                        if let Some(signature) = delta["signature"].as_str() {
                            append_value(step, "signature", signature);
                        }
                    }
                    "thought_summary" => {
                        if step["summary"].is_null() {
                            step["summary"] = json!([]);
                        }
                        if let Some(summary) = step["summary"].as_array_mut() {
                            summary.push(delta["content"].clone());
                        }
                    }
                    "google_search_call" | "google_search_result" => {
                        ensure!(
                            delta["is_error"] != true,
                            "{} native search failed",
                            self.provider
                        );
                        if let Some(object) = delta.as_object() {
                            for (key, value) in object {
                                if key != "type" {
                                    step[key] = value.clone();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            "interaction.completed" => {
                let status = value["interaction"]["status"].as_str().unwrap_or("");
                ensure!(
                    status == "completed"
                        || (status == "requires_action" && !self.calls.is_empty()),
                    "{} interaction did not complete",
                    self.provider
                );
                self.complete(sink)?;
            }
            "error" | "interaction.failed" => bail!("{} stream failed", self.provider),
            _ => {}
        }
        Ok(())
    }
    fn complete(&mut self, sink: &ProviderSink) -> Result<()> {
        for (index, input) in &self.server_inputs {
            let args: Value = serde_json::from_str(input).map_err(|_| {
                anyhow::anyhow!("{} sent invalid server tool arguments", self.provider)
            })?;
            ensure!(
                args.is_object(),
                "{} sent invalid server tool arguments",
                self.provider
            );
            self.native
                .get_mut(index)
                .ok_or_else(|| anyhow::anyhow!("Invalid server tool order"))?["input"] = args;
        }
        ensure!(
            self.calls.len() <= 16,
            "{} requested too many tools",
            self.provider
        );
        let mut calls = vec![];
        let mut ids = HashSet::new();
        for (index, parts) in &self.calls {
            ensure!(
                !parts.id.is_empty()
                    && parts.id.len() <= 512
                    && !parts.name.is_empty()
                    && parts.name.len() <= 128,
                "{} sent an incomplete tool call",
                self.provider
            );
            ensure!(
                ids.insert(parts.id.clone()),
                "{} sent duplicate tool call IDs",
                self.provider
            );
            let args: Value = if parts.arguments.is_empty() {
                match self.protocol {
                    Protocol::Messages => self.native[index]["input"].clone(),
                    Protocol::Interactions => self.native[index]["arguments"].clone(),
                    _ => json!({}),
                }
            } else {
                serde_json::from_str(&parts.arguments)
                    .map_err(|_| anyhow::anyhow!("{} sent invalid tool arguments", self.provider))?
            };
            ensure!(
                args.is_object(),
                "{} sent invalid tool arguments",
                self.provider
            );
            if let Some(native) = self.native.get_mut(index) {
                match self.protocol {
                    Protocol::Messages => native["input"] = args.clone(),
                    Protocol::Interactions => native["arguments"] = args.clone(),
                    Protocol::Responses => native["arguments"] = json!(parts.arguments),
                    _ => {}
                }
            }
            calls.push(ToolCall {
                id: parts.id.clone(),
                name: parts.name.clone(),
                arguments: args,
                provider_data: None,
            });
        }
        let metadata = match self.protocol {
            Protocol::Compatible
                if !self.reasoning.is_empty() || !self.reasoning_details.is_empty() =>
            {
                let mut data = json!({"protocol":"chat-completions"});
                if !self.reasoning.is_empty() {
                    data["reasoning_content"] = json!(self.reasoning);
                }
                if !self.reasoning_details.is_empty() {
                    data["reasoning_details"] =
                        json!(self.reasoning_details.values().collect::<Vec<_>>());
                }
                Some(data)
            }
            Protocol::Responses => Some(
                json!({"protocol":"responses","output":self.native.values().collect::<Vec<_>>()}),
            ),
            Protocol::Messages => Some(
                json!({"protocol":"messages","content":self.native.values().collect::<Vec<_>>()}),
            ),
            Protocol::Interactions => Some(
                json!({"protocol":"interactions","steps":self.native.values().collect::<Vec<_>>()}),
            ),
            _ => None,
        };
        if let Some(first) = calls.first_mut() {
            first.provider_data = metadata;
        }
        for call in calls {
            sink(ProviderEvent::ToolCall(call));
        }
        self.done = true;
        sink(ProviderEvent::Done);
        Ok(())
    }
    fn citations(&mut self, value: &Value, sink: &ProviderSink) {
        let mut sources = vec![];
        collect_citations(value, &mut sources);
        sources.retain(|s| self.sources.insert(s.url.clone().unwrap_or_default()));
        if !sources.is_empty() {
            sink(ProviderEvent::Sources(sources));
        }
    }
}

fn index(value: &Value, key: &str) -> Result<usize> {
    let index = value[key]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Stream item is missing an index"))?;
    ensure!(index < 128, "Stream item index exceeds limit");
    Ok(index as usize)
}
fn required(value: &Value, key: &str) -> Result<String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Stream item is missing required data"))
}
fn append(target: &mut String, value: &Value) {
    if let Some(text) = value.as_str() {
        target.push_str(text);
    }
}
fn replace(target: &mut String, value: &Value) {
    if let Some(text) = value.as_str() {
        *target = text.into();
    }
}
fn append_value(value: &mut Value, key: &str, text: &str) {
    let mut prior = value[key].as_str().unwrap_or("").to_owned();
    prior.push_str(text);
    value[key] = json!(prior);
}
pub(crate) fn source(url: &str, title: &str, snippet: &str) -> Option<Source> {
    let url = super::public_url(url).ok()?.to_string();
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    Some(Source {
        id: format!("web-{}", &hash[..20]),
        kind: "web".into(),
        title: title.chars().take(256).collect(),
        url: Some(url),
        snippet: snippet.chars().take(600).collect(),
        ..Source::default()
    })
}
fn collect_citations(value: &Value, sources: &mut Vec<Source>) {
    if sources.len() >= 32 {
        return;
    }
    match value {
        Value::Object(object) => {
            if matches!(
                value["type"].as_str(),
                Some("url_citation" | "web_search_result_location" | "web_search_result")
            ) && let Some(url) = value["url"].as_str()
                && let Some(source) = source(
                    url,
                    value["title"].as_str().unwrap_or("Web source"),
                    value["cited_text"].as_str().unwrap_or(""),
                )
            {
                sources.push(source);
            }
            for child in object.values() {
                collect_citations(child, sources);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_citations(child, sources);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
