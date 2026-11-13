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
    assert_eq!(open["params"]["sandbox"], "read-only");
    assert_eq!(open["params"]["approvalPolicy"], "never");
    let role = open["params"]["developerInstructions"].as_str().unwrap();
    assert!(role.contains("Melly's conductor"));
    assert!(role.contains("acceptance commands"));
    assert_eq!(
        open["params"]["config"]["skills"]["max_context_tokens"],
        3000
    );
    assert!(open["params"]["config"].get("memories").is_none());
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
            7
        );
        let spawn_schema = &open["params"]["dynamicTools"][0]["tools"][0]["inputSchema"];
        assert_eq!(
            spawn_schema["properties"]["expected_output"]["maxLength"],
            10_000
        );
        assert_eq!(spawn_schema["properties"]["objective"]["maxLength"], 2000);
        assert_eq!(spawn_schema["properties"]["context"]["maxLength"], 6000);
        assert_eq!(spawn_schema["properties"]["reason"]["maxLength"], 1000);
        assert!(
            spawn_schema["description"]
                .as_str()
                .unwrap()
                .contains("10000 Unicode characters")
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
    let role = &request["params"]["additionalContext"]["melly-conductor"];
    assert_eq!(role["kind"], "application");
    assert!(
        role["value"]
            .as_str()
            .unwrap()
            .contains("Melly's conductor")
    );
    assert_eq!(request["params"]["sandboxPolicy"]["type"], "readOnly");
    assert_eq!(request["params"]["approvalPolicy"], "never");
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
fn spawn_can_return_the_finished_report_without_another_conductor_call() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let mut args = spawn_args(json!([]));
    args["wait_for_result"] = json!(true);
    let requests = tool(&mut h, "assign", "spawn", args.clone());
    assert_eq!(h.lead_wait_status(), Some("Waiting for Luna"));
    assert!(!requests.iter().any(|r| r["id"] == "assign"));
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    h.receive(json!({"id":"retry","method":"item/tool/call","params":{
        "threadId":"lead-thread","turnId":"lead-turn","callId":"assign",
        "namespace":"melly_agents","tool":"spawn","arguments":args
    }}));
    assert!(h.drain_requests().is_empty());
    assert_eq!(h.auto_launches(), 1);
    connect_worker(&mut h, start, "worker");
    assert!(!h.agents[1].handoff.report_delivered);
    finish(&mut h, "worker", "completed");
    let replies = h.drain_requests();
    assert!(!replies.iter().any(|r| r["method"] == "turn/start"));
    for id in ["assign", "retry"] {
        let output = reply_output(&replies, id);
        assert_eq!(output["agents"][0]["id"], "luna-1");
        assert_eq!(output["agents"][0]["finished"], true);
        assert_eq!(
            output["agents"][0]["result"],
            "Checked parser.rs:12; no issue."
        );
        assert_eq!(output["timed_out"], false);
    }
    assert!(h.agents[1].handoff.report_delivered);
    assert!(h.agents[1].handoff.review.is_none());
    assert_eq!(h.lead_wait_status(), None);
}

#[test]
fn spawn_with_result_stays_pending_across_quota_refresh() {
    let mut h = ready(None, true);
    start_lead(&mut h);
    let mut args = spawn_args(json!([]));
    args["wait_for_result"] = json!(true);
    let requests = tool(&mut h, "assign", "spawn", args);
    let refresh = requests
        .iter()
        .find(|r| r["method"] == "account/rateLimits/read")
        .unwrap();
    h.receive(json!({"id":refresh["id"],"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":10}}}}));
    let requests = h.drain_requests();
    assert!(!requests.iter().any(|r| r["id"] == "assign"));
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    connect_worker(&mut h, start, "worker");
    finish(&mut h, "worker", "completed");
    assert_eq!(
        reply_output(&h.drain_requests(), "assign")["agents"][0]["finished"],
        true
    );
    assert_eq!(h.auto_launches(), 1);
}

#[test]
fn invalid_spawn_wait_option_cannot_launch_a_worker() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let mut args = spawn_args(json!([]));
    args["wait_for_result"] = json!("yes");
    let requests = tool(&mut h, "assign", "spawn", args);
    assert!(reply_output(&requests, "assign")["error"].is_string());
    assert_eq!(h.auto_launches(), 0);
    assert_eq!(h.agents.len(), 1);
}

#[test]
fn cancelling_conductor_resolves_a_spawn_wait_and_cancels_its_worker() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let mut args = spawn_args(json!([]));
    args["wait_for_result"] = json!(true);
    let requests = tool(&mut h, "assign", "spawn", args);
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    connect_worker(&mut h, start, "worker");
    h.cancel(0);
    let replies = h.drain_requests();
    assert!(reply_output(&replies, "assign")["error"].is_string());
    assert!(
        replies
            .iter()
            .any(|r| r["method"] == "turn/interrupt" && r["params"]["threadId"] == "worker")
    );
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
            2 => arguments["expected_output"] = json!("x".repeat(10_001)),
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
fn spawn_accepts_1016_expected_output_characters_without_truncating_checks() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let mut arguments = spawn_args(json!([]));
    arguments["objective"] = json!("o".repeat(43));
    arguments["context"] = json!("c".repeat(1364));
    arguments["expected_output"] = json!(format!("{}REQUIRED CHECK", "x".repeat(1002)));
    arguments["reason"] = json!("r".repeat(78));
    let expected = arguments["expected_output"].as_str().unwrap().to_owned();
    assert_eq!(expected.chars().count(), 1016);
    let requests = tool(&mut h, "long-checks", "spawn", arguments);
    assert_eq!(reply_output(&requests, "long-checks")["id"], "luna-1");
    assert_eq!(
        h.agents[1].handoff.brief.as_ref().unwrap().expected_output,
        expected
    );
    assert!(
        h.agents[1]
            .handoff
            .prompt
            .as_ref()
            .unwrap()
            .contains(&expected)
    );
}

#[test]
fn spawn_brief_aggregate_limit_counts_unicode_and_preserves_old_valid_maxima() {
    for fill in ["x", "🦀"] {
        for (sizes, valid) in [
            ([2000, 6000, 1000, 1000], true), // Every formerly valid field at its maximum.
            ([1, 0, 9998, 1], true),
            ([1, 0, 9999, 1], false),
        ] {
            let mut h = ready(Some(10), false);
            start_lead(&mut h);
            let mut arguments = spawn_args(json!([]));
            for (field, size) in ["objective", "context", "expected_output", "reason"]
                .into_iter()
                .zip(sizes)
            {
                arguments[field] = json!(fill.repeat(size));
            }
            let requests = tool(&mut h, "aggregate", "spawn", arguments);
            let output = reply_output(&requests, "aggregate");
            if valid {
                assert_eq!(output["id"], "luna-1", "{fill}: {output}");
                assert_eq!(h.auto_launches(), 1);
            } else {
                let error = output["error"].as_str().unwrap();
                assert!(
                    error.contains("10001") && error.contains("10000"),
                    "{error}"
                );
                assert!(
                    error.contains("repeated") && error.contains("required checks"),
                    "{error}"
                );
                assert_eq!(h.auto_launches(), 0);
                assert_eq!(h.agents.len(), 1);
            }
        }
    }
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
        static NEXT_WORKSPACE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "melly-ownership-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_WORKSPACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

#[test]
fn conductor_can_revise_the_same_finished_worker_once_within_budget() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "assign", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker",
    );
    finish(&mut h, "worker", "completed");
    let requests = tool(
        &mut h,
        "fix",
        "revise",
        json!({"id":"luna-1","feedback":"Check the empty input case."}),
    );
    assert_eq!(reply_output(&requests, "fix")["status"], "revising");
    let turn = requests
        .iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    assert_eq!(turn["params"]["threadId"], "worker");
    assert!(
        turn["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("empty input")
    );
    assert_eq!(h.auto_launches(), 2);
    assert_eq!(h.agents.len(), 2);
    assert!(h.agents[1].source == melly::delegation::AgentSource::Automatic);
}

#[test]
fn edit_worker_uses_its_own_permissions_across_spawn_revision_and_restore() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let conductor_environment = "Conductor environment is read-only, approval never.";
    let mut brief = spawn_args(json!(["parser.rs"]));
    brief["mode"] = json!("edit");
    brief["context"] = json!(conductor_environment);
    let requests = tool(&mut h, "assign", "spawn", brief);
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    let check_thread = |request: &Value| {
        let params = &request["params"];
        let instructions = params["developerInstructions"].as_str().unwrap();
        assert!(instructions.contains("Melly edit worker"));
        assert!(instructions.contains("server-provided"));
        assert!(!instructions.contains(conductor_environment));
        assert!(params.get("sandbox").is_none());
        assert!(params.get("approvalPolicy").is_none());
        assert_eq!(params["config"]["skills"]["max_context_tokens"], 1200);
        assert_eq!(params["config"]["memories"]["use_memories"], false);
    };
    let check_turn = |request: &Value, expected_input: &str| {
        let params = &request["params"];
        let role = &params["additionalContext"]["melly-worker"];
        assert_eq!(role["kind"], "application");
        assert!(role["value"].as_str().unwrap().contains("server-provided"));
        assert!(params.get("sandboxPolicy").is_none());
        assert!(params.get("approvalPolicy").is_none());
        assert!(
            params["input"][0]["text"]
                .as_str()
                .unwrap()
                .contains(expected_input)
        );
    };
    check_thread(start);
    let turn = connect_worker(&mut h, start, "worker");
    check_turn(&turn, conductor_environment);
    finish(&mut h, "worker", "completed");

    let revision = tool(
        &mut h,
        "fix",
        "revise",
        json!({"id":"luna-1","feedback":"Check empty input. Conductor environment is read-only, approval never."}),
    );
    check_turn(
        revision
            .iter()
            .find(|r| r["method"] == "turn/start")
            .unwrap(),
        conductor_environment,
    );

    let mut restored = ready(Some(10), true);
    restored.restore_team(h.snapshot().unwrap());
    let resume = restored
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "thread/resume")
        .unwrap();
    check_thread(&resume);
    restored.receive(
        json!({"id":resume["id"],"result":{"thread":{"id":"worker"},"model":"gpt-6-luna"}}),
    );
    assert!(restored.submit(1, "Check the parser output"));
    let turn = restored
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    check_turn(&turn, "Check the parser output");
}

fn observed_command(h: &mut Harness, thread: &str, turn: &str, command: &str, exit: i64) {
    h.receive(json!({"method":"item/completed","params":{
        "threadId":thread,"turnId":turn,
        "item":{"type":"commandExecution","id":command,"command":command,"exitCode":exit}
    }}));
}

#[test]
fn review_requires_delivery_and_records_observed_commands() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "assign", "spawn", spawn_args(json!([])));
    let worker_prompt = connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker",
    );
    observed_command(&mut h, "worker", "worker-turn", "inspect parser", 1);
    finish(&mut h, "worker", "completed");

    let review = json!({"id":"luna-1","verdict":"accepted","summary":"Compared the implementation and the report."});
    let rejected = tool(&mut h, "premature", "review", review.clone());
    assert_eq!(
        reply_output(&rejected, "premature")["error"],
        "Deliver the current finished worker report before recording review."
    );
    observed_command(&mut h, "lead-thread", "lead-turn", "unrelated command", 0);
    h.receive(json!({"method":"item/started","params":{
        "threadId":"lead-thread","turnId":"lead-turn",
        "item":{"type":"commandExecution","id":"started early","command":"started early"}
    }}));
    tool(&mut h, "report", "status", json!({"ids":["luna-1"]}));
    observed_command(&mut h, "lead-thread", "lead-turn", "started early", 0);
    observed_command(
        &mut h,
        "lead-thread",
        "lead-turn",
        "inspect working tree",
        0,
    );

    let metadata = |h: &Harness| {
        serde_json::to_value(h.snapshot().unwrap()).unwrap()["workers"][0]["handoff"].clone()
    };
    assert_eq!(
        metadata(&h)["prompt"],
        worker_prompt["params"]["input"][0]["text"]
    );
    assert_eq!(metadata(&h)["report_delivered"], true);
    assert!(metadata(&h)["review"].is_null());
    assert_eq!(
        metadata(&h)["commands"],
        json!([{"command":"inspect parser","exit_code":1}])
    );

    let accepted = tool(&mut h, "accept", "review", review);
    assert_eq!(reply_output(&accepted, "accept")["verdict"], "accepted");
    assert_eq!(
        metadata(&h)["review"]["commands"],
        json!([{"command":"inspect working tree","exit_code":0}])
    );
    observed_command(&mut h, "lead-thread", "lead-turn", "later command", 0);
    tool(
        &mut h,
        "accept",
        "review",
        json!({"id":"luna-1","verdict":"changes_requested","summary":"Duplicate"}),
    );
    assert_eq!(metadata(&h)["review"]["verdict"], "accepted");
    assert_eq!(
        metadata(&h)["review"]["commands"].as_array().unwrap().len(),
        1
    );

    let mut restored = ready(Some(10), true);
    restored.restore_team(
        serde_json::from_value(serde_json::to_value(h.snapshot().unwrap()).unwrap()).unwrap(),
    );
    assert_eq!(metadata(&h), metadata(&restored));

    let correction = tool(
        &mut h,
        "correct",
        "revise",
        json!({"id":"luna-1","feedback":"Check the failed command."}),
    );
    let turn = correction
        .iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    assert_eq!(
        metadata(&h)["corrections"][0],
        turn["params"]["input"][0]["text"]
    );
    assert!(metadata(&h)["review"].is_null());
    assert_eq!(metadata(&h)["report_delivered"], false);
    assert_eq!(metadata(&h)["commands"], json!([]));
}

#[test]
fn review_rejects_worker_and_stale_turn_calls() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let requests = tool(&mut h, "assign", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut h,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "worker",
    );
    finish(&mut h, "worker", "completed");
    tool(&mut h, "report", "status", json!({"ids":["luna-1"]}));
    for (thread, turn) in [("worker", "worker-turn"), ("lead-thread", "stale-turn")] {
        h.receive(json!({"id":thread,"method":"item/tool/call","params":{
            "threadId":thread,"turnId":turn,"callId":"bad","namespace":"melly_agents","tool":"review",
            "arguments":{"id":"luna-1","verdict":"accepted","summary":"Unsupported assertion."}
        }}));
        let reply = h.drain_requests();
        assert_eq!(
            reply_output(&reply, thread)["error"],
            "Only the active lead turn may call melly_agents tools."
        );
        assert!(h.agents[1].handoff.review.is_none());
    }
}

#[test]
fn old_manifest_defaults_to_unavailable_handoff() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "worker");
    finish(&mut h, "worker", "completed");
    h.collect_completed();
    let mut old = serde_json::to_value(h.snapshot().unwrap()).unwrap();
    old["workers"][0].as_object_mut().unwrap().remove("handoff");
    let mut restored = ready(Some(10), true);
    restored.restore_team(serde_json::from_value(old).unwrap());
    assert_eq!(restored.agents[worker].handoff, Default::default());
}

#[test]
fn restored_delivery_does_not_attach_commands_without_a_fresh_report_boundary() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    start_manual(&mut h, "worker");
    finish(&mut h, "worker", "completed");
    tool(&mut h, "report", "status", json!({"ids":["luna-1"]}));

    let mut restored = ready(Some(10), true);
    restored.restore_team(h.snapshot().unwrap());
    start_lead(&mut restored);
    observed_command(
        &mut restored,
        "lead-thread",
        "lead-turn",
        "command before refreshed report",
        0,
    );
    let review =
        json!({"id":"luna-1","verdict":"accepted","summary":"Reviewed the delivered report."});
    tool(&mut restored, "accept", "review", review.clone());
    assert!(
        restored.agents[1]
            .handoff
            .review
            .as_ref()
            .unwrap()
            .commands
            .is_empty()
    );

    tool(&mut restored, "report", "status", json!({"ids":["luna-1"]}));
    observed_command(
        &mut restored,
        "lead-thread",
        "lead-turn",
        "command after refreshed report",
        0,
    );
    tool(&mut restored, "accept-again", "review", review);
    let commands = &restored.agents[1].handoff.review.as_ref().unwrap().commands;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command, "command after refreshed report");
}

#[test]
fn command_evidence_survives_transcript_eviction_and_is_bounded() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let worker = start_manual(&mut h, "worker");
    for i in 0..20 {
        observed_command(&mut h, "worker", "worker-turn", &format!("check {i}"), i);
    }
    observed_command(&mut h, "worker", "worker-turn", "check 19", 19);
    h.agents[worker].session.entries.clear();
    finish(&mut h, "worker", "completed");
    let commands = &h.snapshot().unwrap().workers[0].handoff.commands;
    assert_eq!(commands.len(), 12);
    assert_eq!(commands[0].command, "check 19");
    assert_eq!(commands[11].exit_code, 8);
}

#[test]
fn correction_history_retains_latest_exact_prompts_with_omission_count() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "worker");
    for i in 0..34 {
        assert!(h.submit(worker, &format!("  Correction {i}\n")));
    }
    let saved = serde_json::to_value(h.snapshot().unwrap()).unwrap();
    let handoff = &saved["workers"][0]["handoff"];
    assert_eq!(handoff["corrections_omitted"], 2);
    let corrections = handoff["corrections"].as_array().unwrap();
    assert_eq!(corrections.len(), 32);
    assert_eq!(corrections[0], "  Correction 2\n");
    assert_eq!(corrections[31], "  Correction 33\n");
}

#[test]
fn a_delayed_command_from_a_previous_worker_turn_cannot_attach_to_a_revision() {
    let mut h = ready(Some(10), false);
    let worker = start_manual(&mut h, "worker");
    finish(&mut h, "worker", "completed");
    assert!(h.submit(worker, "Check another case."));
    let turn = h
        .drain_requests()
        .into_iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    h.receive(json!({"id":turn["id"],"result":{"turn":{"id":"worker-next"}}}));
    observed_command(&mut h, "worker", "worker-turn", "delayed old command", 2);
    observed_command(&mut h, "worker", "worker-next", "current command", 0);
    assert_eq!(h.agents[worker].handoff.commands.len(), 1);
    assert_eq!(
        h.agents[worker].handoff.commands[0].command,
        "current command"
    );
}

#[test]
fn observed_command_output_is_bounded_persisted_and_optional() {
    let mut h = ready(Some(10), false);
    start_lead(&mut h);
    let worker = start_manual(&mut h, "worker");
    observed_command(&mut h, "worker", "worker-turn", "check without output", 0);
    let output = format!("BEGIN OUTPUT\n{}\nTEST SUMMARY AT END", "λ".repeat(4500));
    h.receive(json!({"method":"item/completed","params":{
        "threadId":"worker","turnId":"worker-turn",
        "item":{"type":"commandExecution","id":"long-check","command":"check with output","exitCode":1,"aggregatedOutput":output}
    }}));
    finish(&mut h, "worker", "completed");
    let saved = serde_json::to_value(h.snapshot().unwrap()).unwrap();
    let commands = saved["workers"][0]["handoff"]["commands"]
        .as_array()
        .unwrap();
    let clipped = commands[0]["output"].as_str().expect("recorded output");
    assert_eq!(clipped.chars().count(), 4000);
    assert!(clipped.starts_with("BEGIN OUTPUT"));
    assert!(clipped.contains("middle omitted"));
    assert!(clipped.ends_with("TEST SUMMARY AT END"));
    assert!(commands[1].get("output").is_none());

    let mut restored = ready(Some(10), true);
    restored.restore_team(serde_json::from_value(saved).unwrap());
    assert_eq!(restored.agents[worker].handoff, h.agents[worker].handoff);

    let reports = tool(
        &mut h,
        "output-preview",
        "status",
        json!({"ids":["luna-1"]}),
    );
    let report = reply_output(&reports, "output-preview");
    let observed = &report["agents"][0]["observed_commands"];
    assert_eq!(
        observed[0]["output_preview"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        512
    );
    assert_eq!(observed[0]["output_preview_truncated"], true);
    assert!(
        observed[0]["output_preview"]
            .as_str()
            .unwrap()
            .ends_with("TEST SUMMARY AT END")
    );
    assert!(observed[1].get("output_preview").is_none());
}

const SOURCE_SNAPSHOT: &str = "\n\nSource snapshot (JSON)\n";

fn source_packet(prompt: &str) -> (&str, Value) {
    let (_, packet) = prompt
        .split_once(SOURCE_SNAPSHOT)
        .expect("the worker prompt should include a source snapshot");
    let parsed = serde_json::from_str(packet).expect("source snapshot must be valid JSON");
    (packet, parsed)
}

fn spawn_with_sources(h: &mut Harness, id: &str, owned: Value, context: Value) -> Value {
    let mut brief = spawn_args(owned);
    brief["mode"] = json!("edit");
    brief["context_paths"] = context;
    tool(h, id, "spawn", brief)
        .into_iter()
        .find(|r| r["method"] == "thread/start")
        .expect("worker thread should start")
}

#[test]
fn source_packet_reaches_first_worker_turn_and_persists_the_spawn_snapshot() {
    let workspace = OwnershipWorkspace::new();
    let project = workspace.0.join("project");
    let owned = project.join("src/owned.rs");
    std::fs::write(&owned, "previous source\n").unwrap();
    std::fs::write(project.join("support.rs"), "supporting source\n").unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    std::fs::write(&owned, "source at spawn\n").unwrap();
    let start = spawn_with_sources(
        &mut h,
        "assign",
        json!(["src/owned.rs"]),
        json!(["support.rs"]),
    );
    std::fs::write(&owned, "source after spawn\n").unwrap();
    let turn = connect_worker(&mut h, &start, "worker");
    let prompt = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert_eq!(prompt.matches(SOURCE_SNAPSHOT).count(), 1);
    assert_eq!(
        source_packet(prompt).1,
        json!({"files":[
            {"path":"src/owned.rs","content":"source at spawn\n"},
            {"path":"support.rs","content":"supporting source\n"}
        ],"omitted":[]})
    );
    assert_eq!(h.agents[1].handoff.prompt.as_deref(), Some(prompt));
    assert_eq!(h.agents[1].owned_paths, ["src/owned.rs"]);
    let saved = serde_json::to_string(&h.snapshot().unwrap()).unwrap();
    let mut resumed = ready(Some(10), true);
    resumed.restore_team(serde_json::from_str(&saved).unwrap());
    assert_eq!(resumed.agents[1].handoff.prompt.as_deref(), Some(prompt));

    // The supporting path is available for a different worker to own.
    let start = spawn_with_sources(
        &mut h,
        "edit-support",
        json!(["support.rs"]),
        json!(["src/owned.rs"]),
    );
    let turn = connect_worker(&mut h, &start, "second");
    let packet = source_packet(turn["params"]["input"][0]["text"].as_str().unwrap()).1;
    assert_eq!(packet["files"][1]["content"], "source after spawn\n");
    assert_eq!(h.agents[2].owned_paths, ["support.rs"]);
}

#[test]
fn source_packet_captures_owned_files_for_old_tool_calls_and_manual_workers() {
    let workspace = OwnershipWorkspace::new();
    std::fs::write(workspace.0.join("project/src/owned.rs"), "owned source").unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let mut brief = spawn_args(json!(["src/owned.rs"]));
    brief["mode"] = json!("edit");
    let requests = tool(&mut h, "assign", "spawn", brief);
    let start = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    let turn = connect_worker(&mut h, start, "worker");
    let prompt = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert_eq!(
        source_packet(prompt).1,
        json!({"files":[{"path":"src/owned.rs","content":"owned source"}],"omitted":[]})
    );
    let manual = h
        .spawn_manual(
            "Inspect owned source".into(),
            WorkerMode::Read,
            vec!["src/owned.rs".into()],
        )
        .unwrap();
    let manual_prompt = h.agents[manual].handoff.prompt.as_deref().unwrap();
    assert_eq!(source_packet(manual_prompt).0, source_packet(prompt).0);
}

#[test]
fn source_packet_rejects_malformed_context_paths_without_spending_a_launch() {
    for context_paths in [
        json!(null),
        json!("src"),
        json!([1]),
        json!(vec!["src"; 9]),
        json!([""]),
        json!(["/absolute"]),
        json!(["~/file"]),
        json!(["src/../file"]),
        json!(["src/./file"]),
        json!(["src//file"]),
        json!(["src/"]),
        json!(["src\\file"]),
        json!(["src/file:1"]),
        json!(["src/\nfile"]),
        json!([format!("a{}", "é".repeat(256))]),
    ] {
        let mut h = ready(Some(10), false);
        start_lead(&mut h);
        let mut brief = spawn_args(json!([]));
        brief["context_paths"] = context_paths.clone();
        let requests = tool(&mut h, "assign", "spawn", brief);
        let reply = reply_output(&requests, "assign");
        assert!(
            reply["error"]
                .as_str()
                .is_some_and(|error| error.contains("context_paths")),
            "accepted malformed context_paths {context_paths}: {reply}"
        );
        assert_eq!(h.auto_launches(), 0);
        assert_eq!(h.agents.len(), 1);
    }
}

#[test]
fn source_packet_reports_file_omissions_and_keeps_whole_utf8_at_the_file_limit() {
    let workspace = OwnershipWorkspace::new();
    let project = workspace.0.join("project");
    let whole = "界".repeat(4096);
    std::fs::write(project.join("src/whole"), &whole).unwrap();
    std::fs::write(project.join("large"), format!("{whole}x")).unwrap();
    std::fs::write(project.join("invalid-utf8"), [0xff]).unwrap();
    std::fs::write(project.join("nul"), b"text\0text").unwrap();
    std::fs::write(project.join("src/not-requested"), "directory child").unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(
        &mut h,
        "assign",
        json!(["src", "absent"]),
        json!(["src/whole", "large", "invalid-utf8", "nul", "absent"]),
    );
    let turn = connect_worker(&mut h, &start, "worker");
    let packet = source_packet(turn["params"]["input"][0]["text"].as_str().unwrap()).1;
    assert_eq!(
        packet["files"],
        json!([{"path":"src/whole","content":whole}])
    );
    let omitted = packet["omitted"].as_array().unwrap();
    assert_eq!(
        omitted.iter().map(|item| &item["path"]).collect::<Vec<_>>(),
        ["src", "absent", "large", "invalid-utf8", "nul"]
    );
    for (item, reason) in omitted.iter().zip([
        "directory",
        "missing",
        "too large",
        "invalid UTF-8",
        "contains NUL",
    ]) {
        assert_eq!(item["reason"], reason);
    }
    assert_eq!(h.agents[1].owned_paths, ["src", "absent"]);
}

#[cfg(unix)]
#[test]
fn source_packet_deduplicates_canonical_aliases_and_omits_non_regular_and_outside_paths() {
    let workspace = OwnershipWorkspace::new();
    let project = workspace.0.join("project");
    std::fs::write(project.join("src/owned"), "only once").unwrap();
    std::fs::write(workspace.0.join("outside/private"), "outside data").unwrap();
    std::os::unix::fs::symlink("src/owned", project.join("alias")).unwrap();
    std::os::unix::fs::symlink("../outside", project.join("escape")).unwrap();
    let fifo = std::process::Command::new("mkfifo")
        .arg(project.join("fifo"))
        .status()
        .unwrap();
    assert!(fifo.success());
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(
        &mut h,
        "assign",
        json!(["src/owned"]),
        json!(["alias", "src/owned", "escape/private", "fifo"]),
    );
    let turn = connect_worker(&mut h, &start, "worker");
    assert_eq!(
        source_packet(turn["params"]["input"][0]["text"].as_str().unwrap()).1,
        json!({"files":[{"path":"src/owned","content":"only once"}],
            "omitted":[{"path":"escape/private","reason":"outside workspace"},
                {"path":"fifo","reason":"not a regular file"}]})
    );
}

#[cfg(unix)]
#[test]
fn source_packet_omits_unreadable_supporting_files() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = OwnershipWorkspace::new();
    let unreadable = workspace.0.join("project/unreadable");
    std::fs::write(&unreadable, "private source").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&unreadable).is_ok() {
        return; // A privileged test runner can read files with mode 000.
    }
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["absent"]), json!(["unreadable"]));
    let turn = connect_worker(&mut h, &start, "worker");
    let packet = source_packet(turn["params"]["input"][0]["text"].as_str().unwrap()).1;
    assert_eq!(packet["files"], json!([]));
    assert_eq!(
        packet["omitted"][1],
        json!({"path":"unreadable","reason":"unreadable"})
    );
}

#[test]
fn source_packet_counts_json_escapes_and_omissions_at_the_packet_limit() {
    const LIMIT: usize = 32 * 1024;
    let workspace = OwnershipWorkspace::new();
    let project = workspace.0.join("project");
    let escaped = "\"".repeat(12 * 1024);
    let expected = |filler: &str| {
        json!({"files":[
            {"path":"escape-heavy","content":escaped},
            {"path":"boundary","content":filler}
        ],"omitted":[{"path":"budget-excluded","reason":"packet budget exceeded"}]})
    };
    let filler = "x".repeat(LIMIT - expected("").to_string().len());
    std::fs::write(project.join("escape-heavy"), &escaped).unwrap();
    std::fs::write(project.join("boundary"), &filler).unwrap();
    std::fs::write(project.join("budget-excluded"), "z".repeat(12 * 1024)).unwrap();

    let mut previous = None;
    for _ in 0..2 {
        let mut h = workspace.harness();
        start_lead(&mut h);
        let start = spawn_with_sources(
            &mut h,
            "assign",
            json!(["escape-heavy", "boundary"]),
            json!(["budget-excluded"]),
        );
        let turn = connect_worker(&mut h, &start, "worker");
        let (raw, packet) = source_packet(turn["params"]["input"][0]["text"].as_str().unwrap());
        assert_eq!(packet, expected(&filler));
        assert_eq!(raw.len(), LIMIT);
        if let Some(previous) = previous.replace(raw.to_owned()) {
            assert_eq!(raw, previous);
        }
    }
}

#[test]
fn source_packet_keeps_omissions_for_all_sixteen_requested_paths() {
    let workspace = OwnershipWorkspace::new();
    let project = workspace.0.join("project");
    let requested: Vec<_> = (0..16)
        .map(|i| {
            format!(
                "{:02}{}/{}/{}",
                i,
                "\"".repeat(168),
                "\"".repeat(170),
                "\"".repeat(170)
            )
        })
        .collect();
    assert!(requested.iter().all(|path| path.len() == 512));
    std::fs::create_dir_all(project.join(std::path::Path::new(&requested[0]).parent().unwrap()))
        .unwrap();
    std::fs::write(project.join(&requested[0]), "\"".repeat(12 * 1024)).unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(
        &mut h,
        "assign",
        json!(requested[..8]),
        json!(requested[8..]),
    );
    let turn = connect_worker(&mut h, &start, "worker");
    let (raw, packet) = source_packet(turn["params"]["input"][0]["text"].as_str().unwrap());
    assert!(raw.len() <= 32 * 1024);
    assert_eq!(packet["files"], json!([]));
    let omitted = packet["omitted"].as_array().unwrap();
    assert_eq!(omitted.len(), 16);
    assert_eq!(omitted[0]["reason"], "packet budget exceeded");
    for (item, requested) in omitted.iter().zip(requested) {
        assert_eq!(item["path"], requested);
    }
}

#[test]
fn source_packet_is_not_repeated_on_a_worker_revision() {
    let workspace = OwnershipWorkspace::new();
    let source = workspace.0.join("project/src/owned");
    std::fs::write(&source, "original source").unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src/owned"]), json!([]));
    let turn = connect_worker(&mut h, &start, "worker");
    let original = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert_eq!(
        source_packet(original).1["files"][0]["content"],
        "original source"
    );
    finish(&mut h, "worker", "completed");
    std::fs::write(&source, "updated source").unwrap();

    let requests = tool(
        &mut h,
        "revise",
        "revise",
        json!({"id":"luna-1","feedback":"Check the empty input."}),
    );
    let turn = requests
        .iter()
        .find(|r| r["method"] == "turn/start")
        .unwrap();
    let revision = turn["params"]["input"][0]["text"].as_str().unwrap();
    assert!(revision.contains("Check the empty input."));
    assert!(!revision.contains(SOURCE_SNAPSHOT));
    assert_eq!(h.agents[1].handoff.prompt.as_deref(), Some(original));
}

fn review_git(project: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .args(args)
        .current_dir(project)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn review_repository(workspace: &OwnershipWorkspace) -> std::path::PathBuf {
    let project = workspace.0.join("project");
    review_git(&project, &["init", "-q"]);
    review_git(&project, &["config", "user.name", "Melly test"]);
    review_git(&project, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(project.join("src/tracked"), "before\n").unwrap();
    std::fs::write(project.join("src/staged"), "before staged\n").unwrap();
    std::fs::write(project.join("src/deleted"), "delete me\n").unwrap();
    review_git(&project, &["add", "."]);
    review_git(&project, &["commit", "-qm", "baseline"]);
    project
}

fn review_packet(h: &mut Harness, call: &str) -> Value {
    let requests = tool(h, call, "status", json!({"ids":["luna-1"]}));
    reply_output(&requests, call)["agents"][0]["review_evidence"].clone()
}

#[test]
fn completion_review_captures_actual_owned_changes_and_survives_resume() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    std::fs::write(project.join("src/tracked"), "after unstaged\n").unwrap();
    std::fs::write(project.join("src/staged"), "after staged\n").unwrap();
    review_git(&project, &["add", "src/staged"]);
    std::fs::remove_file(project.join("src/deleted")).unwrap();
    std::fs::write(project.join("src/new"), "new contents\n").unwrap();
    std::fs::write(project.join("outside_scope"), "do not include\n").unwrap();
    assert!(
        tool(
            &mut h,
            "completion-wait",
            "wait",
            json!({"ids":["luna-1"],"timeout_ms":120000})
        )
        .is_empty()
    );
    finish(&mut h, "worker", "completed");
    let waited = h.drain_requests();
    let packet = review_packet(&mut h, "report");
    assert_eq!(
        reply_output(&waited, "completion-wait")["agents"][0]["review_evidence"],
        packet
    );
    assert_eq!(packet["complete"], true);
    let diff = packet["diff"].as_str().unwrap();
    assert!(
        diff.contains("+after unstaged")
            && diff.contains("+after staged")
            && diff.contains("-delete me")
    );
    assert!(!diff.contains("outside_scope"));
    assert_eq!(
        packet["untracked_files"],
        json!([{"path":"src/new","content":"new contents\n"}])
    );
    assert!(
        packet["source"]
            .as_str()
            .unwrap()
            .contains("not attribution")
    );
    assert!(
        packet["source"]
            .as_str()
            .unwrap()
            .contains("preexisting edits")
    );
    assert!(packet["source"].as_str().unwrap().contains("stale"));
    assert!(packet["head"].as_str().unwrap().len() >= 40);
    assert!(h.agents[1].handoff.review.is_none());

    // Repeated status/completion notices must not silently refresh the reviewed snapshot.
    std::fs::write(project.join("src/new"), "changed later\n").unwrap();
    finish(&mut h, "worker", "completed");
    assert_eq!(review_packet(&mut h, "again"), packet);
    let saved = serde_json::to_value(h.snapshot().unwrap()).unwrap();
    assert_eq!(saved["workers"][0]["handoff"]["review_evidence"], packet);
    let mut restored = ready(Some(10), true);
    restored.restore_team(serde_json::from_value(saved.clone()).unwrap());
    assert_eq!(
        serde_json::to_value(&restored.agents[1].handoff).unwrap()["review_evidence"],
        packet
    );
    let mut old = saved;
    old["workers"][0]["handoff"]
        .as_object_mut()
        .unwrap()
        .remove("review_evidence");
    let old: melly::delegation::SavedTeam = serde_json::from_value(old).unwrap();
    assert!(serde_json::to_value(&old.workers[0].handoff).unwrap()["review_evidence"].is_null());

    let revision = tool(
        &mut h,
        "revise",
        "revise",
        json!({"id":"luna-1","feedback":"Check the changed file."}),
    );
    let turn = revision
        .iter()
        .find(|request| request["method"] == "turn/start")
        .unwrap();
    assert!(review_packet(&mut h, "revising").is_null());
    h.receive(json!({"id":turn["id"],"result":{"turn":{"id":"worker-revised"}}}));
    h.receive(json!({"method":"turn/completed","params":{"threadId":"worker","turn":{"id":"worker-revised","status":"completed"}}}));
    h.tick();
    assert_eq!(
        review_packet(&mut h, "revised")["untracked_files"][0]["content"],
        "changed later\n"
    );
}

#[test]
fn completion_review_limits_large_binary_and_special_files() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    std::fs::write(project.join("src/large"), "x".repeat(40_000)).unwrap();
    std::fs::write(project.join("src/binary"), [0, 1, 2]).unwrap();
    std::fs::write(project.join("src/tracked"), [0, 1, 2]).unwrap();
    #[cfg(unix)]
    {
        std::fs::write(workspace.0.join("outside/secret"), "outside secret").unwrap();
        std::os::unix::fs::symlink(workspace.0.join("outside/secret"), project.join("src/link"))
            .unwrap();
        let status = std::process::Command::new("mkfifo")
            .arg(project.join("src/pipe"))
            .status()
            .unwrap();
        assert!(status.success());
    }
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    let encoded = serde_json::to_string(&packet).unwrap();
    assert_eq!(packet["complete"], false);
    assert!(encoded.len() <= 32 * 1024);
    assert!(!encoded.contains("outside secret"));
    let omissions = packet["omissions"].as_array().unwrap();
    assert!(omissions.iter().any(|entry| entry["path"] == "src/large"));
    assert!(omissions.iter().any(|entry| entry["path"] == "src/binary"));
    #[cfg(unix)]
    for path in ["src/link", "src/pipe"] {
        assert!(
            omissions.iter().any(|entry| entry["path"] == path),
            "{packet}"
        );
    }
}

#[test]
fn completion_review_bounds_serialized_escaped_content() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    for i in 0..12 {
        std::fs::write(
            project.join(format!("src/escaped-{i:02}")),
            "\u{0001}".repeat(7000),
        )
        .unwrap();
    }
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert_eq!(packet["complete"], false);
    assert!(serde_json::to_vec(&packet).unwrap().len() <= 32 * 1024);
    assert!(!packet["omissions"].as_array().unwrap().is_empty());
}

#[test]
fn completion_review_uses_literal_git_paths() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let name = "*";
    std::fs::write(project.join(name), "magic before\n").unwrap();
    review_git(&project, &["--literal-pathspecs", "add", "--", name]);
    review_git(&project, &["commit", "-qm", "literal"]);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!([name]), json!([]));
    connect_worker(&mut h, &start, "worker");
    std::fs::write(project.join(name), "magic after\n").unwrap();
    std::fs::write(project.join("src/tracked"), "unrelated marker\n").unwrap();
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert_eq!(packet["complete"], true);
    let diff = packet["diff"].as_str().unwrap();
    assert!(diff.contains("+magic after"));
    assert!(!diff.contains("unrelated marker"));
}

#[test]
fn completion_review_reports_unavailable_and_skips_read_workers() {
    let workspace = OwnershipWorkspace::new();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src/missing"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert_eq!(packet["complete"], false);
    assert!(packet["failure"].is_string());
    assert!(
        packet["source"]
            .as_str()
            .unwrap()
            .contains("inspect directly")
    );
    let mut read = workspace.harness();
    start_lead(&mut read);
    let requests = tool(&mut read, "assign", "spawn", spawn_args(json!([])));
    connect_worker(
        &mut read,
        requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap(),
        "reader",
    );
    finish(&mut read, "reader", "completed");
    assert!(review_packet(&mut read, "report").is_null());
}

#[test]
fn completion_review_disables_git_helpers_and_reports_missing_owned_paths() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    std::fs::write(
        project.join(".gitattributes"),
        "src/tracked diff=probe filter=probe\n",
    )
    .unwrap();
    review_git(&project, &["config", "diff.external", "false"]);
    review_git(&project, &["config", "diff.probe.textconv", "false"]);
    review_git(&project, &["config", "filter.probe.clean", "false"]);
    review_git(&project, &["config", "filter.probe.required", "true"]);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(
        &mut h,
        "assign",
        json!(["src/tracked", "missing"]),
        json!([]),
    );
    connect_worker(&mut h, &start, "worker");
    std::fs::write(project.join("src/tracked"), "safe plain diff\n").unwrap();
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert!(
        packet["diff"]
            .as_str()
            .unwrap()
            .contains("+safe plain diff"),
        "{packet}"
    );
    assert_eq!(packet["complete"], false);
    assert!(
        packet["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| omission["path"] == "missing")
    );
}

#[test]
fn completion_review_omits_an_oversized_diff_instead_of_claiming_completeness() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    std::fs::write(project.join("src/tracked"), "changed line\n".repeat(4000)).unwrap();
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert_eq!(packet["complete"], false);
    assert!(packet["diff"].is_null());
    assert!(
        packet["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|omission| omission["path"] == "(tracked diff)")
    );
    assert!(serde_json::to_vec(&packet).unwrap().len() <= 32 * 1024);
}

#[test]
fn completion_review_omits_ignored_contents_and_repository_metadata() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    std::fs::write(project.join(".gitignore"), "src/.env\nsrc/cache/\n").unwrap();
    std::fs::write(project.join("src/.env"), "private ignored marker").unwrap();
    std::fs::create_dir_all(project.join("src/cache")).unwrap();
    std::fs::write(project.join("src/cache/artifact"), "build ignored marker").unwrap();
    std::fs::create_dir_all(project.join("src/.git")).unwrap();
    std::fs::write(project.join("src/.git/config"), "administrative marker").unwrap();
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    let encoded = serde_json::to_string(&packet).unwrap();
    assert!(!encoded.contains("private ignored marker"));
    assert!(!encoded.contains("build ignored marker"));
    assert!(!encoded.contains("administrative marker"));
    assert_eq!(packet["complete"], false);
    assert!(
        packet["omissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == "src/.env")
    );
}

#[test]
fn completion_review_staged_deletions_include_head_only_paths_and_respect_cwd() {
    for nested_cwd in [false, true] {
        let workspace = OwnershipWorkspace::new();
        let project = review_repository(&workspace);
        let mut h = workspace.harness();
        if nested_cwd {
            h.agents[0].session.cwd = project.join("src").to_string_lossy().into_owned();
        }
        start_lead(&mut h);
        let owned = if nested_cwd {
            json!(["deleted"])
        } else {
            json!(["src"])
        };
        let start = spawn_with_sources(&mut h, "assign", owned, json!([]));
        connect_worker(&mut h, &start, "worker");
        review_git(&project, &["rm", "-q", "--", "src/deleted"]);
        std::fs::remove_file(project.join("src/tracked")).unwrap();
        finish(&mut h, "worker", "completed");
        let packet = review_packet(&mut h, "report");
        assert_eq!(packet["complete"], true, "{packet}");
        let diff = packet["diff"].as_str().unwrap();
        assert!(diff.contains("-delete me"), "{packet}");
        assert_eq!(diff.contains("a/src/tracked"), !nested_cwd, "{packet}");
        assert!(packet["omissions"].as_array().unwrap().is_empty());
    }
}

#[test]
fn completion_review_staged_deletion_keeps_the_untracked_worktree_copy() {
    let workspace = OwnershipWorkspace::new();
    let project = review_repository(&workspace);
    let mut h = workspace.harness();
    start_lead(&mut h);
    let start = spawn_with_sources(&mut h, "assign", json!(["src/tracked"]), json!([]));
    connect_worker(&mut h, &start, "worker");
    review_git(&project, &["rm", "-q", "--cached", "--", "src/tracked"]);
    std::fs::write(
        project.join("src/tracked"),
        "remaining untracked contents\n",
    )
    .unwrap();
    finish(&mut h, "worker", "completed");
    let packet = review_packet(&mut h, "report");
    assert_eq!(packet["complete"], true, "{packet}");
    assert!(
        packet["diff"].as_str().unwrap().contains("-before"),
        "{packet}"
    );
    assert_eq!(
        packet["untracked_files"],
        json!([{"path":"src/tracked","content":"remaining untracked contents\n"}])
    );
}

#[test]
fn completion_review_preserves_opposing_staged_and_unstaged_changes_with_one_budget() {
    for lines in [1, 7000] {
        let workspace = OwnershipWorkspace::new();
        let project = review_repository(&workspace);
        let mut h = workspace.harness();
        start_lead(&mut h);
        let start = spawn_with_sources(&mut h, "assign", json!(["src/tracked"]), json!([]));
        connect_worker(&mut h, &start, "worker");
        std::fs::write(project.join("src/tracked"), "A\n".repeat(lines)).unwrap();
        review_git(&project, &["add", "--", "src/tracked"]);
        std::fs::write(project.join("src/tracked"), "before\n").unwrap();
        finish(&mut h, "worker", "completed");
        let packet = review_packet(&mut h, "report");
        assert!(serde_json::to_vec(&packet).unwrap().len() <= 32 * 1024);
        if lines == 1 {
            assert_eq!(packet["complete"], true, "{packet}");
            let diff = packet["diff"].as_str().unwrap();
            let (staged, unstaged) = diff
                .split_once("Unstaged changes (index -> worktree)")
                .expect("both independent views");
            assert!(staged.contains("Staged changes (HEAD -> index)"));
            assert!(
                staged.contains("-before") && staged.contains("+A"),
                "{diff}"
            );
            assert!(
                unstaged.contains("-A") && unstaged.contains("+before"),
                "{diff}"
            );
        } else {
            assert_eq!(
                packet["complete"], false,
                "combined views must share the packet budget"
            );
            assert!(packet["diff"].is_null());
            assert!(
                packet["omissions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry["path"] == "(tracked diff)")
            );
        }
    }
}
