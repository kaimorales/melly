use std::collections::{HashMap, VecDeque};

use serde_json::{Value, json};

const HISTORY_PAGE_SIZE: usize = 40;
const MAX_ENTRIES: usize = 400;
const MAX_TOOL_CHARS: usize = 12_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    You,
    Codex,
    Tool,
    Notice,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Self::You => "You",
            Self::Codex => "Codex",
            Self::Tool => "Tool",
            Self::Notice => "Session",
        }
    }
}

pub struct Entry {
    pub speaker: Speaker,
    pub text: String,
    id: Option<String>,
    complete: bool,
}

pub struct Approval {
    pub description: String,
    pub can_accept: bool,
    pub id: Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    Account,
    Models,
    Thread,
    History,
    Start,
    Steer,
    Interrupt,
}

pub struct Session {
    pub cwd: String,
    pub model: Option<String>,
    pub thread_id: Option<String>,
    pub entries: VecDeque<Entry>,
    pub status: String,
    pub account: Option<String>,
    pub tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub approvals: VecDeque<Approval>,
    requested_model: Option<String>,
    resume_id: Option<String>,
    pending: HashMap<u64, Pending>,
    next_id: u64,
    outbox: Vec<Value>,
    turn_id: Option<String>,
    last_finished_turn: Option<String>,
    older_cursor: Option<String>,
    patches: HashMap<String, String>,
    interrupt_requested: bool,
    stopped: bool,
}

impl Session {
    pub fn new(cwd: String, model: Option<String>, resume: Option<String>) -> Self {
        let mut session = Self {
            cwd,
            model: None,
            thread_id: None,
            entries: VecDeque::new(),
            status: "Connecting to Codex".into(),
            account: None,
            tokens: None,
            context_window: None,
            approvals: VecDeque::new(),
            requested_model: model,
            resume_id: resume,
            pending: HashMap::new(),
            next_id: 1,
            outbox: Vec::new(),
            turn_id: None,
            last_finished_turn: None,
            older_cursor: None,
            patches: HashMap::new(),
            interrupt_requested: false,
            stopped: false,
        };
        session.request(
            Pending::Initialize,
            "initialize",
            json!({
                "clientInfo": {
                    "name": "melly",
                    "title": "Melly",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        );
        session
    }

    fn request(&mut self, purpose: Pending, method: &str, params: Value) {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, purpose);
        self.outbox
            .push(json!({"id": id, "method": method, "params": params}));
    }

    pub fn drain_requests(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.outbox)
    }

    pub fn submit(&mut self, prompt: &str) -> bool {
        let Some(thread_id) = self.thread_id.clone() else {
            return false;
        };
        if self.stopped || prompt.trim().is_empty() {
            return false;
        }

        let input = json!([{"type": "text", "text": prompt}]);
        if let Some(turn_id) = &self.turn_id {
            self.request(
                Pending::Steer,
                "turn/steer",
                json!({
                    "threadId": thread_id, "expectedTurnId": turn_id, "input": input
                }),
            );
        } else if self
            .pending
            .values()
            .any(|pending| *pending == Pending::Start)
        {
            return false;
        } else {
            self.request(
                Pending::Start,
                "turn/start",
                json!({
                    "threadId": thread_id, "input": input
                }),
            );
            self.status = "Sending".into();
        }
        self.push(Entry::complete(Speaker::You, prompt));
        true
    }

    pub fn interrupt(&mut self) -> bool {
        if self
            .pending
            .values()
            .any(|pending| *pending == Pending::Interrupt)
        {
            return true;
        }
        let (Some(thread_id), Some(turn_id)) = (&self.thread_id, &self.turn_id) else {
            if self
                .pending
                .values()
                .any(|pending| *pending == Pending::Start)
            {
                self.interrupt_requested = true;
                self.status = "Interrupt pending".into();
                return true;
            }
            return false;
        };
        self.request(
            Pending::Interrupt,
            "turn/interrupt",
            json!({
                "threadId": thread_id, "turnId": turn_id
            }),
        );
        self.status = "Interrupting".into();
        true
    }

    pub fn approve(&mut self, accept: bool) {
        if accept
            && self
                .approvals
                .front()
                .is_some_and(|approval| !approval.can_accept)
        {
            return;
        }
        if let Some(approval) = self.approvals.pop_front() {
            let decision = if accept { "accept" } else { "decline" };
            self.outbox
                .push(json!({"id": approval.id, "result": {"decision": decision}}));
        }
    }

    pub fn disconnected(&mut self, message: &str) {
        self.stopped = true;
        self.turn_id = None;
        self.pending.clear();
        self.approvals.clear();
        self.outbox.clear();
        self.status = "Disconnected".into();
        self.push(Entry::complete(Speaker::Notice, message));
    }

    pub fn receive(&mut self, message: Value) {
        match (
            message.get("method").and_then(Value::as_str),
            message.get("id"),
        ) {
            (Some(method), Some(id)) => self.server_request(method, id, &message["params"]),
            (Some(method), None) => self.notification(method, &message["params"]),
            (None, Some(id)) => {
                if let Some(purpose) = id.as_u64().and_then(|id| self.pending.remove(&id)) {
                    self.response(purpose, &message);
                }
            }
            (None, None) => {}
        }
    }

    fn response(&mut self, purpose: Pending, message: &Value) {
        if let Some(error) = message["error"]["message"].as_str() {
            match purpose {
                Pending::Initialize | Pending::Thread => self.disconnected(error),
                Pending::Models => {
                    self.push(Entry::complete(Speaker::Notice, error));
                    self.open_thread(false);
                }
                Pending::Start | Pending::Interrupt => {
                    self.interrupt_requested = false;
                    self.status = if self.turn_id.is_some() {
                        "Running"
                    } else {
                        "Ready"
                    }
                    .into();
                    self.push(Entry::complete(Speaker::Notice, error));
                }
                _ => self.push(Entry::complete(Speaker::Notice, error)),
            }
            return;
        }

        let result = &message["result"];
        match purpose {
            Pending::Initialize => {
                self.outbox.push(json!({"method": "initialized"}));
                self.status = "Loading Codex session".into();
                self.request(
                    Pending::Account,
                    "account/read",
                    json!({"refreshToken": false}),
                );
                self.request(Pending::Models, "model/list", json!({"limit": 100}));
            }
            Pending::Account => {
                self.account = owned_string(&result["account"]["type"]);
                if self.account.is_none() && result["requiresOpenaiAuth"] == true {
                    self.push(Entry::complete(
                        Speaker::Notice,
                        "Run codex login, then reopen Melly.",
                    ));
                }
            }
            Pending::Models => {
                let sol_available = result["data"].as_array().is_some_and(|models| {
                    models.iter().any(|model| model["model"] == "gpt-6.1-sol")
                });
                self.open_thread(sol_available);
            }
            Pending::Thread => {
                self.thread_id = owned_string(&result["thread"]["id"]);
                if self.thread_id.is_none() {
                    self.disconnected("Codex did not return a thread ID.");
                    return;
                }
                self.model = owned_string(&result["model"]);
                if let Some(cwd) = owned_string(&result["cwd"]) {
                    self.cwd = cwd;
                }
                self.status = "Ready".into();
                if self.resume_id.is_some() {
                    self.request_history(None);
                }
            }
            Pending::History => {
                self.older_cursor = owned_string(&result["nextCursor"]);
                if let Some(rows) = result["data"].as_array() {
                    for row in rows {
                        if let Some(entry) = item_entry(&row["item"])
                            && !self.entries.iter().any(|prior| prior.id == entry.id)
                        {
                            self.entries.push_front(entry);
                        }
                    }
                    self.entries.truncate(MAX_ENTRIES);
                }
            }
            Pending::Start => {
                if let Some(id) = result["turn"]["id"].as_str() {
                    self.mark_running(id);
                } else {
                    self.status = "Ready".into();
                    self.push(Entry::complete(
                        Speaker::Notice,
                        "Codex did not return a turn ID.",
                    ));
                }
            }
            Pending::Steer | Pending::Interrupt => {}
        }
    }

    fn open_thread(&mut self, sol_available: bool) {
        let mut params = json!({});
        if let Some(model) = &self.requested_model {
            params["model"] = json!(model);
        }
        if let Some(id) = &self.resume_id {
            params["threadId"] = json!(id);
            params["excludeTurns"] = json!(true);
            self.request(Pending::Thread, "thread/resume", params);
        } else {
            params["cwd"] = json!(self.cwd);
            if self.requested_model.is_none() && sol_available {
                params["model"] = json!("gpt-6.1-sol");
            }
            self.request(Pending::Thread, "thread/start", params);
        }
    }

    fn request_history(&mut self, cursor: Option<String>) {
        self.request(
            Pending::History,
            "thread/items/list",
            json!({
                "threadId": self.thread_id, "limit": HISTORY_PAGE_SIZE,
                "sortDirection": "desc", "cursor": cursor,
            }),
        );
    }

    pub fn load_older(&mut self) {
        if self.entries.len() >= MAX_ENTRIES - HISTORY_PAGE_SIZE
            || self
                .pending
                .values()
                .any(|pending| *pending == Pending::History)
        {
            return;
        }
        if let Some(cursor) = self.older_cursor.take() {
            self.request_history(Some(cursor));
        }
    }

    fn mark_running(&mut self, id: &str) {
        if self.last_finished_turn.as_deref() != Some(id) {
            self.turn_id = Some(id.to_owned());
            if self.interrupt_requested {
                self.interrupt_requested = false;
                self.interrupt();
            } else if !self
                .pending
                .values()
                .any(|pending| *pending == Pending::Interrupt)
            {
                self.status = "Running".into();
            }
        }
    }

    fn notification(&mut self, method: &str, params: &Value) {
        if self.thread_id.is_none() || params["threadId"].as_str() != self.thread_id.as_deref() {
            return;
        }

        match method {
            "turn/started" => {
                if let Some(id) = params["turn"]["id"].as_str() {
                    self.mark_running(id);
                }
            }
            "turn/completed" => {
                let turn = &params["turn"];
                let Some(id) = turn["id"].as_str() else {
                    return;
                };
                if self.turn_id.as_deref().is_some_and(|current| current != id) {
                    return;
                }
                self.last_finished_turn = Some(id.to_owned());
                self.turn_id = None;
                self.interrupt_requested = false;
                self.approvals.clear();
                self.patches.clear();
                self.status = match turn["status"].as_str() {
                    Some("interrupted") => "Interrupted",
                    Some("failed") => "Turn failed",
                    _ => "Ready",
                }
                .into();
                if let Some(error) = turn["error"]["message"].as_str() {
                    self.push(Entry::complete(Speaker::Notice, error));
                }
            }
            "item/started" | "item/completed" => {
                let item = &params["item"];
                if item["type"] == "userMessage" {
                    return;
                }
                if item["type"] == "fileChange"
                    && let Some(id) = item["id"].as_str()
                {
                    if method == "item/completed" {
                        self.patches.remove(id);
                    } else if let Some(preview) = patch_preview(item) {
                        self.patches.insert(id.to_owned(), preview);
                    }
                }
                if let Some(mut entry) = item_entry(item) {
                    entry.complete = method == "item/completed";
                    if let Some(prior) = self.entries.iter_mut().find(|prior| prior.id == entry.id)
                    {
                        if entry.complete || prior.text.is_empty() {
                            if item["type"] == "commandExecution"
                                && !item["aggregatedOutput"].is_string()
                            {
                                entry.text.clone_from(&prior.text);
                            }
                            *prior = entry;
                        }
                    } else {
                        self.push(entry);
                    }
                }
            }
            "item/agentMessage/delta" | "item/commandExecution/outputDelta" => {
                let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str())
                else {
                    return;
                };
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.id.as_deref() == Some(id))
                {
                    if !entry.complete {
                        if entry.speaker == Speaker::Tool && !entry.text.contains('\n') {
                            entry.text.push('\n');
                        }
                        entry.text.push_str(delta);
                        if entry.speaker == Speaker::Tool {
                            truncate_tool(&mut entry.text);
                        }
                    }
                } else {
                    let speaker = if method == "item/agentMessage/delta" {
                        Speaker::Codex
                    } else {
                        Speaker::Tool
                    };
                    self.push(Entry {
                        speaker,
                        text: delta.to_owned(),
                        id: Some(id.to_owned()),
                        complete: false,
                    });
                }
            }
            "thread/tokenUsage/updated" => {
                self.tokens = params["tokenUsage"]["total"]["totalTokens"].as_u64();
                self.context_window = params["tokenUsage"]["modelContextWindow"].as_u64();
            }
            "model/rerouted" => self.model = owned_string(&params["toModel"]),
            "serverRequest/resolved" => self
                .approvals
                .retain(|approval| approval.id != params["requestId"]),
            _ => {}
        }
    }

    fn server_request(&mut self, method: &str, id: &Value, params: &Value) {
        let supported = matches!(
            method,
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
        );
        if !supported
            || self.thread_id.is_none()
            || params["threadId"].as_str() != self.thread_id.as_deref()
        {
            self.outbox.push(json!({"id": id, "error": {"code": -32601, "message": "Melly cannot handle this request"}}));
            return;
        }

        if self.approvals.iter().any(|approval| approval.id == *id) {
            return;
        }
        let action = if method == "item/fileChange/requestApproval" {
            params["itemId"]
                .as_str()
                .and_then(|id| self.patches.get(id))
                .cloned()
        } else if params["kind"].is_null() || params["kind"] == "command" {
            if let Some(host) = params["networkApprovalContext"]["host"].as_str() {
                let protocol = params["networkApprovalContext"]["protocol"]
                    .as_str()
                    .unwrap_or("network");
                Some(format!("Network access  {protocol} {host}"))
            } else {
                params["command"]
                    .as_str()
                    .filter(|command| !command.trim().is_empty())
                    .map(|command| format!("$ {command}"))
            }
        } else {
            None
        };
        let offers_accept = params["availableDecisions"]
            .as_array()
            .is_none_or(|decisions| decisions.iter().any(|decision| decision == "accept"));
        let can_accept = action.is_some() && offers_accept && params["grantRoot"].is_null();
        let mut details: Vec<String> = action.into_iter().collect();
        if let Some(cwd) = params["cwd"].as_str() {
            details.push(format!("Directory  {cwd}"));
        }
        if let Some(root) = params["grantRoot"].as_str() {
            details.push(format!("Requested session write access  {root}"));
        }
        if let Some(reason) = params["reason"].as_str() {
            details.push(reason.to_owned());
        }
        if !can_accept {
            details.push("This action cannot be approved from Melly. Press n to decline.".into());
        }
        self.approvals.push_back(Approval {
            id: id.clone(),
            can_accept,
            description: details.join("\n"),
        });
    }

    fn push(&mut self, entry: Entry) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn transcript(&self) -> String {
        self.entries
            .iter()
            .map(|entry| format!("{}\n{}", entry.speaker.label(), entry.text))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Entry {
    fn complete(speaker: Speaker, text: impl Into<String>) -> Self {
        Self {
            speaker,
            text: text.into(),
            id: None,
            complete: true,
        }
    }
}

fn owned_string(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn item_entry(item: &Value) -> Option<Entry> {
    let id = owned_string(&item["id"])?;
    let (speaker, mut text) = match item["type"].as_str()? {
        "agentMessage" => (
            Speaker::Codex,
            item["text"].as_str().unwrap_or_default().to_owned(),
        ),
        "userMessage" => {
            let text = item["content"]
                .as_array()?
                .iter()
                .filter_map(|input| input["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            (Speaker::You, text)
        }
        "commandExecution" => {
            let command = item["command"].as_str().unwrap_or("Command");
            let output = item["aggregatedOutput"]
                .as_str()
                .filter(|output| !output.is_empty());
            let mut text = format!("$ {command}");
            if let Some(output) = output {
                text.push('\n');
                text.push_str(output.trim_end());
            }
            if let Some(exit) = item["exitCode"].as_i64() {
                text.push_str(&format!("\nExited {exit}"));
            }
            (Speaker::Tool, text)
        }
        "fileChange" => {
            let paths = item["changes"]
                .as_array()?
                .iter()
                .filter_map(|change| change["path"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let status = item["status"].as_str().unwrap_or("updated");
            (Speaker::Tool, format!("Files · {status}\n{paths}"))
        }
        "mcpToolCall" => {
            let name = item["tool"].as_str().unwrap_or("MCP tool");
            let status = item["status"].as_str().unwrap_or("running");
            (Speaker::Tool, format!("{name} · {status}"))
        }
        _ => return None,
    };
    if speaker == Speaker::Tool {
        truncate_tool(&mut text);
    }
    Some(Entry {
        id: Some(id),
        speaker,
        text,
        complete: true,
    })
}

fn patch_preview(item: &Value) -> Option<String> {
    let mut lines = Vec::new();
    for change in item["changes"].as_array()? {
        let path = change["path"].as_str()?;
        let kind = change["kind"]["type"].as_str()?;
        let diff = change["diff"].as_str()?;
        let target = change["kind"]["movePath"]
            .as_str()
            .map_or(String::new(), |to| format!(" → {to}"));
        lines.push(format!("{kind}  {path}{target}\n{diff}"));
    }
    (!lines.is_empty()).then(|| lines.join("\n\n"))
}

fn truncate_tool(text: &mut String) {
    if text.len() > MAX_TOOL_CHARS {
        let start = text.ceil_char_boundary(text.len() - MAX_TOOL_CHARS);
        text.replace_range(..start, "… earlier tool output clipped …\n");
    }
}
