use crate::catalog::{Model, Quota, Skill, quota_windows};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const HISTORY_PAGE_SIZE: usize = 40;
const MAX_ENTRIES: usize = 400;
const MAX_TOOL_CHARS: usize = 12_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    You,
    Codex,
    Tool,
    Thinking,
    Notice,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Self::You => "You",
            Self::Codex => "Codex",
            Self::Tool => "Tool",
            Self::Thinking => "Thinking",
            Self::Notice => "Session",
        }
    }
}

pub struct Entry {
    pub speaker: Speaker,
    pub text: String,
    id: Option<String>,
    pub complete: bool,
    pub key: u64,
    pub revision: u64,
    pub model: Option<String>,
    pub outcome: Option<String>,
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
    Compact,
    Skills,
    Quota,
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
    pub models: Vec<Model>,
    pub models_loaded: bool,
    pub effort: Option<String>,
    pub skills: Vec<Skill>,
    pub skills_loaded: bool,
    pub quota: Vec<Quota>,
    pub quota_loaded: bool,
    pub quota_updated_at: HashMap<String, Instant>,
    pub context_used: Option<u64>,
    pub input_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub startup_duration: Option<Duration>,
    pub first_event_duration: Option<Duration>,
    pub last_turn_duration: Option<Duration>,
    pub selected_skills: Vec<Skill>,
    pub content_revision: u64,
    started_at: Instant,
    turn_started_at: Option<Instant>,
    pub active_model: Option<String>,
    pub active_effort: Option<String>,
    compacting: bool,
    next_entry_key: u64,
    summaries: HashMap<String, Vec<String>>,
    raw_reasoning: HashMap<String, Vec<String>>,
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
    settings_selected: bool,
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
            models: Vec::new(),
            models_loaded: false,
            effort: None,
            skills: Vec::new(),
            skills_loaded: false,
            quota: Vec::new(),
            quota_loaded: false,
            quota_updated_at: HashMap::new(),
            context_used: None,
            input_tokens: None,
            cached_tokens: None,
            output_tokens: None,
            reasoning_tokens: None,
            startup_duration: None,
            first_event_duration: None,
            last_turn_duration: None,
            selected_skills: Vec::new(),
            content_revision: 0,
            started_at: Instant::now(),
            turn_started_at: None,
            active_model: None,
            active_effort: None,
            compacting: false,
            next_entry_key: 1,
            summaries: HashMap::new(),
            raw_reasoning: HashMap::new(),
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
            settings_selected: false,
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

    /// Open a child thread on the already initialized shared connection.
    pub fn worker(cwd: String, model: Model, read_only: bool) -> Self {
        let mut session = Self::new(cwd, Some(model.id.clone()), None);
        session.pending.clear();
        session.outbox.clear();
        session.model = Some(model.id.clone());
        session.effort = if model.efforts.iter().any(|effort| effort.id == "low") {
            Some("low".into())
        } else {
            model.default_effort.clone()
        };
        session.models = vec![model];
        session.models_loaded = true;
        session.settings_selected = true;
        session.open_thread(false);
        if read_only && let Some(request) = session.outbox.last_mut() {
            request["params"]["sandbox"] = json!("read-only");
            request["params"]["approvalPolicy"] = json!("never");
        }
        session
    }

    pub(crate) fn restore_worker(
        cwd: String,
        thread_id: String,
        model: Option<String>,
        read_only: bool,
    ) -> Self {
        let mut session = Self::new(cwd, None, Some(thread_id));
        session.model = model;
        session.pending.clear();
        session.outbox.clear();
        session.models_loaded = true;
        session.open_thread(false);
        if read_only && let Some(request) = session.outbox.last_mut() {
            request["params"]["sandbox"] = json!("read-only");
            request["params"]["approvalPolicy"] = json!("never");
        }
        session
    }

    pub(crate) fn replace_empty_thread(&mut self, request_id: Option<u64>) {
        if let Some(id) = request_id {
            self.pending.remove(&id);
        }
        self.resume_id = None;
        if self.requested_model.is_none() {
            self.requested_model.clone_from(&self.model);
        }
        self.open_thread(false);
        self.notice("Opened a new lead chat because Codex had no transcript for the empty lead. Restoring the saved workers.");
    }

    pub(crate) fn saved_thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref().or(self.resume_id.as_deref())
    }

    pub(crate) fn compacting(&self) -> bool {
        self.compacting
    }

    pub fn active_turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
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

    pub fn busy(&self) -> bool {
        self.compacting
            || self.turn_id.is_some()
            || self.pending.values().any(|p| *p == Pending::Start)
    }

    pub fn connecting(&self) -> bool {
        (self.thread_id.is_none() || !self.models_loaded) && !self.stopped
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.turn_started_at
            .map(|start| start.elapsed())
            .or(self.last_turn_duration)
    }

    pub fn current_model(&self) -> Option<&Model> {
        self.models
            .iter()
            .find(|m| Some(&m.id) == self.model.as_ref())
    }

    pub fn select_model(&mut self, id: &str) -> bool {
        let Some(model) = self.models.iter().find(|m| m.id == id) else {
            return false;
        };
        self.model = Some(model.id.clone());
        self.effort.clone_from(&model.default_effort);
        self.settings_selected = true;
        true
    }

    pub fn select_effort(&mut self, effort: &str) -> bool {
        if !self
            .current_model()
            .is_some_and(|m| m.efforts.iter().any(|e| e.id == effort))
        {
            return false;
        }
        self.effort = Some(effort.into());
        self.settings_selected = true;
        true
    }

    pub fn notice(&mut self, text: impl Into<String>) {
        self.push(Entry::complete(Speaker::Notice, text));
    }

    pub fn compact(&mut self) -> bool {
        if self.stopped || self.connecting() || self.busy() {
            return false;
        }
        self.compacting = true;
        self.turn_started_at = Some(Instant::now());
        self.first_event_duration = None;
        self.last_turn_duration = None;
        self.status = "Compacting".into();
        self.request(
            Pending::Compact,
            "thread/compact/start",
            json!({"threadId":self.thread_id}),
        );
        true
    }

    pub fn load_skills(&mut self) {
        if self.stopped || self.connecting() || self.pending.values().any(|p| *p == Pending::Skills)
        {
            return;
        }
        self.skills_loaded = false;
        self.request(
            Pending::Skills,
            "skills/list",
            json!({"cwds":[self.cwd],"forceReload":false}),
        );
    }

    pub(crate) fn quota_pending(&self) -> bool {
        self.pending
            .values()
            .any(|pending| *pending == Pending::Quota)
    }

    pub fn refresh_quota(&mut self) {
        if self.stopped
            || self
                .pending
                .values()
                .any(|pending| *pending == Pending::Initialize)
            || self.quota_pending()
        {
            return;
        }
        self.request(Pending::Quota, "account/rateLimits/read", json!({}));
    }

    pub fn attach_skill(&mut self, index: usize) {
        if let Some(skill) = self.skills.get(index)
            && !self.selected_skills.iter().any(|s| s.path == skill.path)
        {
            self.selected_skills.push(skill.clone());
        }
    }

    pub fn submit(&mut self, prompt: &str) -> bool {
        let Some(thread_id) = self.thread_id.clone() else {
            return false;
        };
        if self.stopped || !self.models_loaded || self.compacting || prompt.trim().is_empty() {
            return false;
        }

        let mut input = vec![json!({"type": "text", "text": prompt})];
        input.extend(
            self.selected_skills
                .iter()
                .map(|skill| json!({"type":"skill","name":skill.name,"path":skill.path})),
        );
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
                    "threadId": thread_id, "input": input,
                    "model": self.model, "effort": self.effort, "summary": "auto"
                }),
            );
            self.status = "Sending".into();
            self.active_model.clone_from(&self.model);
            self.active_effort.clone_from(&self.effort);
            self.turn_started_at = Some(Instant::now());
            self.first_event_duration = None;
            self.last_turn_duration = None;
        }
        self.selected_skills.clear();
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
            if self.compacting
                || self
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
        self.compacting = false;
        self.turn_started_at = None;
        self.pending.clear();
        self.approvals.clear();
        self.outbox.clear();
        self.status = "Disconnected".into();
        self.push(Entry::complete(Speaker::Notice, message));
    }

    pub fn receive(&mut self, message: Value) -> bool {
        match (
            message.get("method").and_then(Value::as_str),
            message.get("id"),
        ) {
            (Some(method), Some(id)) => self.server_request(method, id, &message["params"]),
            (Some(method), None) => return self.notification(method, &message["params"]),
            (None, Some(id)) => {
                if let Some(purpose) = id.as_u64().and_then(|id| self.pending.remove(&id)) {
                    self.response(purpose, &message);
                } else {
                    return false;
                }
            }
            (None, None) => return false,
        }
        true
    }

    fn response(&mut self, purpose: Pending, message: &Value) {
        if let Some(error) = message["error"]["message"].as_str() {
            match purpose {
                Pending::Initialize | Pending::Thread => self.disconnected(error),
                Pending::Models => {
                    self.models_loaded = true;
                    self.push(Entry::complete(Speaker::Notice, error));
                    if self.thread_id.is_none()
                        && !self.pending.values().any(|p| *p == Pending::Thread)
                    {
                        self.open_thread(false);
                    } else if self.thread_id.is_some() && self.startup_duration.is_none() {
                        self.ready_with_settings();
                    }
                }
                Pending::Start | Pending::Interrupt => {
                    self.interrupt_requested = false;
                    self.status = if self.turn_id.is_some() {
                        "Running"
                    } else {
                        "Ready"
                    }
                    .into();
                    if self.turn_id.is_none() {
                        self.finish_timer();
                    }
                    self.push(Entry::complete(Speaker::Notice, error));
                }
                Pending::Compact => {
                    self.compacting = false;
                    self.interrupt_requested = false;
                    self.status = "Ready".into();
                    self.finish_timer();
                    self.notice(error);
                }
                Pending::Skills => {
                    self.skills_loaded = true;
                    self.notice(error);
                }
                Pending::Quota => {
                    self.quota_loaded = true;
                    self.notice(format!("Account usage unavailable: {error}"));
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
                self.request(Pending::Quota, "account/rateLimits/read", json!({}));
                if self.requested_model.is_some() || self.resume_id.is_some() {
                    self.open_thread(false);
                }
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
                self.models_loaded = result["nextCursor"].is_null();
                self.models.extend(
                    result["data"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Model::parse),
                );
                if self.models_loaded
                    && self.thread_id.is_none()
                    && !self.pending.values().any(|p| *p == Pending::Thread)
                {
                    let sol_available = self.models.iter().any(|m| m.id == "gpt-6.1-sol");
                    self.open_thread(sol_available);
                }
                if self.models_loaded && self.thread_id.is_some() && self.startup_duration.is_none()
                {
                    self.ready_with_settings();
                }
                if let Some(cursor) = result["nextCursor"].as_str() {
                    self.request(
                        Pending::Models,
                        "model/list",
                        json!({"limit":100,"cursor":cursor}),
                    );
                }
            }
            Pending::Thread => {
                self.thread_id = owned_string(&result["thread"]["id"]);
                if self.thread_id.is_none() {
                    self.disconnected("Codex did not return a thread ID.");
                    return;
                }
                if !self.settings_selected {
                    if let Some(model) = owned_string(&result["model"]) {
                        self.model = Some(model);
                    }
                    self.effort = owned_string(&result["reasoningEffort"]);
                }
                if let Some(cwd) = owned_string(&result["cwd"]) {
                    self.cwd = cwd;
                }
                if self.models_loaded {
                    self.ready_with_settings();
                } else {
                    self.status = "Loading models".into();
                }
                if self.resume_id.is_some() {
                    self.request_history(None);
                }
            }
            Pending::History => {
                self.older_cursor = owned_string(&result["nextCursor"]);
                if let Some(rows) = result["data"].as_array() {
                    for row in rows {
                        if let Some(mut entry) = item_entry(&row["item"])
                            && !self.entries.iter().any(|prior| prior.id == entry.id)
                        {
                            entry.key = self.next_entry_key;
                            self.next_entry_key += 1;
                            self.entries.push_front(entry);
                            self.content_revision += 1;
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
            Pending::Compact => {} // Acceptance is not completion; wait for contextCompaction.
            Pending::Quota => {
                self.quota = quota_windows(result);
                self.quota_updated_at.clear();
                for quota in &self.quota {
                    self.quota_updated_at
                        .insert(quota.bucket_id.clone(), Instant::now());
                }
                self.quota_loaded = true;
            }
            Pending::Skills => {
                self.skills.clear();
                for group in result["data"].as_array().into_iter().flatten() {
                    for skill in group["skills"].as_array().into_iter().flatten() {
                        if skill["enabled"] != true {
                            continue;
                        }
                        if let (Some(name), Some(path)) =
                            (skill["name"].as_str(), skill["path"].as_str())
                        {
                            self.skills.push(Skill {
                                name: name.into(),
                                path: path.into(),
                                description: skill["interface"]["shortDescription"]
                                    .as_str()
                                    .or(skill["shortDescription"].as_str())
                                    .or(skill["description"].as_str())
                                    .unwrap_or_default()
                                    .into(),
                            });
                        }
                    }
                    for error in group["errors"].as_array().into_iter().flatten() {
                        if let Some(message) = error["message"].as_str() {
                            self.notice(format!("Skill could not load: {message}"));
                        }
                    }
                }
                self.skills.sort_by(|a, b| a.name.cmp(&b.name));
                self.skills_loaded = true;
            }
        }
    }

    fn open_thread(&mut self, sol_available: bool) {
        self.status = "Opening session".into();
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

    fn ready_with_settings(&mut self) {
        if self.resume_id.is_none()
            && !self.settings_selected
            && let Some(default) = self.current_model().and_then(|m| m.default_effort.clone())
        {
            self.effort = Some(default);
        }
        self.status = "Ready".into();
        self.startup_duration = Some(self.started_at.elapsed());
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
            let already_running = self.turn_id.as_deref() == Some(id);
            self.turn_id = Some(id.to_owned());
            if self.interrupt_requested {
                self.interrupt_requested = false;
                self.interrupt();
            } else if !already_running
                && !self
                    .pending
                    .values()
                    .any(|pending| *pending == Pending::Interrupt)
            {
                self.status = if self.compacting {
                    "Compacting"
                } else {
                    "Thinking"
                }
                .into();
            }
        }
    }

    fn finish_timer(&mut self) {
        if let Some(start) = self.turn_started_at.take() {
            self.last_turn_duration = Some(start.elapsed());
        }
    }

    fn notification(&mut self, method: &str, params: &Value) -> bool {
        if method == "account/updated" {
            self.account = owned_string(&params["authMode"]);
            self.quota.clear();
            self.quota_updated_at.clear();
            self.quota_loaded = false;
            let stale: Vec<u64> = self
                .pending
                .iter()
                .filter(|(_, purpose)| **purpose == Pending::Quota)
                .map(|(id, _)| *id)
                .collect();
            self.pending.retain(|_, purpose| *purpose != Pending::Quota);
            self.outbox
                .retain(|request| request["id"].as_u64().is_none_or(|id| !stale.contains(&id)));
            self.refresh_quota();
            return true;
        }
        if method == "account/rateLimits/updated" {
            let id = params["rateLimits"]["limitId"].as_str().unwrap_or("codex");
            self.quota.retain(|q| q.bucket_id != id);
            let windows = quota_windows(params);
            if windows.is_empty() {
                self.quota_updated_at.remove(id);
            } else {
                self.quota_updated_at.insert(id.to_owned(), Instant::now());
            }
            self.quota.extend(windows);
            self.quota_loaded = true;
            return true;
        }
        if method == "skills/changed" {
            self.load_skills();
            return true;
        }
        if self.thread_id.is_none() || params["threadId"].as_str() != self.thread_id.as_deref() {
            return false;
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
                    return false;
                };
                if self.last_finished_turn.as_deref() == Some(id)
                    || self.turn_id.as_deref().is_some_and(|current| current != id)
                {
                    return false;
                }
                self.last_finished_turn = Some(id.to_owned());
                self.turn_id = None;
                self.interrupt_requested = false;
                self.compacting = false;
                self.finish_timer();
                self.summaries.clear();
                self.raw_reasoning.clear();
                for entry in &mut self.entries {
                    if !entry.complete {
                        entry.complete = true;
                        entry.outcome = owned_string(&turn["status"]);
                        entry.revision += 1;
                    }
                }
                self.content_revision += 1;
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
                    return false;
                }
                if self.first_event_duration.is_none() {
                    self.first_event_duration = self.turn_started_at.map(|t| t.elapsed());
                }
                if item["type"] == "contextCompaction" {
                    self.status = if self.compacting || method == "item/started" {
                        "Compacting"
                    } else {
                        "Running"
                    }
                    .into();
                    if method == "item/completed" {
                        self.context_used = None;
                        self.notice(
                            "Context compacted. The next usage snapshot will update the meter.",
                        );
                    }
                }
                if method == "item/started" {
                    self.status = match item["type"].as_str() {
                        Some("reasoning") => "Thinking",
                        Some("agentMessage") => "Responding",
                        Some("commandExecution" | "fileChange" | "mcpToolCall") => "Running tool",
                        _ => &self.status,
                    }
                    .into();
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
                    if entry.speaker == Speaker::Thinking
                        && reasoning_text(&item["summary"]).is_empty()
                        && let Some(parts) =
                            item["id"].as_str().and_then(|id| self.summaries.get(id))
                        && parts.iter().any(|part| !part.is_empty())
                    {
                        entry.text = parts.join("\n\n");
                    }
                    entry.complete = method == "item/completed";
                    entry.model.clone_from(&self.active_model);
                    if let Some(prior) = self.entries.iter_mut().find(|prior| prior.id == entry.id)
                    {
                        if entry.complete || prior.text.is_empty() {
                            if item["type"] == "commandExecution"
                                && !item["aggregatedOutput"].is_string()
                            {
                                entry.text.clone_from(&prior.text);
                            }
                            if entry.speaker == Speaker::Thinking && entry.text.is_empty() {
                                entry.text.clone_from(&prior.text);
                            }
                            entry.key = prior.key;
                            entry.revision = prior.revision + 1;
                            *prior = entry;
                            self.content_revision += 1;
                        }
                    } else {
                        self.push(entry);
                    }
                }
            }
            "item/reasoning/summaryPartAdded"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta" => {
                let raw = method == "item/reasoning/textDelta";
                let index_key = if raw { "contentIndex" } else { "summaryIndex" };
                let (Some(id), Some(index)) =
                    (params["itemId"].as_str(), params[index_key].as_u64())
                else {
                    return false;
                };
                if index > 1024
                    || self
                        .entries
                        .iter()
                        .any(|e| e.id.as_deref() == Some(id) && e.complete)
                {
                    return false;
                }
                if raw
                    && self
                        .summaries
                        .get(id)
                        .is_some_and(|parts| parts.iter().any(|p| !p.is_empty()))
                {
                    return false;
                }
                let parts = if raw {
                    self.raw_reasoning.entry(id.into()).or_default()
                } else {
                    self.summaries.entry(id.into()).or_default()
                };
                parts.resize(parts.len().max(index as usize + 1), String::new());
                parts[index as usize].push_str(params["delta"].as_str().unwrap_or_default());
                let text = parts.join("\n\n");
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|e| e.id.as_deref() == Some(id))
                {
                    entry.text = text;
                    entry.revision += 1;
                    self.content_revision += 1;
                } else {
                    let mut entry = Entry::complete(Speaker::Thinking, text);
                    entry.id = Some(id.into());
                    entry.complete = false;
                    entry.model.clone_from(&self.active_model);
                    self.push(entry);
                }
                self.status = "Thinking".into();
                if self.first_event_duration.is_none() {
                    self.first_event_duration = self.turn_started_at.map(|t| t.elapsed());
                }
            }
            "item/agentMessage/delta" | "item/commandExecution/outputDelta" => {
                if self.first_event_duration.is_none() {
                    self.first_event_duration = self.turn_started_at.map(|t| t.elapsed());
                }
                self.status = if method == "item/agentMessage/delta" {
                    "Responding"
                } else {
                    "Running tool"
                }
                .into();
                let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str())
                else {
                    return false;
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
                        entry.revision += 1;
                        self.content_revision += 1;
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
                        key: 0,
                        revision: 0,
                        model: self.active_model.clone(),
                        outcome: None,
                    });
                }
            }
            "thread/tokenUsage/updated" => {
                self.tokens = params["tokenUsage"]["total"]["totalTokens"].as_u64();
                self.context_window = params["tokenUsage"]["modelContextWindow"].as_u64();
                self.context_used = params["tokenUsage"]["last"]["totalTokens"].as_u64();
                let total = &params["tokenUsage"]["total"];
                self.input_tokens = total["inputTokens"].as_u64();
                self.cached_tokens = total["cachedInputTokens"].as_u64();
                self.output_tokens = total["outputTokens"].as_u64();
                self.reasoning_tokens = total["reasoningOutputTokens"].as_u64();
            }
            "model/rerouted" => {
                if params["turnId"].as_str() != self.turn_id.as_deref() {
                    return false;
                }
                self.active_model = owned_string(&params["toModel"]);
                self.active_effort = None;
                self.notice(format!(
                    "Codex routed this turn to {}. Your next-turn selection is unchanged.",
                    self.active_model.as_deref().unwrap_or("another model")
                ));
            }
            "serverRequest/resolved" => self
                .approvals
                .retain(|approval| approval.id != params["requestId"]),
            _ => return false,
        }
        true
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
            self.notice(format!(
                "Codex requested {method}, which Melly cannot handle yet."
            ));
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
        let additional_permissions = match &params["additionalPermissions"] {
            Value::Null => false,
            Value::Object(fields) => !fields.is_empty(),
            Value::Array(values) => !values.is_empty(),
            Value::String(text) => !text.is_empty(),
            _ => true,
        };
        let can_accept = action.is_some()
            && offers_accept
            && params["grantRoot"].is_null()
            && !additional_permissions;
        let mut details: Vec<String> = action.into_iter().collect();
        if additional_permissions {
            details.push("Additional permissions were requested and cannot be reviewed or approved from Melly.".into());
        }
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

    fn push(&mut self, mut entry: Entry) {
        entry.key = self.next_entry_key;
        self.next_entry_key += 1;
        self.content_revision += 1;
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
            key: 0,
            revision: 0,
            model: None,
            outcome: None,
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
        "reasoning" => {
            let summary = reasoning_text(&item["summary"]);
            let text = if summary.is_empty() {
                reasoning_text(&item["content"])
            } else {
                summary
            };
            (Speaker::Thinking, text)
        }
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
        key: 0,
        revision: 0,
        model: None,
        outcome: if item["exitCode"].as_i64().is_some_and(|code| code != 0) {
            Some("failed".into())
        } else {
            owned_string(
                &item[if speaker == Speaker::Codex {
                    "phase"
                } else {
                    "status"
                }],
            )
        },
    })
}

fn reasoning_text(parts: &Value) -> String {
    parts
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn patch_preview(item: &Value) -> Option<String> {
    let mut lines = Vec::new();
    for change in item["changes"].as_array()? {
        let path = change["path"].as_str()?;
        let kind = change["kind"]["type"].as_str()?;
        let diff = change["diff"].as_str()?;
        let target = change["kind"]["move_path"]
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
