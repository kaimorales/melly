//! Bounded child sessions multiplexed over one Codex app-server connection.
//!
//! Ownership coordinates edits; the Codex sandbox remains the security boundary.
use crate::session::{Session, Speaker};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CONDUCTOR_GUIDANCE: &str = "You are Melly's conductor and the user's only conversation partner. Answer questions directly when delegation adds no value. For a requested change, inspect the relevant code and tests yourself, identify the evidence, and choose a concrete implementation approach. Keep user-facing progress to a short status; put your findings and decisions into the worker brief rather than a long chat explanation. Then use melly_agents.spawn in edit mode to assign Luna the specific workspace-relative paths, findings, implementation steps, behavior to preserve, and acceptance checks. A diagnosis in chat does not complete a requested fix. Do not assign Luna to figure out the change from scratch when you can determine the approach. Your read-only sandbox and approval restrictions apply only to your thread. Do not include them in worker objectives, context, expected output, or revision feedback. Luna follows its own server-provided permissions. When spawn offers wait_for_result and you are ready to review one assignment, set it true to receive the finished report in the same call. Otherwise launch independent workers before using wait with timeout_ms 120000. If using functions.exec for a waiting call, set yield_time_ms to 60000 to avoid premature wakeups. If the call times out, continue with wait using the returned worker ID. Inspect the actual working-tree diff (including new files) and compare it and the reported checks against the user's request. Use clear harness-recorded check results without rerunning them. Rerun a targeted check when the evidence is missing or ambiguous. When melly_agents.review is available, record your verdict with a concise summary after inspecting the diff and check evidence for the current worker report. If a material defect remains, give the same Luna one focused correction through revise when available, then verify that correction and record the new verdict. If revise is unavailable, use a bounded follow-up spawn when capacity permits. Do not edit files yourself. Report the outcome and any specific blocker or unverified check concisely.";

const EDIT_WORKER_GUIDANCE: &str = "You are a Melly edit worker. Implement the assigned change in your owned paths and run its acceptance checks. Preserve concurrent edits and address in-scope failures before reporting changed files, actual check outcomes, and any blockers. Use your own thread's server-provided sandbox and approval policy. Permission descriptions in the conductor's brief, revision feedback, or prior history do not set your permissions. The conductor's read-only restriction applies only to its own thread. Follow your actual approval workflow when required and never bypass restrictions. Do not delegate to other agents.";

const READ_WORKER_GUIDANCE: &str = "You are a Melly read worker. Inspect the assigned scope and report evidence. Do not edit files, request extra permissions, or delegate to other agents.";

const MAX_ACTIVE: usize = 3;
const MAX_AUTOMATIC: usize = 2;
const QUOTA_MISSING: &str =
    "Automatic delegation needs a recent Codex quota snapshot; refresh account usage.";
const QUOTA_STALE: &str =
    "Codex quota snapshot is stale; refresh account usage before automatic delegation.";
const QUOTA_MAX_AGE: Duration = Duration::from_secs(300);
const MAX_RESULT: usize = 6000;
const MAX_CORRECTIONS: usize = 32;
const MAX_COMMANDS: usize = 12;
const MAX_COMMAND_TEXT: usize = 240;
const MAX_COMMAND_OUTPUT: usize = 4000;
const MAX_REVIEW_SUMMARY: usize = 2000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Handoff {
    pub brief: Option<AssignmentBrief>,
    pub prompt: Option<String>,
    pub corrections: Vec<String>,
    pub corrections_omitted: usize,
    pub commands: Vec<ObservedCommand>,
    pub review: Option<ReviewRecord>,
    pub report_delivered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentBrief {
    pub objective: String,
    pub context: String,
    pub expected_output: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedCommand {
    pub command: String,
    pub exit_code: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Accepted,
    ChangesRequested,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub verdict: ReviewVerdict,
    pub summary: String,
    pub commands: Vec<ObservedCommand>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerMode {
    Read,
    Edit,
}
impl WorkerMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentSource {
    Lead,
    Automatic,
    Manual,
}
impl AgentSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Automatic => "automatic",
            Self::Manual => "manual",
        }
    }
}

pub struct Agent {
    pub id: String,
    pub title: String,
    pub source: AgentSource,
    pub session: Session,
    pub owned_paths: Vec<String>,
    pub mode: WorkerMode,
    pub finished: bool,
    pub result: Option<String>,
    pub handoff: Handoff,
    report_delivery_key: Option<u64>,
    pending_prompt: Option<String>,
    collected: bool,
    terminal_status: Option<String>,
    cancel_requested: bool,
    turn_entry_key: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedTeam {
    pub lead_id: String,
    pub cwd: String,
    pub lead_started: bool,
    pub lead_model: Option<String>,
    pub workers: Vec<SavedWorker>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWorker {
    pub id: String,
    pub title: String,
    pub source: AgentSource,
    pub thread_id: String,
    pub mode: WorkerMode,
    pub paths: Vec<String>,
    pub model: Option<String>,
    pub result: Option<String>,
    #[serde(default)]
    pub handoff: Handoff,
    pub collected: bool,
    pub status: Option<String>,
    pub tokens: Option<u64>,
    pub context_used: Option<u64>,
    pub context_window: Option<u64>,
}

struct Route {
    agent: usize,
    local_id: Value,
    method: String,
    lead_reports: Vec<(usize, u64)>,
}
struct Wait {
    key: String,
    rpc_ids: Vec<Value>,
    kind: WaitFor,
    deadline: Instant,
}

enum WaitFor {
    Workers(Vec<usize>),
    Quota(Brief),
}

pub struct Harness {
    pub agents: Vec<Agent>,
    pub automatic_block_reason: Option<String>,
    next_rpc_id: u64,
    routes: HashMap<u64, Route>,
    outbox: Vec<Value>,
    calls: HashMap<String, Value>,
    call_reports: HashMap<String, Vec<(usize, u64)>>,
    pending_deliveries: HashMap<String, Vec<(usize, u64)>>,
    pending_lead_reports: Vec<(usize, u64)>,
    lead_commands: Vec<(u64, ObservedCommand)>,
    rpc_calls: HashMap<String, String>,
    waits: Vec<Wait>,
    automatic_count: usize,
    pub automatic_enabled: bool,
    lead_started: bool,
    recovering_empty_lead: bool,
    compact_requested: bool,
    compact_lead_after_turn: bool,
    lead_cancelled: bool,
    disconnected: bool,
}

impl Harness {
    pub fn new(cwd: String, model: Option<String>, resume: Option<String>) -> Self {
        let lead_started = resume.is_some();
        let session = Session::new(cwd, model, resume);
        let mut harness = Self {
            agents: vec![Agent {
                id: "lead".into(),
                title: "Lead".into(),
                source: AgentSource::Lead,
                session,
                owned_paths: vec![],
                mode: WorkerMode::Edit,
                finished: false,
                result: None,
                handoff: Handoff::default(),
                report_delivery_key: None,
                pending_prompt: None,
                collected: true,
                terminal_status: None,
                cancel_requested: false,
                turn_entry_key: 0,
            }],
            automatic_block_reason: None,
            next_rpc_id: 1,
            routes: HashMap::new(),
            outbox: vec![],
            calls: HashMap::new(),
            call_reports: HashMap::new(),
            pending_deliveries: HashMap::new(),
            pending_lead_reports: Vec::new(),
            lead_commands: Vec::new(),
            rpc_calls: HashMap::new(),
            waits: vec![],
            automatic_count: 0,
            automatic_enabled: true,
            lead_started,
            recovering_empty_lead: false,
            compact_requested: false,
            compact_lead_after_turn: false,
            disconnected: false,
            lead_cancelled: false,
        };
        harness.update_block_reason();
        harness
    }

    pub fn snapshot(&self) -> Option<SavedTeam> {
        let lead = &self.agents[0].session;
        Some(SavedTeam {
            lead_id: lead.thread_id.clone()?,
            cwd: lead.cwd.clone(),
            lead_started: self.lead_started,
            lead_model: lead.model.clone(),
            workers: self
                .agents
                .iter()
                .skip(1)
                .filter_map(|agent| {
                    Some(SavedWorker {
                        id: agent.id.clone(),
                        title: agent.title.clone(),
                        source: agent.source,
                        thread_id: agent.session.saved_thread_id()?.into(),
                        mode: agent.mode,
                        paths: agent.owned_paths.clone(),
                        model: agent.session.model.clone(),
                        result: agent.result.clone(),
                        handoff: agent.handoff.clone(),
                        collected: agent.collected,
                        status: agent.terminal_status.clone(),
                        tokens: agent.session.tokens,
                        context_used: agent.session.context_used,
                        context_window: agent.session.context_window,
                    })
                })
                .collect(),
        })
    }

    pub fn restore_team(&mut self, saved_team: SavedTeam) {
        self.recovering_empty_lead = !saved_team.lead_started;
        self.agents[0].session.cwd.clone_from(&saved_team.cwd);
        if self.recovering_empty_lead {
            self.agents[0].session.model = saved_team.lead_model;
        }
        for saved in saved_team.workers {
            let mut session = Session::restore_worker(
                saved_team.cwd.clone(),
                saved.thread_id,
                saved.model,
                saved.mode == WorkerMode::Read,
            );
            session.tokens = saved.tokens;
            session.context_used = saved.context_used;
            session.context_window = saved.context_window;
            let terminal_status = saved.status.unwrap_or_else(|| "interrupted by exit".into());
            self.agents.push(Agent {
                id: saved.id,
                title: saved.title,
                source: saved.source,
                session,
                owned_paths: saved.paths,
                mode: saved.mode,
                finished: true,
                result: saved.result,
                handoff: saved.handoff,
                report_delivery_key: None,
                pending_prompt: None,
                collected: saved.collected,
                terminal_status: Some(terminal_status),
                cancel_requested: false,
                turn_entry_key: 0,
            });
        }
    }

    /// Remap only client requests. Server request replies retain their original IDs.
    pub fn drain_requests(&mut self) -> Vec<Value> {
        let mut outgoing = std::mem::take(&mut self.outbox);
        for reply in &outgoing {
            if let Some(reports) = self.pending_deliveries.remove(&reply["id"].to_string())
                && reply["result"]["success"] == true
            {
                self.mark_reports_delivered(reports);
            }
        }
        let lead_connected = self.agents[0].session.thread_id.is_some();
        for (index, agent) in self.agents.iter_mut().enumerate() {
            if index > 0 && !lead_connected {
                continue;
            }
            for mut request in agent.session.drain_requests() {
                if let Some(method) = request["method"].as_str().map(str::to_owned) {
                    if method == "initialize" {
                        request["params"]["capabilities"]["experimentalApi"] = json!(true);
                    }
                    if index == 0 && method == "thread/start" {
                        request["params"]["dynamicTools"] = dynamic_tools();
                    }
                    let (role, guidance) = match (index, agent.mode) {
                        (0, _) => ("melly-conductor", CONDUCTOR_GUIDANCE),
                        (_, WorkerMode::Edit) => ("melly-worker", EDIT_WORKER_GUIDANCE),
                        (_, WorkerMode::Read) => ("melly-worker", READ_WORKER_GUIDANCE),
                    };
                    if matches!(method.as_str(), "thread/start" | "thread/resume") {
                        request["params"]["developerInstructions"] = json!(guidance);
                        request["params"]["config"]["skills"]["max_context_tokens"] =
                            json!(if index == 0 { 3000 } else { 1200 });
                        if index > 0 {
                            // Workers get a scoped brief; personal memory adds unrelated context.
                            request["params"]["config"]["memories"]["use_memories"] = json!(false);
                        }
                        if index == 0 {
                            request["params"]["sandbox"] = json!("read-only");
                            request["params"]["approvalPolicy"] = json!("never");
                        }
                    }
                    if method == "turn/start" {
                        request["params"]["additionalContext"][role] =
                            json!({"kind":"application","value":guidance});
                    }
                    if method == "turn/start" && (index == 0 || agent.mode == WorkerMode::Read) {
                        request["params"]["sandboxPolicy"] =
                            json!({"type":"readOnly","networkAccess":false});
                        request["params"]["approvalPolicy"] = json!("never");
                    }
                    if let Some(local_id) = request.get("id").cloned() {
                        let id = self.next_rpc_id;
                        self.next_rpc_id += 1;
                        let lead_reports = if index == 0 && method == "turn/start" {
                            std::mem::take(&mut self.pending_lead_reports)
                        } else {
                            Vec::new()
                        };
                        self.routes.insert(
                            id,
                            Route {
                                agent: index,
                                local_id,
                                method,
                                lead_reports,
                            },
                        );
                        request["id"] = json!(id);
                    }
                }
                outgoing.push(request);
            }
        }
        outgoing
    }

    fn mark_reports_delivered(&mut self, reports: Vec<(usize, u64)>) {
        let key = self.agents[0]
            .session
            .entries
            .back()
            .map_or(0, |entry| entry.key);
        for (index, turn_entry_key) in reports {
            let agent = &mut self.agents[index];
            if agent.finished && agent.turn_entry_key == turn_entry_key {
                agent.collected = true;
                agent.handoff.report_delivered = true;
                agent.report_delivery_key.get_or_insert(key);
            }
        }
    }

    pub fn receive(&mut self, mut value: Value) -> bool {
        if self.disconnected {
            return false;
        }
        let method = value["method"].as_str().map(str::to_owned);
        if method.as_deref() == Some("item/tool/call") && value.get("id").is_some() {
            self.dynamic_call(&value);
            self.update_block_reason();
            return true;
        }
        if method.is_none() {
            let Some(route) = value["id"].as_u64().and_then(|id| self.routes.remove(&id)) else {
                return false;
            };
            value["id"] = route.local_id;
            if route.agent == 0 && route.method == "thread/resume" && self.recovering_empty_lead {
                self.recovering_empty_lead = false;
                if value["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.starts_with("no rollout found for thread id "))
                {
                    self.lead_started = false;
                    self.agents[0]
                        .session
                        .replace_empty_thread(value["id"].as_u64());
                    return true;
                }
            }
            let failed_start = route.agent > 0
                && matches!(
                    route.method.as_str(),
                    "thread/start" | "thread/resume" | "turn/start"
                )
                && (value.get("error").is_some()
                    || (route.method != "turn/start"
                        && !value["result"]["thread"]["id"].is_string())
                    || (route.method == "turn/start"
                        && !value["result"]["turn"]["id"].is_string()));
            let failed_lead_start = route.agent == 0
                && route.method == "turn/start"
                && (value.get("error").is_some() || !value["result"]["turn"]["id"].is_string());
            let changed = self.agents[route.agent].session.receive(value.clone());
            if route.agent == 0
                && matches!(route.method.as_str(), "thread/start" | "thread/resume")
                && self.agents[0].session.thread_id.is_none()
            {
                for agent in self.agents.iter_mut().skip(1) {
                    agent
                        .session
                        .disconnected("The lead conversation could not open.");
                }
            }
            if route.agent == 0 && route.method == "turn/start" && !failed_lead_start {
                self.lead_started = true;
                // A local submission or drained request is not proof that Codex accepted it.
                // Match the exact reports attached to this request, not later worker results.
                self.mark_reports_delivered(route.lead_reports);
            }
            if route.agent > 0 && route.method == "turn/interrupt" && value.get("error").is_some() {
                self.agents[route.agent].cancel_requested = false;
            }
            if route.agent > 0 && value.get("error").is_none() {
                let (lead, workers) = self.agents.split_at_mut(1);
                let source = &workers[route.agent - 1].session;
                if route.method == "skills/list" {
                    lead[0].session.skills.clone_from(&source.skills);
                    lead[0].session.skills_loaded = source.skills_loaded;
                }
                if route.method == "account/rateLimits/read" {
                    lead[0].session.quota.clone_from(&source.quota);
                    lead[0].session.quota_loaded = source.quota_loaded;
                    lead[0]
                        .session
                        .quota_updated_at
                        .clone_from(&source.quota_updated_at);
                }
            }
            if self.agents[route.agent].finished
                && !self.agents[route.agent].session.connecting()
                && !self.agents[route.agent].session.busy()
                && self.agents[route.agent].session.thread_id.is_some()
            {
                self.agents[route.agent].session.status = self.agents[route.agent]
                    .terminal_status
                    .clone()
                    .unwrap_or_else(|| "finished".into());
            }
            if failed_lead_start {
                self.lead_cancelled = true;
                self.compact_lead_after_turn = false;
                self.cancel_automatic();
                self.resolve_parent_waits();
            }
            if failed_start {
                self.finish_worker(route.agent, "failed");
            }
            self.tick();
            return changed;
        }
        let method = method.unwrap();
        if method == "account/rateLimits/updated"
            || method == "skills/changed"
            || method == "account/updated"
        {
            // One account and catalog owner avoids duplicate refresh requests.
            let changed = self.agents[0].session.receive(value);
            return self.tick() | changed;
        }
        if method == "serverRequest/resolved" {
            let request_id = &value["params"]["requestId"];
            let previous_outbox_len = self.outbox.len();
            self.outbox.retain(|reply| reply["id"] != *request_id);
            self.pending_deliveries.remove(&request_id.to_string());
            let removed_reply = self.outbox.len() != previous_outbox_len;
            if removed_reply && let Some(key) = self.rpc_calls.get(&request_id.to_string()) {
                self.calls.insert(
                    key.clone(),
                    tool_result(json!({"error":"Request was resolved by Codex."}), false),
                );
                self.call_reports.remove(key);
            }
            let mut changed = removed_reply
                || self
                    .waits
                    .iter()
                    .any(|wait| wait.rpc_ids.contains(request_id));
            for agent in &mut self.agents {
                let prior = agent.session.approvals.len();
                agent
                    .session
                    .approvals
                    .retain(|approval| approval.id != *request_id);
                changed |= prior != agent.session.approvals.len();
            }
            for wait in &mut self.waits {
                wait.rpc_ids.retain(|id| id != request_id);
            }
            let abandoned: Vec<String> = self
                .waits
                .iter()
                .filter(|wait| wait.rpc_ids.is_empty())
                .map(|wait| wait.key.clone())
                .collect();
            self.waits.retain(|wait| !wait.rpc_ids.is_empty());
            for key in abandoned {
                self.calls.insert(
                    key,
                    tool_result(json!({"error":"Request was resolved by Codex."}), false),
                );
            }
            for agent in &mut self.agents {
                changed |= agent.session.receive(value.clone());
            }
            return changed;
        }
        let thread = value["params"]["threadId"].as_str();
        let Some(index) = self
            .agents
            .iter()
            .position(|agent| thread.is_some() && agent.session.thread_id.as_deref() == thread)
        else {
            if let Some(id) = value.get("id") {
                self.outbox.push(
                    json!({"id":id,"error":{"code":-32602,"message":"Unknown Melly thread"}}),
                );
                return true;
            }
            return false;
        };
        if value.get("id").is_some()
            && index > 0
            && self.agents[index].mode == WorkerMode::Read
            && matches!(
                method.as_str(),
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
            )
        {
            self.outbox
                .push(json!({"id":value["id"],"result":{"decision":"decline"}}));
            self.agents[index]
                .session
                .notice("Read-only worker approval declined.");
            return true;
        }
        if index == 0 && method == "turn/started" {
            self.lead_started = true;
        }
        let completed = method == "turn/completed";
        let status = value["params"]["turn"]["status"]
            .as_str()
            .unwrap_or("completed")
            .to_owned();
        // Read live completions only. History loads can assign new keys to old commands.
        let command_item = (method == "item/completed"
            && value["params"]["item"]["type"] == "commandExecution"
            && self.agents[index].session.active_turn_id().is_some()
            && value["params"]["turnId"].as_str() == self.agents[index].session.active_turn_id())
        .then(|| value["params"]["item"]["id"].as_str())
        .flatten()
        .filter(|id| {
            !self.agents[index].session.entries.iter().any(|entry| {
                entry.id.as_deref() == Some(id) && entry.complete && entry.command_result.is_some()
            })
        })
        .map(|id| {
            let output = value["params"]["item"]["aggregatedOutput"]
                .as_str()
                .filter(|output| !output.is_empty())
                .map(|output| clip(output, MAX_COMMAND_OUTPUT));
            (id.to_owned(), output)
        });
        let changed = self.agents[index].session.receive(value);
        if let Some((id, output)) = command_item {
            self.record_command(index, &id, output);
        }
        if completed && changed {
            if index > 0 {
                if self.agents[index].finished {
                    let agent = &mut self.agents[index];
                    agent.cancel_requested = false;
                    agent.session.status = agent
                        .terminal_status
                        .clone()
                        .unwrap_or_else(|| "Ready".into());
                } else {
                    self.finish_worker(index, &status);
                }
            } else {
                self.resolve_parent_waits();
                if matches!(status.as_str(), "failed" | "interrupted") {
                    self.cancel_automatic();
                }
                let compact = std::mem::take(&mut self.compact_lead_after_turn);
                if compact && status == "completed" && !self.lead_cancelled {
                    self.compact(0);
                }
            }
        }
        self.tick();
        changed
    }

    fn record_command(&mut self, index: usize, id: &str, output: Option<String>) {
        let agent = &mut self.agents[index];
        let Some(entry) = agent
            .session
            .entries
            .iter()
            .find(|entry| entry.id.as_deref() == Some(id))
        else {
            return;
        };
        let Some((command, exit_code)) = &entry.command_result else {
            return;
        };
        let observed = ObservedCommand {
            command: clip(command, MAX_COMMAND_TEXT),
            exit_code: *exit_code,
            output,
        };
        if index == 0 {
            self.lead_commands.retain(|(key, _)| *key != entry.key);
            self.lead_commands.insert(0, (entry.key, observed));
            self.lead_commands.truncate(MAX_COMMANDS);
        } else if !agent.finished && entry.key > agent.turn_entry_key {
            agent.handoff.commands.insert(0, observed);
            agent.handoff.commands.truncate(MAX_COMMANDS);
        }
    }

    /// Poll pending worker starts and held wait calls without starting a lead turn.
    pub fn tick(&mut self) -> bool {
        if self.disconnected {
            return false;
        }
        let mut changed = false;
        for index in 1..self.agents.len() {
            let agent = &mut self.agents[index];
            if !agent.finished
                && !agent.session.connecting()
                && let Some(prompt) = agent.pending_prompt.take()
            {
                if agent.session.submit(&prompt) {
                    agent.turn_entry_key =
                        agent.session.entries.back().map_or(0, |entry| entry.key);
                    changed = true;
                } else {
                    agent.pending_prompt = Some(prompt);
                }
            }
        }
        let mut pending = Vec::new();
        for wait in std::mem::take(&mut self.waits) {
            let timed_out = Instant::now() >= wait.deadline;
            match wait.kind {
                WaitFor::Workers(ref indices) => {
                    let done = indices.iter().all(|&index| self.agents[index].finished);
                    if done || timed_out {
                        let output = self.status_output(indices, !done && timed_out);
                        self.complete_call(wait.key, wait.rpc_ids, output, true);
                        changed = true;
                    } else {
                        pending.push(wait);
                    }
                }
                WaitFor::Quota(_)
                    if self.agents[0].session.quota_pending()
                        && self.block_reason().is_some_and(|reason| {
                            matches!(reason.as_str(), QUOTA_MISSING | QUOTA_STALE)
                        })
                        && !timed_out =>
                {
                    pending.push(wait)
                }
                WaitFor::Quota(brief) => {
                    if let Some(wait) = self.launch_call(wait.key, wait.rpc_ids, brief) {
                        pending.push(wait);
                    }
                    changed = true;
                }
            }
        }
        self.waits = pending;
        self.sync_globals();
        changed | self.update_block_reason()
    }

    pub fn disconnected(&mut self, message: &str) {
        self.disconnected = true;
        self.routes.clear();
        self.waits.clear();
        self.calls.clear();
        self.call_reports.clear();
        self.pending_deliveries.clear();
        self.pending_lead_reports.clear();
        self.rpc_calls.clear();
        self.outbox.clear();
        for (index, agent) in self.agents.iter_mut().enumerate() {
            agent.session.disconnected(message);
            agent.pending_prompt = None;
            if index > 0 && !agent.finished {
                agent.finished = true;
                agent.terminal_status = Some("disconnected".into());
                agent.result = Some("Worker disconnected before completion.".into());
            }
        }
        self.update_block_reason();
    }

    pub fn busy(&self) -> bool {
        self.agents[0].session.connecting()
            || self.agents[0].session.busy()
            || self
                .agents
                .iter()
                .skip(1)
                .any(|agent| agent.session.connecting())
            || self.active_workers() > 0
            || !self.waits.is_empty()
    }

    /// Local harness state, independent of model-generated progress messages.
    pub fn lead_wait_status(&self) -> Option<&'static str> {
        if self
            .waits
            .iter()
            .any(|wait| matches!(wait.kind, WaitFor::Workers(_)))
        {
            Some("Waiting for Luna")
        } else if !self.waits.is_empty() {
            Some("Checking quota")
        } else {
            None
        }
    }

    pub fn active_workers(&self) -> usize {
        self.agents
            .iter()
            .skip(1)
            .filter(|agent| !agent.finished || agent.session.compacting())
            .count()
    }

    pub fn auto_launches(&self) -> usize {
        self.automatic_count
    }

    pub fn spawn_manual(
        &mut self,
        task: String,
        mode: WorkerMode,
        paths: Vec<String>,
    ) -> Result<usize, String> {
        self.spawn(
            Brief {
                objective: task,
                context: String::new(),
                expected_output: "Report the result and relevant file references concisely.".into(),
                reason: "User requested this worker.".into(),
                mode,
                paths,
                wait_for_result: false,
            },
            AgentSource::Manual,
        )
    }

    fn spawn(&mut self, mut brief: Brief, source: AgentSource) -> Result<usize, String> {
        for path in &mut brief.paths {
            *path = path.trim_end_matches('/').to_owned();
        }
        brief.validate()?;
        if self.disconnected || self.agents[0].session.connecting() {
            return Err("Wait for the lead session to connect.".into());
        }
        if self.active_workers() >= MAX_ACTIVE {
            return Err(format!(
                "At most {MAX_ACTIVE} workers may be active, including starting and cancelling workers."
            ));
        }
        if source == AgentSource::Automatic {
            self.update_block_reason();
            if let Some(reason) = &self.automatic_block_reason {
                return Err(reason.clone());
            }
        }
        self.validate_ownership(brief.mode, &brief.paths, None)?;
        let model = self.agents[0]
            .session
            .models
            .iter()
            .find(|model| model.id == "gpt-6-luna")
            .cloned()
            .ok_or_else(|| {
                "gpt-6-luna is not advertised in the connected Codex model catalog.".to_owned()
            })?;
        let mut session = Session::worker(
            self.agents[0].session.cwd.clone(),
            model,
            brief.mode == WorkerMode::Read,
        );
        session.account.clone_from(&self.agents[0].session.account);
        let index = self.agents.len();
        let prompt = brief.prompt();
        self.agents.push(Agent {
            id: format!(
                "luna-{}",
                self.agents
                    .iter()
                    .filter_map(|agent| agent
                        .id
                        .strip_prefix("luna-")
                        .and_then(|n| n.parse::<usize>().ok()))
                    .max()
                    .unwrap_or(0)
                    + 1
            ),
            title: clip(&brief.objective, 70),
            source,
            session,
            owned_paths: brief.paths,
            mode: brief.mode,
            finished: false,
            result: None,
            handoff: Handoff {
                brief: Some(AssignmentBrief {
                    objective: brief.objective,
                    context: brief.context,
                    expected_output: brief.expected_output,
                    reason: brief.reason,
                }),
                prompt: Some(prompt.clone()),
                ..Handoff::default()
            },
            report_delivery_key: None,
            pending_prompt: Some(prompt),
            collected: false,
            terminal_status: None,
            cancel_requested: false,
            turn_entry_key: 0,
        });
        if source == AgentSource::Automatic {
            self.automatic_count += 1;
        }
        self.update_block_reason();
        Ok(index)
    }

    pub fn submit(&mut self, index: usize, prompt: &str) -> bool {
        if index >= self.agents.len() || self.disconnected || prompt.trim().is_empty() {
            return false;
        }
        if index == 0 {
            let new_turn = !self.agents[0].session.busy();
            let results = if new_turn {
                self.completed_text()
            } else {
                String::new()
            };
            let input = if results.is_empty() {
                prompt.to_owned()
            } else {
                format!(
                    "{prompt}\n\nCompleted Melly worker reports (verify these findings):\n{results}"
                )
            };
            if !self.agents[0].session.submit(&input) {
                return false;
            }
            if new_turn {
                self.automatic_count = 0;
                self.compact_requested = false;
                self.lead_cancelled = false;
                self.pending_lead_reports = self
                    .agents
                    .iter()
                    .enumerate()
                    .skip(1)
                    .filter(|(_, agent)| agent.finished && !agent.collected)
                    .map(|(index, agent)| (index, agent.turn_entry_key))
                    .collect();
                self.update_block_reason();
            }
            return true;
        }
        if prompt.trim().is_empty() || prompt.chars().count() > 2000 {
            self.agents[index]
                .session
                .notice("Worker continuation must contain 1–2000 characters.");
            return false;
        }
        if self.agents[index].cancel_requested {
            self.agents[index]
                .session
                .notice("Wait for this worker to finish cancelling.");
            return false;
        }
        if self.agents[index].finished {
            if self.active_workers() >= MAX_ACTIVE {
                self.agents[index]
                    .session
                    .notice(format!("At most {MAX_ACTIVE} workers may be active."));
                return false;
            }
            if let Err(error) = self.validate_ownership(
                self.agents[index].mode,
                &self.agents[index].owned_paths,
                Some(index),
            ) {
                self.agents[index].session.notice(error);
                return false;
            }
        }
        let new_worker_turn = !self.agents[index].session.busy();
        if self.agents[index].session.submit(prompt) {
            let agent = &mut self.agents[index];
            // An explicit user continuation is manual work, regardless of the original launch.
            agent.source = AgentSource::Manual;
            if new_worker_turn {
                agent.turn_entry_key = agent.session.entries.back().map_or(0, |entry| entry.key);
            }
            let handoff = &mut agent.handoff;
            if handoff.corrections.len() == MAX_CORRECTIONS {
                handoff.corrections.remove(0);
                handoff.corrections_omitted = handoff.corrections_omitted.saturating_add(1);
            }
            handoff.corrections.push(prompt.to_owned());
            if new_worker_turn {
                agent.handoff.commands.clear();
                agent.handoff.review = None;
                agent.handoff.report_delivered = false;
                agent.report_delivery_key = None;
            }
            agent.finished = false;
            agent.result = None;
            agent.terminal_status = None;
            agent.collected = false;
            true
        } else {
            false
        }
    }

    pub fn compact(&mut self, index: usize) -> bool {
        if self.disconnected || index >= self.agents.len() {
            return false;
        }
        if index > 0 && self.agents[index].finished && self.active_workers() >= MAX_ACTIVE {
            self.agents[index].session.notice(format!(
                "Wait for a free worker slot (maximum {MAX_ACTIVE}) before compacting."
            ));
            return false;
        }
        let session = &mut self.agents[index].session;
        if session.compact() {
            true
        } else {
            session.notice("Compaction is available when this agent is connected and idle.");
            false
        }
    }

    pub fn cancel(&mut self, index: usize) -> bool {
        if index >= self.agents.len() {
            return false;
        }
        if index == 0 {
            self.lead_cancelled = true;
            self.compact_lead_after_turn = false;
            self.cancel_automatic();
            self.resolve_parent_waits();
            return self.agents[0].session.interrupt();
        }
        if self.agents[index].finished {
            return self.agents[index].session.interrupt();
        }
        self.agents[index].pending_prompt = None;
        self.agents[index].cancel_requested = true;
        if self.agents[index].session.interrupt() {
            true
        } else {
            self.finish_worker(index, "cancelled");
            true
        }
    }

    /// Collect report text for display or an explicitly submitted next prompt.
    pub fn collect_completed(&mut self) -> String {
        let text = self.completed_text();
        self.mark_collected();
        text
    }

    fn completed_text(&self) -> String {
        self.agents
            .iter()
            .skip(1)
            .filter(|agent| agent.finished && !agent.collected)
            .map(|agent| {
                format!(
                    "{} · {} · {}\n{}",
                    agent.id,
                    agent.title,
                    agent.terminal_status.as_deref().unwrap_or("finished"),
                    agent.result.as_deref().unwrap_or("No final message.")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn mark_collected(&mut self) {
        for agent in self
            .agents
            .iter_mut()
            .skip(1)
            .filter(|agent| agent.finished)
        {
            agent.collected = true;
        }
    }

    fn validate_ownership(
        &self,
        mode: WorkerMode,
        paths: &[String],
        except: Option<usize>,
    ) -> Result<(), String> {
        let requested = paths
            .iter()
            .map(|path| ownership_key(&self.agents[0].session.cwd, path))
            .collect::<Result<Vec<_>, _>>()?;
        if mode == WorkerMode::Edit {
            if paths.is_empty() {
                return Err("Edit workers require at least one owned relative path.".into());
            }
            for (index, agent) in self.agents.iter().enumerate().skip(1) {
                if Some(index) == except || agent.finished || agent.mode != WorkerMode::Edit {
                    continue;
                }
                let owned = agent
                    .owned_paths
                    .iter()
                    .map(|path| ownership_key(&agent.session.cwd, path))
                    .collect::<Result<Vec<_>, _>>()?;
                for (path, key) in paths.iter().zip(&requested) {
                    if owned
                        .iter()
                        .any(|owned| key.starts_with(owned) || owned.starts_with(key))
                    {
                        return Err(format!(
                            "Path {path} overlaps active ownership by {}.",
                            agent.id
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn finish_worker(&mut self, index: usize, status: &str) {
        let agent = &mut self.agents[index];
        if agent.finished {
            return;
        }
        agent.finished = true;
        agent.pending_prompt = None;
        agent.cancel_requested = false;
        agent.terminal_status = Some(status.into());
        let result = agent
            .session
            .entries
            .iter()
            .rev()
            .find(|entry| {
                entry.speaker == Speaker::Codex
                    && entry.outcome.as_deref() != Some("commentary")
                    && entry.complete
                    && entry.key > agent.turn_entry_key
            })
            .map(|entry| clip(&entry.text, MAX_RESULT));
        agent.result =
            Some(result.unwrap_or_else(|| format!("Worker {status}; no final assistant message.")));
        agent.session.status = status.into();
    }

    fn cancel_automatic(&mut self) {
        let indices: Vec<usize> = self
            .agents
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, agent)| agent.source == AgentSource::Automatic && !agent.finished)
            .map(|(index, _)| index)
            .collect();
        for index in indices {
            self.cancel(index);
        }
    }

    fn resolve_parent_waits(&mut self) {
        // A prepared result has not been delivered until drain_requests hands it to transport.
        // If the lead ends first, preserve these reports for the next explicit user prompt.
        let cancelled = tool_result(json!({"error":"Lead turn ended or was cancelled."}), false);
        for rpc_id in std::mem::take(&mut self.pending_deliveries).into_keys() {
            for reply in &mut self.outbox {
                // Keys are serialized JSON IDs: numeric 1 and string "1" are distinct.
                let serialized_reply_id = reply["id"].to_string();
                if serialized_reply_id == rpc_id {
                    reply["result"] = cancelled.clone();
                }
            }
            if let Some(key) = self.rpc_calls.get(&rpc_id) {
                self.calls.insert(key.clone(), cancelled.clone());
                self.call_reports.remove(key);
            }
        }
        for wait in std::mem::take(&mut self.waits) {
            self.complete_call(
                wait.key,
                wait.rpc_ids,
                json!({"error":"Lead turn ended or was cancelled."}),
                false,
            );
        }
    }

    fn sync_globals(&mut self) {
        let (lead, workers) = self.agents.split_at_mut(1);
        for agent in workers {
            if agent.session.models.is_empty() {
                agent.session.models.extend(
                    lead[0]
                        .session
                        .models
                        .iter()
                        .filter(|model| model.id == "gpt-6-luna")
                        .cloned(),
                );
            }
            agent.session.account.clone_from(&lead[0].session.account);
            agent.session.quota.clone_from(&lead[0].session.quota);
            agent.session.quota_loaded = lead[0].session.quota_loaded;
            agent
                .session
                .quota_updated_at
                .clone_from(&lead[0].session.quota_updated_at);
            agent.session.skills.clone_from(&lead[0].session.skills);
            agent.session.skills_loaded = lead[0].session.skills_loaded;
        }
    }

    fn update_block_reason(&mut self) -> bool {
        let previous = self.automatic_block_reason.clone();
        self.automatic_block_reason = self.block_reason();
        previous != self.automatic_block_reason
    }

    fn block_reason(&self) -> Option<String> {
        if self.disconnected {
            return Some("Codex is disconnected.".into());
        }
        if self.lead_cancelled {
            return Some("Lead cancellation requested; wait for a new user turn before automatic delegation.".into());
        }
        if !self.automatic_enabled {
            return Some("Automatic delegation is off in /settings.".into());
        }
        if self.automatic_count >= MAX_AUTOMATIC {
            return Some(format!(
                "Automatic limit of {MAX_AUTOMATIC} launches reached for this user turn."
            ));
        }
        if self.active_workers() >= MAX_ACTIVE {
            return Some(format!("All {MAX_ACTIVE} worker slots are occupied."));
        }
        let lead = &self.agents[0].session;
        if !lead.models_loaded {
            return Some("Waiting for the Codex model catalog.".into());
        }
        if !lead.models.iter().any(|model| model.id == "gpt-6-luna") {
            return Some("gpt-6-luna is not advertised in the Codex model catalog.".into());
        }
        let model = lead
            .active_model
            .as_deref()
            .or(lead.model.as_deref())
            .unwrap_or("");
        let relevant: Vec<_> = lead
            .quota
            .iter()
            .filter(|quota| {
                let id = quota.bucket_id.to_ascii_lowercase();
                id == "codex"
                    || id.contains("luna")
                    || (!model.is_empty()
                        && (id.contains(model) || quota.name.to_ascii_lowercase().contains(model)))
            })
            .collect();
        if relevant.is_empty() {
            return Some(QUOTA_MISSING.into());
        }
        for quota in relevant {
            if lead
                .quota_updated_at
                .get(&quota.bucket_id)
                .is_none_or(|time| time.elapsed() > QUOTA_MAX_AGE)
            {
                return Some(QUOTA_STALE.into());
            }
            if quota.used_percent >= 85 {
                return Some(format!(
                    "Automatic delegation paused to preserve the 15% Codex quota reserve ({}: {}% used).",
                    quota.name, quota.used_percent
                ));
            }
        }
        None
    }

    fn status_output(&self, indices: &[usize], timed_out: bool) -> Value {
        json!({"agents":indices.iter().map(|&index| {
            let agent = &self.agents[index];
            json!({"id":agent.id,"title":agent.title,"status":agent.session.status,
                "finished":agent.finished,"model":agent.session.active_model.as_ref().or(agent.session.model.as_ref()),
                "tokens":agent.session.tokens,"context_used":agent.session.context_used,
                "context_window":agent.session.context_window,"paths":agent.owned_paths,
                "result":if agent.finished { agent.result.as_deref() } else { None },
                "observed_commands":agent.handoff.commands.iter()
                    .map(|entry| json!({"command":entry.command,"exit_code":entry.exit_code}))
                    .collect::<Vec<_>>()})
        }).collect::<Vec<_>>(),"timed_out":timed_out})
    }

    fn complete_call(&mut self, key: String, rpc_ids: Vec<Value>, output: Value, success: bool) {
        let reports = if success {
            output["agents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|report| report["finished"] == true && report["result"].is_string())
                .filter_map(|report| {
                    report["id"]
                        .as_str()
                        .and_then(|id| self.worker_index(id).ok())
                })
                .map(|index| (index, self.agents[index].turn_entry_key))
                .collect()
        } else {
            Vec::new()
        };
        let result = tool_result(output, success);
        self.call_reports.insert(key.clone(), reports);
        self.calls.insert(key.clone(), result.clone());
        for id in rpc_ids {
            self.queue_call_reply(&key, id, result.clone());
        }
    }

    fn queue_call_reply(&mut self, key: &str, id: Value, result: Value) {
        if let Some(reports) = self
            .call_reports
            .get(key)
            .filter(|reports| !reports.is_empty())
        {
            self.pending_deliveries
                .insert(id.to_string(), reports.clone());
        }
        self.outbox.push(json!({"id":id,"result":result}));
    }

    fn dynamic_call(&mut self, value: &Value) {
        let rpc_id = value["id"].clone();
        let params = &value["params"];
        let key = format!(
            "{}:{}:{}",
            params["threadId"], params["turnId"], params["callId"]
        );
        let rpc_key = rpc_id.to_string();
        let key = self.rpc_calls.get(&rpc_key).cloned().unwrap_or(key);
        self.rpc_calls.insert(rpc_key.clone(), key.clone());
        if let Some(result) = self.calls.get(&key).cloned() {
            self.queue_call_reply(&key, rpc_id, result);
            return;
        }
        if let Some(wait) = self.waits.iter_mut().find(|wait| wait.key == key) {
            if !wait.rpc_ids.contains(&rpc_id) {
                wait.rpc_ids.push(rpc_id);
            }
            return;
        }
        let lead = &self.agents[0].session;
        if params["namespace"] != "melly_agents"
            || params["threadId"].as_str() != lead.thread_id.as_deref()
            || lead.active_turn_id().is_none()
            || params["turnId"].as_str() != lead.active_turn_id()
            || params["callId"].as_str().is_none_or(str::is_empty)
        {
            self.complete_call(
                key,
                vec![rpc_id],
                json!({"error":"Only the active lead turn may call melly_agents tools."}),
                false,
            );
            return;
        }
        let arguments = if let Some(text) = params["arguments"].as_str() {
            match serde_json::from_str::<Value>(text) {
                Ok(value) => value,
                Err(_) => {
                    self.complete_call(
                        key,
                        vec![rpc_id],
                        json!({"error":"Tool arguments must be a JSON object."}),
                        false,
                    );
                    return;
                }
            }
        } else {
            params["arguments"].clone()
        };
        let result: Result<Value, String> = match params["tool"].as_str().unwrap_or("") {
            "spawn" => match Brief::parse(&arguments) {
                Ok(brief) if self.defer_for_quota(&brief) => {
                    self.waits.push(Wait {
                        key,
                        rpc_ids: vec![rpc_id],
                        kind: WaitFor::Quota(brief),
                        deadline: Instant::now() + Duration::from_secs(8),
                    });
                    return;
                }
                Ok(brief) => {
                    if let Some(wait) = self.launch_call(key, vec![rpc_id], brief) {
                        self.waits.push(wait);
                    }
                    return;
                }
                Err(error) => Err(error),
            },
            "revise" => self.revise_worker(&arguments),
            "review" => self.review_worker(&arguments),
            "status" => self
                .parse_ids(&arguments)
                .map(|indices| self.status_output(&indices, false)),
            "compact" => self.request_compaction(&arguments),
            "cancel" => arguments["id"]
                .as_str()
                .ok_or_else(|| "cancel requires an agent id.".to_owned())
                .and_then(|id| self.worker_index(id))
                .map(|index| {
                    self.cancel(index);
                    self.status_output(&[index], false)
                }),
            "wait" => {
                let indices = self.parse_ids(&arguments);
                let timeout = arguments["timeout_ms"]
                    .as_u64()
                    .filter(|timeout| *timeout <= 120_000);
                match (indices, timeout) {
                    (Ok(indices), Some(timeout)) => {
                        let done = indices.iter().all(|&index| self.agents[index].finished);
                        if done || timeout == 0 {
                            Ok(self.status_output(&indices, !done))
                        } else {
                            self.waits.push(Wait {
                                key,
                                rpc_ids: vec![rpc_id],
                                kind: WaitFor::Workers(indices),
                                deadline: Instant::now() + Duration::from_millis(timeout),
                            });
                            return;
                        }
                    }
                    (Err(error), _) => Err(error),
                    (_, None) => Err("wait timeout_ms must be an integer from 0 to 120000.".into()),
                }
            }
            _ => Err("Unknown melly_agents tool.".into()),
        };
        self.complete_result(key, vec![rpc_id], result);
    }

    fn defer_for_quota(&mut self, brief: &Brief) -> bool {
        if brief.validate().is_err()
            || !self
                .block_reason()
                .is_some_and(|reason| matches!(reason.as_str(), QUOTA_MISSING | QUOTA_STALE))
            || self
                .waits
                .iter()
                .filter(|wait| matches!(wait.kind, WaitFor::Quota(_)))
                .count()
                >= MAX_AUTOMATIC
        {
            return false;
        }
        let lead = &mut self.agents[0].session;
        lead.refresh_quota();
        lead.quota_pending()
    }

    fn launch_call(&mut self, key: String, rpc_ids: Vec<Value>, brief: Brief) -> Option<Wait> {
        let wait_for_result = brief.wait_for_result;
        let result = match self.spawn(brief, AgentSource::Automatic) {
            Ok(index) if wait_for_result => {
                // Hold this tool reply, not the event loop. Worker output, approvals,
                // cancellation and other assignments continue through normal routing.
                return Some(Wait {
                    key,
                    rpc_ids,
                    kind: WaitFor::Workers(vec![index]),
                    deadline: Instant::now() + Duration::from_secs(120),
                });
            }
            Ok(index) => {
                Ok(json!({"id":self.agents[index].id,"status":"starting","model":"gpt-6-luna"}))
            }
            Err(error) => Err(error),
        };
        self.complete_result(key, rpc_ids, result);
        None
    }

    fn complete_result(&mut self, key: String, rpc_ids: Vec<Value>, result: Result<Value, String>) {
        match result {
            Ok(output) => self.complete_call(key, rpc_ids, output, true),
            Err(error) => self.complete_call(key, rpc_ids, json!({"error":error}), false),
        }
    }

    fn revise_worker(&mut self, arguments: &Value) -> Result<Value, String> {
        let id = arguments["id"].as_str().ok_or("Provide a worker ID.")?;
        let index = self.worker_index(id)?;
        let feedback = arguments["feedback"]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty() && text.chars().count() <= 1800)
            .ok_or("Provide focused feedback of 1–1800 characters.")?;
        if !self.agents[index].finished || self.agents[index].session.busy() {
            return Err("Wait for this worker to finish before requesting a correction.".into());
        }
        if let Some(reason) = self.block_reason() {
            return Err(reason);
        }
        let prompt = format!(
            "Conductor review feedback: {feedback}\nCorrect this within your original scope, rerun the acceptance checks, and report the outcome."
        );
        if !self.submit(index, &prompt) {
            return Err(
                "The worker could not restart. Check its connection and owned paths.".into(),
            );
        }
        self.agents[index].source = AgentSource::Automatic;
        self.automatic_count += 1;
        Ok(json!({"id":id,"status":"revising"}))
    }

    fn review_worker(&mut self, arguments: &Value) -> Result<Value, String> {
        let id = arguments["id"].as_str().ok_or("Provide a worker ID.")?;
        let index = self.worker_index(id)?;
        let verdict = match arguments["verdict"].as_str() {
            Some("accepted") => ReviewVerdict::Accepted,
            Some("changes_requested") => ReviewVerdict::ChangesRequested,
            _ => return Err("Provide a verdict of accepted or changes_requested.".into()),
        };
        let summary = arguments["summary"]
            .as_str()
            .filter(|text| !text.trim().is_empty() && text.chars().count() <= MAX_REVIEW_SUMMARY)
            .ok_or("Provide a review summary of 1–2000 characters.")?;
        let agent = &mut self.agents[index];
        if !agent.finished || !agent.handoff.report_delivered {
            return Err(
                "Deliver the current finished worker report before recording review.".into(),
            );
        }
        let commands = self
            .lead_commands
            .iter()
            .filter(|(key, _)| {
                agent
                    .report_delivery_key
                    .is_some_and(|boundary| *key > boundary)
            })
            .map(|(_, command)| command.clone())
            .collect();
        agent.handoff.review = Some(ReviewRecord {
            verdict,
            summary: summary.to_owned(),
            commands,
        });
        Ok(json!({"id":id,"verdict":verdict,"status":"recorded"}))
    }

    fn request_compaction(&mut self, arguments: &Value) -> Result<Value, String> {
        if self.lead_cancelled || self.compact_requested {
            return Err("Only one compaction request is allowed per active lead turn.".into());
        }
        let id = arguments["id"].as_str().ok_or("Provide an agent ID.")?;
        let status = if id == "lead" {
            self.compact_lead_after_turn = true;
            "scheduled after the lead turn"
        } else {
            let index = self.worker_index(id)?;
            if !self.compact(index) {
                return Err(
                    "The worker must be connected, idle, and have a free worker slot.".into(),
                );
            }
            "compacting"
        };
        self.compact_requested = true;
        Ok(json!({"id":id,"status":status}))
    }

    fn worker_index(&self, id: &str) -> Result<usize, String> {
        self.agents
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, agent)| agent.id == id)
            .map(|(index, _)| index)
            .ok_or_else(|| format!("Unknown worker {id}."))
    }

    fn parse_ids(&self, arguments: &Value) -> Result<Vec<usize>, String> {
        let ids = arguments["ids"]
            .as_array()
            .ok_or_else(|| "ids must be an array of worker ids.".to_owned())?;
        if ids.is_empty() || ids.len() > 3 {
            return Err("Request between 1 and 3 worker ids.".into());
        }
        ids.iter()
            .map(|id| {
                id.as_str()
                    .ok_or_else(|| "Worker id must be a string.".into())
                    .and_then(|id| self.worker_index(id))
            })
            .collect()
    }
}

struct Brief {
    objective: String,
    context: String,
    expected_output: String,
    reason: String,
    mode: WorkerMode,
    paths: Vec<String>,
    wait_for_result: bool,
}
impl Brief {
    fn parse(value: &Value) -> Result<Self, String> {
        let string = |key: &str| {
            value[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must be a string."))
        };
        let paths = value["paths"]
            .as_array()
            .ok_or_else(|| "paths must be an array.".to_owned())?
            .iter()
            .map(|path| {
                path.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "paths must contain strings.".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mode = match value["mode"].as_str() {
            Some("read") => WorkerMode::Read,
            Some("edit") => WorkerMode::Edit,
            _ => return Err("mode must be read or edit.".into()),
        };
        let wait_for_result = match value.get("wait_for_result") {
            None => false, // Older persisted tool schemas return the worker ID immediately.
            Some(Value::Bool(wait)) => *wait,
            Some(_) => return Err("wait_for_result must be a boolean.".into()),
        };
        Ok(Self {
            objective: string("objective")?,
            context: string("context")?,
            expected_output: string("expected_output")?,
            reason: string("reason")?,
            mode,
            paths,
            wait_for_result,
        })
    }
    fn validate(&self) -> Result<(), String> {
        for (name, value, limit, required) in [
            ("objective", &self.objective, 2000, true),
            ("context", &self.context, 6000, false),
            ("expected_output", &self.expected_output, 1000, true),
            ("reason", &self.reason, 1000, true),
        ] {
            if value.chars().count() > limit || (required && value.trim().is_empty()) {
                return Err(format!(
                    "{name} must {}be at most {limit} characters.",
                    if required { "be nonempty and " } else { "" }
                ));
            }
        }
        if self.paths.len() > 8 {
            return Err("At most 8 owned paths are allowed.".into());
        }
        for path in &self.paths {
            if path.trim().is_empty()
                || path.len() > 512
                || path.starts_with('/')
                || path.starts_with('~')
                || path.contains('\\')
                || path.contains(':')
                || path.chars().any(char::is_control)
                || path
                    .split('/')
                    .any(|part| part == ".." || part == "." || part.is_empty())
            {
                return Err(format!(
                    "Owned path must be a plain relative path without traversal: {path}"
                ));
            }
        }
        let objective = self
            .objective
            .trim()
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase();
        if matches!(
            objective.as_str(),
            "hi" | "hello" | "hey" | "thanks" | "thank you" | "say hello" | "greet the user"
        ) {
            return Err("Delegate worthwhile independent work, not trivial greetings.".into());
        }
        Ok(())
    }
    fn prompt(&self) -> String {
        format!(
            "You are a Melly worker. Complete only this bounded task; do not spawn or delegate to other agents. You are not alone in the codebase: preserve others' edits and accommodate concurrent work.\nMode: {}. {}\nOwned paths: {}\nObjective: {}\nRelevant context: {}\nExpected output: {}\nWhy this is independent: {}\nFor edits, run the acceptance checks yourself, fix failures within scope, and rerun the checks before reporting. Run final acceptance checks as separate commands that preserve their real exit status. Do not mask a failing check with a successful shell exit. Return a concise final report covering changed files, checks and any remaining failures. Flag any check you could not run.",
            self.mode.label(),
            if self.mode == WorkerMode::Read {
                "Inspect only; do not edit files or request extra permissions."
            } else {
                "Edit only your owned paths. Ownership is coordination, not an OS guarantee. Follow the sandbox and approval rules of your own Codex thread."
            },
            if self.paths.is_empty() {
                "None (read-only inspection)".into()
            } else {
                self.paths.join(", ")
            },
            self.objective,
            self.context,
            self.expected_output,
            self.reason
        )
    }
}

/// Resolve existing ancestors so aliases also conflict before a new file exists.
/// This is a coordination check; it cannot prevent filesystem changes after validation.
fn ownership_key(cwd: &str, relative: &str) -> Result<Vec<String>, String> {
    let root = canonical_existing_ancestor(Path::new(cwd))?;
    let target = canonical_existing_ancestor(&root.join(relative))?;
    if !target.starts_with(&root) {
        return Err(format!(
            "Owned path resolves outside the workspace: {relative}"
        ));
    }
    Ok(path_components(&target))
}

fn canonical_existing_ancestor(path: &Path) -> Result<PathBuf, String> {
    let mut ancestor = path.to_owned();
    let mut missing = Vec::new();
    loop {
        match std::fs::canonicalize(&ancestor) {
            Ok(mut canonical) => {
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) => {
                // Existing dangling symlinks and inaccessible aliases must not be treated
                // as ordinary uncreated paths.
                if error.kind() != std::io::ErrorKind::NotFound
                    || std::fs::symlink_metadata(&ancestor).is_ok()
                {
                    return Err(format!(
                        "Cannot resolve owned path {}: {error}",
                        path.display()
                    ));
                }
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| format!("Cannot resolve workspace path {}", path.display()))?
                    .to_owned();
                missing.push(name);
                if !ancestor.pop() {
                    return Err(format!("Cannot resolve workspace path {}", path.display()));
                }
            }
        }
    }
}

fn path_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|part| {
            let value = part.as_os_str().to_string_lossy().into_owned();
            if cfg!(target_os = "macos") {
                value.to_lowercase()
            } else {
                value
            }
        })
        .collect()
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_owned()
    } else {
        format!(
            "{}…",
            text.chars()
                .take(limit.saturating_sub(1))
                .collect::<String>()
        )
    }
}
fn tool_result(output: Value, success: bool) -> Value {
    json!({"contentItems":[{"type":"inputText","text":output.to_string()}],"success":success})
}

fn dynamic_tools() -> Value {
    let ids = json!({"type":"array","items":{"type":"string"},"minItems":1,"maxItems":3});
    let tools = vec![
        json!({"type":"function","name":"spawn","description":"Assign worthwhile work to Luna. Use edit mode for implementation and include explicit acceptance checks so Luna can edit, test, and iterate independently. Never delegate trivial greetings or the same work twice. Include the evidence and chosen implementation steps in context, behavior to preserve, and concrete acceptance checks. Keep conductor permissions out of the brief. At most 2 worker turns (spawns and revisions) per user turn and 3 active workers total. Read mode cannot edit and can use an empty paths array. Any supplied paths must be relative to the workspace. Edit mode requires disjoint owned paths; preserve concurrent edits and configured permissions. Workers cannot delegate. Set wait_for_result=true when ready to wait for this assignment: returns its report and check evidence on completion, or status after 120 seconds. Omit or set false to return the worker ID immediately for parallel work. In functions.exec, use yield_time_ms 60000 for waiting calls.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["objective","context","expected_output","reason","mode","paths"],"properties":{
                "objective":{"type":"string","minLength":1,"maxLength":2000},"context":{"type":"string","maxLength":6000,"description":"Relevant findings, implementation steps, and behavior to preserve."},
                "expected_output":{"type":"string","minLength":1,"maxLength":1000,"description":"Expected changes, acceptance checks, and the report to return."},"reason":{"type":"string","minLength":1,"maxLength":1000},
                "mode":{"type":"string","enum":["read","edit"]},"paths":{"type":"array","maxItems":8,"items":{"type":"string","maxLength":512}},
                "wait_for_result":{"type":"boolean","description":"Wait for this worker's result in the same call; default false. Use true when no other conductor work is pending."}}}}),
        json!({"type":"function","name":"wait","description":"Wait without polling model turns for these workers to finish or for a bounded timeout. Returns final reports, observed command exit codes, model usage and owned paths. After implementation, inspect the actual diff and check the evidence before accepting. No hidden lead turn is started.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["ids","timeout_ms"],"properties":{"ids":ids,"timeout_ms":{"type":"integer","minimum":0,"maximum":120000}}}}),
        json!({"type":"function","name":"status","description":"Read compact worker status and completed reports when useful; avoid repetitive polling. Prefer wait when you need a result.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["ids"],"properties":{"ids":ids}}}),
        json!({"type":"function","name":"revise","description":"Send one focused correction citing a defect in the diff or checks. Keep conductor permissions out of feedback. The same worker keeps its context and owned paths. Counts against the shared worker-turn cap.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["id","feedback"],"properties":{"id":{"type":"string"},"feedback":{"type":"string","maxLength":1800}}}}),
        json!({"type":"function","name":"review","description":"Record your assessment of a current finished worker after its report was delivered. Inspect the diff and check evidence before recording acceptance. The verdict is your assertion, and observed conductor command outcomes are attached separately.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["id","verdict","summary"],"properties":{"id":{"type":"string"},"verdict":{"type":"string","enum":["accepted","changes_requested"]},"summary":{"type":"string","minLength":1,"maxLength":MAX_REVIEW_SUMMARY}}}}),
        json!({"type":"function","name":"compact","description":"When context is crowded, compact an idle worker or schedule the lead to compact after this turn. Use sparingly: compaction is a model operation. One successful request per lead turn.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":{"type":"string","description":"lead or the ID of an idle worker"}}}}),
        json!({"type":"function","name":"cancel","description":"Cancel an unneeded worker to avoid spending further usage. Running workers retain their slot until cancellation completes.","deferLoading":false,
            "inputSchema":{"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":{"type":"string"}}}}),
    ];
    json!([{"type":"namespace","name":"melly_agents","description":"Bounded independent Melly workers on the same Codex connection. The lead coordinates and verifies final results.","tools":tools}])
}
