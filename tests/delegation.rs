use melly::delegation::{Harness, WorkerMode};
use serde_json::{Value, json};

fn ready(quota: Option<u64>, resume: bool) -> Harness {
    let mut h = Harness::new(
        "/tmp/project".into(),
        None,
        resume.then(|| "lead-thread".into()),
    );
    let init = h.drain_requests().remove(0);
    assert_eq!(init["params"]["capabilities"]["experimentalApi"], true);
    h.receive(json!({"id":init["id"],"result":{}}));
    let requests = h.drain_requests();
    for request in &requests {
        match request["method"].as_str().unwrap_or("") {
            "account/read" => {
                h.receive(json!({"id":request["id"],"result":{"account":{"type":"chatgpt"}}}));
            }
            "account/rateLimits/read" => {
                h.receive(json!({"id":request["id"],"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":quota}}}}));
            }
            "model/list" => {
                h.receive(json!({"id":request["id"],"result":{"data":[
                    {"model":"gpt-6.1-sol","defaultReasoningEffort":"medium"},
                    {"model":"gpt-6-luna","defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"low"}]}
                ]}}));
            }
            _ => {}
        }
    }
    let open = if resume {
        requests
            .iter()
            .find(|r| r["method"] == "thread/resume")
            .unwrap()
            .clone()
    } else {
        h.drain_requests()
            .into_iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap()
    };
    if resume {
        assert!(open["params"].get("dynamicTools").is_none());
    } else {
        assert_eq!(open["params"]["dynamicTools"][0]["type"], "namespace");
        assert_eq!(open["params"]["dynamicTools"][0]["name"], "melly_agents");
        assert_eq!(
            open["params"]["dynamicTools"][0]["tools"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
    }
    h.receive(
        json!({"id":open["id"],"result":{"thread":{"id":"lead-thread"},"model":"gpt-6.1-sol"}}),
    );
    h.drain_requests();
    h
}

fn start_lead(h: &mut Harness) {
    assert!(h.submit(
        0,
        "Review the implementation and delegate independent checks."
    ));
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":request["id"],"result":{"turn":{"id":"lead-turn"}}}));
}

fn tool(h: &mut Harness, id: &str, name: &str, arguments: Value) -> Vec<Value> {
    h.receive(json!({"id":id,"method":"item/tool/call","params":{
        "threadId":"lead-thread","turnId":"lead-turn","callId":id,"namespace":"melly_agents","tool":name,"arguments":arguments
    }}));
    h.drain_requests()
}

fn spawn_args(paths: Value) -> Value {
    json!({"objective":"Review parsing edge cases", "context":"Inspect the existing parser and tests.",
        "expected_output":"A concise list of actionable findings with file references.",
        "reason":"Independent inspection can proceed while the lead checks the interface.",
        "mode":"read","paths":paths})
}

fn reply_output(requests: &[Value], id: &str) -> Value {
    let reply = requests.iter().find(|r| r["id"] == id).unwrap();
    serde_json::from_str(reply["result"]["contentItems"][0]["text"].as_str().unwrap()).unwrap()
}

fn connect_worker(h: &mut Harness, request: &Value, thread: &str) -> Value {
    h.receive(json!({"id":request["id"],"result":{"thread":{"id":thread},"model":"gpt-6-luna"}}));
    h.tick();
    let start = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":start["id"],"result":{"turn":{"id":format!("{thread}-turn")}}}));
    start
}

fn finish(h: &mut Harness, thread: &str, status: &str) {
    h.receive(json!({"method":"item/completed","params":{"threadId":thread,"turnId":format!("{thread}-turn"),
        "item":{"id":"answer","type":"agentMessage","text":"Checked parser.rs:12; no issue."}}}));
    h.receive(
        json!({"method":"turn/completed","params":{"threadId":thread,
        "turn":{"id":format!("{thread}-turn"),"status":status}}}),
    );
    h.tick();
}

#[test]
fn concurrent_threads_route_remapped_responses_and_wait_for_worker_start() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let a = h
        .spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let b = h
        .spawn_manual(
            "Inspect rendering edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let requests = h.drain_requests();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0]["id"], requests[1]["id"]);
    assert!(requests.iter().all(|r| r["method"] == "thread/start"));
    assert!(
        requests
            .iter()
            .all(|r| r["params"].get("dynamicTools").is_none())
    );
    let turn_b = connect_worker(&mut h, &requests[1], "worker-b");
    let turn_a = connect_worker(&mut h, &requests[0], "worker-a");
    assert_eq!(turn_b["params"]["threadId"], "worker-b");
    assert_eq!(turn_a["params"]["threadId"], "worker-a");
    assert_eq!(h.agents[a].session.thread_id.as_deref(), Some("worker-a"));
    assert_eq!(h.agents[b].session.thread_id.as_deref(), Some("worker-b"));
    assert_eq!(turn_a["params"]["effort"], "low");
    assert!(
        !turn_a["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("delegate independent checks")
    );
    h.receive(json!({"method":"item/agentMessage/delta","params":{"threadId":"worker-a","itemId":"one","delta":"Only A"}}));
    assert!(h.agents[a].session.transcript().contains("Only A"));
    assert!(!h.agents[b].session.transcript().contains("Only A"));
}

#[test]
fn automatic_limit_does_not_reset_when_a_worker_finishes_or_lead_is_steered() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let first = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    let worker = first
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    connect_worker(&mut h, worker, "worker-1");
    finish(&mut h, "worker-1", "completed");
    assert!(h.submit(0, "Also inspect escaping"));
    h.drain_requests();
    assert!(
        tool(&mut h, "spawn-2", "spawn", spawn_args(json!([])))
            .iter()
            .any(|r| r["method"] == "thread/start")
    );
    let third = tool(&mut h, "spawn-3", "spawn", spawn_args(json!([])));
    assert!(
        reply_output(&third, "spawn-3")["error"]
            .as_str()
            .unwrap()
            .contains("2")
    );
    assert_eq!(h.auto_launches(), 2);
}

#[test]
fn manual_and_restarted_workers_obey_the_shared_concurrency_cap() {
    let mut h = ready(Some(10), false);
    for _ in 0..3 {
        h.spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    }
    assert!(
        h.spawn_manual("One too many".into(), WorkerMode::Read, vec![])
            .is_err()
    );
    assert_eq!(h.auto_launches(), 0);
    assert_eq!(h.active_workers(), 3);
}

#[test]
fn duplicate_dynamic_calls_launch_once_even_with_a_new_rpc_id() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let first = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    assert_eq!(reply_output(&first, "spawn-1")["id"], "luna-1");
    h.receive(json!({"id":"replayed-rpc","method":"item/tool/call","params":{
        "threadId":"lead-thread","turnId":"lead-turn","callId":"spawn-1","namespace":"melly_agents","tool":"spawn","arguments":spawn_args(json!([]))
    }}));
    let replayed = h.drain_requests();
    assert_eq!(reply_output(&replayed, "replayed-rpc")["id"], "luna-1");
    assert!(!replayed.iter().any(|r| r["method"] == "thread/start"));
    assert_eq!(h.agents.len(), 2);
}

#[test]
fn quota_reserve_and_unavailable_data_block_only_automatic_launches() {
    for quota in [None, Some(85), Some(100)] {
        let mut h = ready(quota, false);
        start_lead(&mut h);
        let mut responses = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
        if quota.is_none() {
            let refresh = responses
                .iter()
                .find(|r| r["method"] == "account/rateLimits/read")
                .unwrap();
            h.receive(json!({"id":refresh["id"],"result":{"rateLimits":null}}));
            responses = h.drain_requests();
        }
        assert!(reply_output(&responses, "spawn-1")["error"].is_string());
        assert_eq!(h.auto_launches(), 0);
        assert!(
            h.spawn_manual(
                "Inspect parsing edge cases".into(),
                WorkerMode::Read,
                vec![]
            )
            .is_ok()
        );
    }
}

#[test]
fn resumed_spawn_waits_for_fresh_quota_without_an_extra_model_turn() {
    let mut h = ready(Some(5), true);
    for timestamp in h.agents[0].session.quota_updated_at.values_mut() {
        *timestamp = std::time::Instant::now() - std::time::Duration::from_secs(600);
    }
    start_lead(&mut h);
    let responses = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    assert!(responses.iter().all(|r| r["id"] != "spawn-1"));
    let refresh = responses
        .iter()
        .find(|r| r["method"] == "account/rateLimits/read")
        .unwrap();
    h.receive(json!({"id":refresh["id"],"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":10}}}}));
    let requests = h.drain_requests();
    assert_eq!(reply_output(&requests, "spawn-1")["id"], "luna-1");
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "thread/start")
            .count(),
        1
    );
    assert!(!requests.iter().any(|r| r["method"] == "turn/start"));
}

#[test]
fn editable_ownership_is_relative_nonempty_and_disjoint() {
    let mut h = ready(Some(10), false);
    for paths in [
        vec![],
        vec!["../escape"],
        vec!["/absolute"],
        vec!["src/../escape"],
    ] {
        assert!(
            h.spawn_manual(
                "Implement parser checks".into(),
                WorkerMode::Edit,
                paths.into_iter().map(str::to_owned).collect()
            )
            .is_err()
        );
    }
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/parser".into()],
    )
    .unwrap();
    assert!(
        h.spawn_manual(
            "Implement parser checks".into(),
            WorkerMode::Edit,
            vec!["src/parser/mod.rs".into()]
        )
        .is_err()
    );
    assert!(
        h.spawn_manual(
            "Implement renderer checks".into(),
            WorkerMode::Edit,
            vec!["src/render.rs".into()]
        )
        .is_ok()
    );
    let starts = h.drain_requests();
    assert!(
        starts
            .iter()
            .all(|r| r["params"].get("approvalPolicy").is_none())
    );
    assert!(starts.iter().all(|r| r["params"].get("sandbox").is_none()));
}

#[test]
fn read_only_approvals_are_declined_and_edit_approvals_route_to_the_worker() {
    let mut h = ready(Some(10), false);
    h.spawn_manual(
        "Inspect parsing edge cases".into(),
        WorkerMode::Read,
        vec![],
    )
    .unwrap();
    h.spawn_manual(
        "Implement renderer checks".into(),
        WorkerMode::Edit,
        vec!["src/render.rs".into()],
    )
    .unwrap();
    let starts = h.drain_requests();
    connect_worker(&mut h, &starts[0], "reader");
    connect_worker(&mut h, &starts[1], "editor");
    for (thread, id) in [("reader", "deny"), ("editor", "ask")] {
        h.receive(json!({"id":id,"method":"item/commandExecution/requestApproval","params":{"threadId":thread,"command":"cargo test"}}));
    }
    assert!(h.agents[0].session.approvals.is_empty());
    assert!(h.agents[1].session.approvals.is_empty());
    assert_eq!(h.agents[2].session.approvals[0].id, "ask");
    assert!(
        h.drain_requests()
            .iter()
            .any(|r| r["id"] == "deny" && r["result"]["decision"] == "decline")
    );
}

#[test]
fn waits_are_nonblocking_and_resolve_with_bounded_results_without_a_hidden_turn() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let spawn = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        spawn
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker-1",
    );
    let waiting = tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    assert!(!waiting.iter().any(|r| r["id"] == "wait-1"));
    finish(&mut h, "worker-1", "completed");
    let results = h.drain_requests();
    let output = reply_output(&results, "wait-1");
    assert_eq!(output["agents"][0]["status"], "completed");
    assert!(
        output["agents"][0]["result"]
            .as_str()
            .unwrap()
            .contains("parser.rs:12")
    );
    assert!(!results.iter().any(|r| r["method"] == "turn/start"));
}

#[test]
fn zero_timeout_returns_status_and_interrupted_lead_cancels_only_automatic_workers() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let auto = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        auto.iter().find(|r| r["method"] == "thread/start").unwrap(),
        "automatic",
    );
    h.spawn_manual("Inspect output edge cases".into(), WorkerMode::Read, vec![])
        .unwrap();
    let manual = h.drain_requests().remove(0);
    connect_worker(&mut h, &manual, "manual");
    let wait = tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":0}),
    );
    assert_eq!(reply_output(&wait, "wait-1")["timed_out"], true);
    h.receive(json!({"method":"turn/completed","params":{"threadId":"lead-thread","turn":{"id":"lead-turn","status":"interrupted"}}}));
    let requests = h.drain_requests();
    assert!(
        requests
            .iter()
            .any(|r| r["method"] == "turn/interrupt" && r["params"]["threadId"] == "automatic")
    );
    assert!(
        !requests
            .iter()
            .any(|r| r["method"] == "turn/interrupt" && r["params"]["threadId"] == "manual")
    );
}

#[test]
fn workers_and_stale_lead_turns_cannot_launch_children() {
    let mut h = ready(Some(10), false);
    let requests = tool(&mut h, "before-turn", "spawn", spawn_args(json!([])));
    assert!(reply_output(&requests, "before-turn")["error"].is_string());
    start_lead(&mut h);
    h.receive(json!({"id":"bad","method":"item/tool/call","params":{
        "threadId":"worker","turnId":"lead-turn","callId":"bad","namespace":"melly_agents","tool":"spawn","arguments":spawn_args(json!([]))
    }}));
    assert!(reply_output(&h.drain_requests(), "bad")["error"].is_string());
    assert_eq!(h.agents.len(), 1);
}

#[test]
fn resumed_lead_can_dispatch_its_persisted_delegation_tools() {
    let mut h = ready(Some(10), true);
    start_lead(&mut h);
    let requests = tool(&mut h, "restored-spawn", "spawn", spawn_args(json!([])));
    assert_eq!(reply_output(&requests, "restored-spawn")["id"], "luna-1");
    assert_eq!(h.auto_launches(), 1);
}

#[test]
fn startup_is_busy_and_blank_lead_prompt_never_sends_a_collected_report_alone() {
    let h = Harness::new("/tmp/project".into(), None, None);
    assert!(h.busy());
    let mut h = ready(Some(10), false);
    h.spawn_manual(
        "Inspect parsing edge cases".into(),
        WorkerMode::Read,
        vec![],
    )
    .unwrap();
    let start = h.drain_requests().remove(0);
    connect_worker(&mut h, &start, "worker-1");
    finish(&mut h, "worker-1", "completed");
    h.drain_requests();
    assert!(!h.submit(0, "  "));
    assert!(h.drain_requests().is_empty());
    assert!(h.collect_completed().contains("parser.rs:12"));
}

#[test]
fn cancelling_lead_blocks_late_automatic_launches_until_the_next_user_turn() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    assert!(h.cancel(0));
    h.drain_requests();
    let requests = tool(&mut h, "late-spawn", "spawn", spawn_args(json!([])));
    assert!(reply_output(&requests, "late-spawn")["error"].is_string());
    assert_eq!(h.agents.len(), 1);
}

#[test]
fn restarted_workers_do_not_report_the_previous_turn_as_a_new_failed_result() {
    let mut h = ready(Some(10), false);
    let worker = h
        .spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let start = h.drain_requests().remove(0);
    connect_worker(&mut h, &start, "worker-1");
    finish(&mut h, "worker-1", "completed");
    h.collect_completed();
    assert!(h.submit(worker, "Now inspect output formatting"));
    let start = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":start["id"],"error":{"message":"Out of quota"}}));
    assert!(h.agents[worker].finished);
    assert!(
        !h.agents[worker]
            .result
            .as_deref()
            .unwrap()
            .contains("parser.rs:12")
    );
}

#[test]
fn failed_lead_start_cancels_automatic_workers_from_the_previous_turn() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker-1",
    );
    h.receive(json!({"method":"turn/completed","params":{"threadId":"lead-thread","turn":{"id":"lead-turn","status":"completed"}}}));
    assert!(h.submit(0, "Continue with another step"));
    let start = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":start["id"],"error":{"message":"Cannot start"}}));
    let requests = h.drain_requests();
    assert!(
        requests
            .iter()
            .any(|r| r["method"] == "turn/interrupt" && r["params"]["threadId"] == "worker-1")
    );
}

#[test]
fn resolved_wait_requests_are_forgotten_even_without_a_thread_id() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker-1",
    );
    tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    assert!(h.receive(json!({"method":"serverRequest/resolved","params":{"requestId":"wait-1"}})));
    finish(&mut h, "worker-1", "completed");
    assert!(!h.drain_requests().iter().any(|r| r["id"] == "wait-1"));
}

#[test]
fn truncated_results_are_collected_once_and_never_submitted_without_user_input() {
    let mut h = ready(Some(10), false);
    h.spawn_manual(
        "Inspect parsing edge cases".into(),
        WorkerMode::Read,
        vec![],
    )
    .unwrap();
    let start = h.drain_requests().remove(0);
    connect_worker(&mut h, &start, "worker-1");
    h.receive(
        json!({"method":"item/completed","params":{"threadId":"worker-1","item":{
        "type":"agentMessage","id":"answer","text":"界".repeat(8000)}}}),
    );
    h.receive(json!({"method":"turn/completed","params":{"threadId":"worker-1","turn":{"id":"worker-1-turn","status":"completed"}}}));
    assert!(h.agents[1].result.as_deref().unwrap().chars().count() <= 6000);
    assert!(h.drain_requests().is_empty());
    assert!(!h.collect_completed().is_empty());
    assert!(h.collect_completed().is_empty());
}

#[test]
fn completed_worker_restart_rechecks_slots_and_edit_ownership() {
    let mut h = ready(Some(10), false);
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/parser".into()],
    )
    .unwrap();
    let start = h.drain_requests().remove(0);
    connect_worker(&mut h, &start, "editor");
    finish(&mut h, "editor", "completed");
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/parser/new.rs".into()],
    )
    .unwrap();
    assert!(!h.submit(1, "Continue editing the parser"));
    h.spawn_manual("Inspect output formatting".into(), WorkerMode::Read, vec![])
        .unwrap();
    h.spawn_manual("Inspect command usage".into(), WorkerMode::Read, vec![])
        .unwrap();
    assert_eq!(h.active_workers(), 3);
    assert!(!h.submit(1, "Continue editing the parser"));
}

#[test]
fn bounds_reject_invalid_briefs_before_spending_a_launch() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    for (index, mut arguments) in [
        spawn_args(json!([])),
        spawn_args(json!([])),
        spawn_args(json!([])),
        spawn_args(json!([])),
    ]
    .into_iter()
    .enumerate()
    {
        match index {
            0 => arguments["objective"] = json!("x".repeat(2001)),
            1 => arguments["context"] = json!("x".repeat(6001)),
            2 => arguments["expected_output"] = json!("x".repeat(1001)),
            _ => arguments["reason"] = json!(""),
        }
        let id = format!("invalid-{index}");
        let requests = tool(&mut h, &id, "spawn", arguments);
        assert!(reply_output(&requests, &id)["error"].is_string());
    }
    assert_eq!(h.auto_launches(), 0);
    assert_eq!(h.agents.len(), 1);
}

#[test]
fn directory_ownership_normalizes_a_trailing_slash() {
    let mut h = ready(Some(10), false);
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/parser/".into()],
    )
    .unwrap();
    assert_eq!(h.agents[1].owned_paths, ["src/parser"]);
    assert!(
        h.spawn_manual(
            "Implement parser checks".into(),
            WorkerMode::Edit,
            vec!["src/parser/mod.rs".into()]
        )
        .is_err()
    );
}

#[test]
fn cancelling_an_opening_worker_never_submits_its_prompt_after_connection() {
    let mut h = ready(Some(10), false);
    let worker = h
        .spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let start = h.drain_requests().remove(0);
    assert!(h.cancel(worker));
    h.receive(json!({"id":start["id"],"result":{"thread":{"id":"cancelled-worker"},"model":"gpt-6-luna"}}));
    h.tick();
    assert!(h.agents[worker].finished);
    assert_eq!(h.agents[worker].session.status, "cancelled");
    assert!(h.drain_requests().is_empty());
}

#[test]
fn disconnect_drops_pending_waits_and_never_restarts_workers() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "spawn-1", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker-1",
    );
    tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    h.disconnected("transport lost");
    assert!(!h.busy());
    assert!(!h.tick());
    assert!(h.drain_requests().is_empty());
    assert!(!h.submit(1, "Try again"));
}

#[test]
fn usage_and_skill_refreshes_from_a_worker_update_shared_account_state() {
    let mut h = ready(Some(10), false);
    let worker = h
        .spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let start = h.drain_requests().remove(0);
    connect_worker(&mut h, &start, "worker-1");
    h.agents[worker].session.refresh_quota();
    h.agents[worker].session.load_skills();
    let requests = h.drain_requests();
    for request in requests {
        if request["method"] == "account/rateLimits/read" {
            h.receive(json!({"id":request["id"],"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":90}}}}));
        }
        if request["method"] == "skills/list" {
            h.receive(json!({"id":request["id"],"result":{"data":[{"skills":[{"enabled":true,"name":"review","path":"/tmp/review/SKILL.md"}]}]}}));
        }
    }
    assert_eq!(h.agents[0].session.quota[0].used_percent, 90);
    assert_eq!(h.agents[worker].session.quota[0].used_percent, 90);
    assert_eq!(h.agents[0].session.skills[0].name, "review");
    assert_eq!(h.agents[worker].session.skills[0].name, "review");
    assert!(
        h.automatic_block_reason
            .as_deref()
            .unwrap()
            .contains("reserve")
    );
}

struct OwnershipWorkspace(std::path::PathBuf);
impl OwnershipWorkspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "melly-ownership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(path.join("project/src")).unwrap();
        std::fs::create_dir_all(path.join("outside")).unwrap();
        Self(path)
    }
    fn harness(&self) -> Harness {
        let mut h = ready(Some(10), false);
        h.agents[0].session.cwd = self.0.join("project").to_string_lossy().into_owned();
        h
    }
}
impl Drop for OwnershipWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[test]
fn ownership_resolves_symlink_aliases_of_existing_ancestors() {
    let workspace = OwnershipWorkspace::new();
    std::os::unix::fs::symlink("src", workspace.0.join("project/link")).unwrap();
    let mut h = workspace.harness();
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/new_module".into()],
    )
    .unwrap();
    assert!(
        h.spawn_manual(
            "Implement parser checks".into(),
            WorkerMode::Edit,
            vec!["link/new_module/mod.rs".into()]
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn ownership_rejects_symlink_aliases_that_escape_the_workspace() {
    let workspace = OwnershipWorkspace::new();
    std::os::unix::fs::symlink("../outside", workspace.0.join("project/link")).unwrap();
    let mut h = workspace.harness();
    assert!(
        h.spawn_manual(
            "Implement parser checks".into(),
            WorkerMode::Edit,
            vec!["link/new_module.rs".into()]
        )
        .is_err()
    );
    assert!(
        h.spawn_manual(
            "Inspect outside files".into(),
            WorkerMode::Read,
            vec!["link/new_module.rs".into()]
        )
        .is_err()
    );
    assert_eq!(h.agents.len(), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_ownership_compares_uncreated_paths_case_insensitively() {
    let workspace = OwnershipWorkspace::new();
    let mut h = workspace.harness();
    h.spawn_manual(
        "Implement parser checks".into(),
        WorkerMode::Edit,
        vec!["src/new_module".into()],
    )
    .unwrap();
    assert!(
        h.spawn_manual(
            "Implement parser checks".into(),
            WorkerMode::Edit,
            vec!["SRC/NEW_MODULE/mod.rs".into()]
        )
        .is_err()
    );
}

fn start_manual(h: &mut Harness, thread: &str) -> usize {
    let index = h
        .spawn_manual(
            "Inspect parsing edge cases".into(),
            WorkerMode::Read,
            vec![],
        )
        .unwrap();
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    connect_worker(h, &request, thread);
    index
}

fn finish_lead(h: &mut Harness) {
    h.receive(json!({"method":"turn/completed","params":{"threadId":"lead-thread","turn":{"id":"lead-turn","status":"completed"}}}));
}

#[test]
fn dynamic_status_collects_only_the_finished_reports_it_delivered() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    start_manual(&mut h, "one");
    start_manual(&mut h, "two");
    finish(&mut h, "one", "completed");
    finish(&mut h, "two", "completed");
    let responses = tool(&mut h, "status-1", "status", json!({"ids":["luna-1"]}));
    assert_eq!(
        reply_output(&responses, "status-1")["agents"][0]["finished"],
        true
    );
    finish_lead(&mut h);
    assert!(h.submit(0, "Continue reviewing"));
    let turn = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    let prompt = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert!(!prompt.contains("luna-1 ·"));
    assert!(prompt.contains("luna-2 ·"));
}

#[test]
fn completed_wait_reports_are_not_reinjected_after_delivery() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    start_manual(&mut h, "one");
    tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    finish(&mut h, "one", "completed");
    let replies = h.drain_requests();
    assert_eq!(
        reply_output(&replies, "wait-1")["agents"][0]["finished"],
        true
    );
    assert!(h.collect_completed().is_empty());
}

#[test]
fn a_resolved_undelivered_wait_keeps_its_worker_report_uncollected() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    start_manual(&mut h, "one");
    tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    finish(&mut h, "one", "completed");
    h.receive(json!({"method":"serverRequest/resolved","params":{"requestId":"wait-1"}}));
    assert!(!h.drain_requests().iter().any(|r| r["id"] == "wait-1"));
    assert!(h.collect_completed().contains("luna-1 ·"));
}

#[test]
fn cancelling_parent_before_wait_reply_is_drained_preserves_the_report() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    start_manual(&mut h, "one");
    tool(
        &mut h,
        "wait-1",
        "wait",
        json!({"ids":["luna-1"],"timeout_ms":120000}),
    );
    finish(&mut h, "one", "completed");
    h.cancel(0);
    let replies = h.drain_requests();
    assert!(reply_output(&replies, "wait-1")["error"].is_string());
    assert!(h.collect_completed().contains("luna-1 ·"));
}

#[test]
fn replaying_a_previous_report_does_not_collect_a_later_worker_turn() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let worker = start_manual(&mut h, "one");
    finish(&mut h, "one", "completed");
    tool(&mut h, "status-1", "status", json!({"ids":["luna-1"]}));
    h.collect_completed();
    assert!(h.submit(worker, "Inspect a different file now"));
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":request["id"],"result":{"turn":{"id":"another-turn"}}}));
    h.receive(json!({"method":"item/completed","params":{"threadId":"one","item":{"id":"answer-2","type":"agentMessage","text":"New result for another file."}}}));
    h.receive(json!({"method":"turn/completed","params":{"threadId":"one","turn":{"id":"another-turn","status":"completed"}}}));
    h.receive(json!({"id":"replay-rpc","method":"item/tool/call","params":{"namespace":"melly_agents","tool":"status","threadId":"lead-thread","turnId":"lead-turn","callId":"status-1","arguments":{"ids":["luna-1"]}}}));
    let replies = h.drain_requests();
    assert!(
        reply_output(&replies, "replay-rpc")["agents"][0]["result"]
            .as_str()
            .unwrap()
            .contains("parser.rs:12")
    );
    assert!(
        h.collect_completed()
            .contains("New result for another file.")
    );
}

#[test]
fn a_rejected_worker_interrupt_keeps_the_slot_but_allows_user_steer_and_retry() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "one");
    assert!(h.cancel(worker));
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/interrupt")
        .unwrap();
    h.receive(json!({"id":request["id"],"error":{"message":"Temporarily unavailable"}}));
    assert_eq!(h.active_workers(), 1);
    assert!(h.submit(worker, "Stop after the current check"));
    assert!(h.cancel(worker));
    let requests = h.drain_requests();
    assert!(requests.iter().any(|r| r["method"] == "turn/steer"));
    assert!(requests.iter().any(|r| r["method"] == "turn/interrupt"));
}

#[test]
fn rejected_worker_steering_does_not_hide_an_existing_final_answer() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "one");
    h.receive(json!({"method":"item/completed","params":{"threadId":"one","item":{"id":"answer","type":"agentMessage","text":"Valid answer before late steering."}}}));
    assert!(h.submit(worker, "Also check another file"));
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/steer")
        .unwrap();
    h.receive(json!({"id":request["id"],"error":{"message":"Turn already finishing"}}));
    h.receive(json!({"method":"turn/completed","params":{"threadId":"one","turn":{"id":"one-turn","status":"completed"}}}));
    assert!(
        h.agents[worker]
            .result
            .as_deref()
            .unwrap()
            .contains("Valid answer before late steering.")
    );
}

#[test]
fn worker_compaction_uses_a_slot_and_preserves_its_completed_report() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "one");
    finish(&mut h, "one", "completed");
    let report = h.agents[worker].result.clone();
    assert!(h.compact(worker));
    assert_eq!(h.active_workers(), 1);
    let request = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "thread/compact/start")
        .unwrap();
    assert_eq!(request["params"]["threadId"], "one");
    h.receive(json!({"id":request["id"],"result":{}}));
    assert_eq!(h.agents[worker].session.status, "Compacting");
    h.receive(
        json!({"method":"turn/started","params":{"threadId":"one","turn":{"id":"compact-turn"}}}),
    );
    h.receive(json!({"method":"turn/completed","params":{"threadId":"one","turn":{"id":"compact-turn","status":"completed"}}}));
    assert_eq!(h.active_workers(), 0);
    assert_eq!(h.agents[worker].result, report);
    assert!(h.agents[worker].finished);
}

#[test]
fn lead_compaction_tool_waits_until_the_current_turn_has_ended() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "compact", "compact", json!({"id":"lead"}));
    assert_eq!(
        reply_output(&requests, "compact")["status"],
        "scheduled after the lead turn"
    );
    assert!(
        !requests
            .iter()
            .any(|r| r["method"] == "thread/compact/start")
    );
    assert!(
        reply_output(
            &tool(&mut h, "twice", "compact", json!({"id":"lead"})),
            "twice"
        )["error"]
            .is_string()
    );
    h.receive(json!({"method":"turn/completed","params":{"threadId":"lead-thread","turn":{"id":"lead-turn","status":"completed"}}}));
    assert!(
        h.drain_requests()
            .iter()
            .any(|r| r["method"] == "thread/compact/start"
                && r["params"]["threadId"] == "lead-thread")
    );
}

#[test]
fn a_rejected_lead_start_preserves_worker_reports_for_retry() {
    let mut h = ready(Some(10), false);
    start_manual(&mut h, "one");
    finish(&mut h, "one", "completed");
    assert!(h.submit(0, "Review the worker findings"));
    let first = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    assert!(
        first["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("luna-1 ·")
    );
    h.receive(json!({"id":first["id"],"error":{"message":"Try again later"}}));
    assert!(h.submit(0, "Retry reviewing the findings"));
    let retry = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    assert!(
        retry["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("luna-1 ·")
    );
    h.receive(json!({"id":retry["id"],"result":{"turn":{"id":"accepted-lead-turn"}}}));
    assert!(h.collect_completed().is_empty());
}

#[test]
fn lead_acceptance_collects_only_the_exact_worker_turn_reports_it_received() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "one");
    finish(&mut h, "one", "completed");
    start_manual(&mut h, "two");
    assert!(h.submit(0, "Review the first report"));
    let lead = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    assert!(h.submit(worker, "Inspect another file"));
    let restart = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":restart["id"],"result":{"turn":{"id":"new-worker-turn"}}}));
    h.receive(json!({"method":"item/completed","params":{"threadId":"one","item":{"id":"new-report","type":"agentMessage","text":"A newer result after the lead submission."}}}));
    h.receive(json!({"method":"turn/completed","params":{"threadId":"one","turn":{"id":"new-worker-turn","status":"completed"}}}));
    finish(&mut h, "two", "completed");
    h.receive(json!({"id":lead["id"],"result":{"turn":{"id":"accepted-lead-turn"}}}));
    let reports = h.collect_completed();
    assert!(reports.contains("A newer result after the lead submission."));
    assert!(reports.contains("luna-2 ·"));
}
