use melly::session::Session;
use serde_json::{Value, json};

fn take_request(session: &mut Session, method: &str) -> Value {
    session
        .drain_requests()
        .into_iter()
        .find(|request| request["method"] == method)
        .unwrap_or_else(|| panic!("expected outgoing {method} request"))
}

fn ready(resume: Option<&str>) -> Session {
    let mut session = Session::new("/tmp/project".into(), None, resume.map(str::to_owned));
    let initialize = take_request(&mut session, "initialize");
    assert_eq!(initialize["params"]["clientInfo"]["name"], "melly");
    session.receive(json!({"id": initialize["id"], "result": {}}));

    let requests = session.drain_requests();
    assert!(
        requests
            .iter()
            .any(|r| r["method"] == "initialized" && r.get("id").is_none())
    );
    let models = requests
        .iter()
        .find(|r| r["method"] == "model/list")
        .unwrap();
    let account = requests
        .iter()
        .find(|r| r["method"] == "account/read")
        .unwrap();
    session.receive(json!({"id": account["id"], "result": {
        "account": {"type": "chatgpt"}, "requiresOpenaiAuth": true
    }}));
    session.receive(json!({"id": models["id"], "result": {
        "data": [{"model": "gpt-6.1-sol"}], "nextCursor": null
    }}));

    let open = if resume.is_some() {
        requests
            .iter()
            .find(|r| r["method"] == "thread/resume")
            .unwrap()
            .clone()
    } else {
        take_request(&mut session, "thread/start")
    };
    if let Some(id) = resume {
        assert!(open["params"].get("cwd").is_none());
        assert_eq!(open["params"]["threadId"], id);
        assert_eq!(open["params"]["excludeTurns"], true);
    } else {
        assert_eq!(open["params"]["cwd"], "/tmp/project");
        assert_eq!(open["params"]["model"], "gpt-6.1-sol");
    }
    session.receive(json!({"id": open["id"], "result": {
        "thread": {"id": "thread-1"}, "model": "gpt-6.1-sol"
    }}));
    session
}

#[test]
fn model_and_effort_apply_to_the_next_turn_and_validate_catalog_choices() {
    let mut s = ready(None);
    s.models = vec![melly::catalog::Model::parse(&json!({"model":"gpt-6-astra",
        "defaultReasoningEffort":"medium","supportedReasoningEfforts":[
        {"reasoningEffort":"medium","description":"Balanced"},{"reasoningEffort":"high","description":"More reasoning"}]})).unwrap()];
    assert!(!s.select_model("made-up"));
    assert!(s.select_model("gpt-6-astra"));
    assert_eq!(s.effort.as_deref(), Some("medium"));
    assert!(!s.select_effort("impossible"));
    assert!(s.select_effort("high"));
    let r = start(&mut s);
    assert_eq!(r["params"]["model"], "gpt-6-astra");
    assert_eq!(r["params"]["effort"], "high");
    assert_eq!(r["params"]["summary"], "auto");
    s.receive(json!({"id":r["id"],"result":{"turn":{"id":"t"}}}));
    s.select_effort("medium");
    s.submit("Small correction");
    let r = take_request(&mut s, "turn/steer");
    assert!(r["params"].get("effort").is_none());
}

#[test]
fn compaction_waits_for_completion_and_blocks_new_prompts() {
    let mut s = ready(None);
    assert!(s.compact());
    let r = take_request(&mut s, "thread/compact/start");
    s.receive(json!({"id":r["id"],"result":{}}));
    assert!(!s.submit("Should wait"));
    assert!(!s.compact());
    s.receive(json!({"method":"turn/started","params":{"threadId":"thread-1","turn":{"id":"compact-turn"}}}));
    s.receive(
        json!({"method":"item/completed","params":{"threadId":"thread-1","item":{
        "type":"contextCompaction","id":"c"}}}),
    );
    assert!(!s.submit("Still waiting for the compaction turn"));
    s.receive(json!({"method":"turn/completed","params":{"threadId":"thread-1","turn":{"id":"compact-turn","status":"completed"}}}));
    assert!(s.submit("Continue"));
}

#[test]
fn interrupting_pending_compaction_does_not_exit() {
    let mut s = ready(None);
    assert!(s.compact());
    take_request(&mut s, "thread/compact/start");
    assert!(s.interrupt());
    s.receive(json!({"method":"turn/started","params":{"threadId":"thread-1","turn":{"id":"c"}}}));
    assert_eq!(
        take_request(&mut s, "turn/interrupt")["params"]["turnId"],
        "c"
    );
}

#[test]
fn rejected_compaction_does_not_interrupt_the_next_turn() {
    let mut s = ready(None);
    s.compact();
    let r = take_request(&mut s, "thread/compact/start");
    s.interrupt();
    s.receive(json!({"id":r["id"],"error":{"message":"Cannot compact"}}));
    let r = start(&mut s);
    s.receive(json!({"id":r["id"],"result":{"turn":{"id":"next"}}}));
    assert!(s.drain_requests().is_empty());
}

#[test]
fn rerouting_preserves_queued_model_and_effort() {
    let mut s = ready(None);
    let request = start(&mut s);
    s.receive(json!({"id":request["id"],"result":{"turn":{"id":"t"}}}));
    s.model = Some("next-model".into());
    s.effort = Some("next-effort".into());
    s.receive(json!({"method":"model/rerouted","params":{"threadId":"thread-1","turnId":"t","toModel":"routed-model"}}));
    assert_eq!(s.model.as_deref(), Some("next-model"));
    assert_eq!(s.effort.as_deref(), Some("next-effort"));
    s.receive(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-1","turnId":"t","itemId":"a","delta":"Hello"}}));
    assert_eq!(
        s.entries.back().unwrap().model.as_deref(),
        Some("routed-model")
    );
}

#[test]
fn quota_notifications_preserve_other_buckets() {
    let mut s = ready(None);
    for (id, used) in [("codex", 10), ("astra", 20), ("codex", 15)] {
        s.receive(json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":id,"primary":{"usedPercent":used}}}}));
    }
    assert_eq!(s.quota.len(), 2);
    assert_eq!(
        s.quota
            .iter()
            .find(|q| q.bucket_id == "codex")
            .unwrap()
            .used_percent,
        15
    );
    assert_eq!(
        s.quota
            .iter()
            .find(|q| q.bucket_id == "astra")
            .unwrap()
            .used_percent,
        20
    );
}

#[test]
fn rename_approval_includes_the_destination() {
    let mut s = ready(None);
    s.receive(json!({"method":"item/started","params":{"threadId":"thread-1","item":{"id":"patch","type":"fileChange",
        "changes":[{"path":"old.txt","kind":{"type":"update","move_path":"new.txt"},"diff":"-old\n+new"}]}}}));
    s.receive(json!({"id":"approval","method":"item/fileChange/requestApproval","params":{"threadId":"thread-1","itemId":"patch"}}));
    assert!(s.approvals[0].description.contains("old.txt → new.txt"));
}

#[test]
fn new_chat_uses_catalog_effort_even_when_thread_opens_before_models() {
    let mut s = Session::new("/tmp/project".into(), Some("gpt-6-astra".into()), None);
    let init = take_request(&mut s, "initialize");
    s.receive(json!({"id":init["id"],"result":{}}));
    let requests = s.drain_requests();
    let thread = requests
        .iter()
        .find(|r| r["method"] == "thread/start")
        .unwrap();
    let models = requests
        .iter()
        .find(|r| r["method"] == "model/list")
        .unwrap();
    s.receive(json!({"id":thread["id"],"result":{"thread":{"id":"thread-1"},"model":"gpt-6-astra","reasoningEffort":"xhigh"}}));
    assert!(!s.submit("Wait for discovered settings"));
    s.receive(json!({"id":models["id"],"result":{"data":[{"model":"gpt-6-astra","defaultReasoningEffort":"medium"}]}}));
    let turn = start(&mut s);
    assert_eq!(turn["params"]["effort"], "medium");
}

#[test]
fn new_chat_waits_for_all_model_catalog_pages() {
    let mut s = Session::new("/tmp/project".into(), None, None);
    let init = take_request(&mut s, "initialize");
    s.receive(json!({"id":init["id"],"result":{}}));
    let models = take_request(&mut s, "model/list");
    s.receive(
        json!({"id":models["id"],"result":{"data":[{"model":"other"}],"nextCursor":"page-2"}}),
    );
    let requests = s.drain_requests();
    assert!(!requests.iter().any(|r| r["method"] == "thread/start"));
    let models = requests
        .iter()
        .find(|r| r["method"] == "model/list")
        .unwrap();
    s.receive(json!({"id":models["id"],"result":{"data":[{"model":"gpt-6.1-sol","defaultReasoningEffort":"low"}]}}));
    assert_eq!(
        take_request(&mut s, "thread/start")["params"]["model"],
        "gpt-6.1-sol"
    );
}

#[test]
fn context_snapshot_is_separate_from_cumulative_tokens() {
    let mut s = ready(None);
    s.receive(json!({"method":"thread/tokenUsage/updated","params":{"threadId":"thread-1",
        "tokenUsage":{"total":{"totalTokens":90000,"inputTokens":80000,"cachedInputTokens":60000,
            "outputTokens":10000,"reasoningOutputTokens":8000},"last":{"totalTokens":12000},"modelContextWindow":200000}}}));
    assert_eq!(s.tokens, Some(90000));
    assert_eq!(s.context_used, Some(12000));
    assert_eq!(s.cached_tokens, Some(60000));
}

#[test]
fn skills_attach_as_typed_inputs_and_are_consumed_once() {
    let mut s = ready(None);
    s.load_skills();
    let r = take_request(&mut s, "skills/list");
    s.receive(json!({"id":r["id"],"result":{"data":[{"skills":[
        {"name":"review","path":"/tmp/skills/review/SKILL.md","enabled":true},
        {"name":"disabled","path":"/tmp/skills/disabled/SKILL.md","enabled":false}
    ],"errors":[]}]}}));
    assert_eq!(s.skills.len(), 1);
    s.attach_skill(0);
    s.attach_skill(0);
    let r = start(&mut s);
    assert_eq!(r["params"]["input"].as_array().unwrap().len(), 2);
    assert_eq!(
        r["params"]["input"][1],
        json!({"type":"skill","name":"review","path":"/tmp/skills/review/SKILL.md"})
    );
    assert!(s.selected_skills.is_empty());
}

fn start(session: &mut Session) -> Value {
    assert!(session.submit("Fix the test."));
    let request = take_request(session, "turn/start");
    assert_eq!(request["params"]["threadId"], "thread-1");
    assert_eq!(request["params"]["input"][0]["text"], "Fix the test.");
    request
}

#[test]
fn streams_only_the_open_thread_and_replaces_delta_with_final_message() {
    let mut session = ready(None);
    let request = start(&mut session);
    session.receive(json!({"id": request["id"], "result": {"turn": {"id": "turn-1"}}}));
    session.receive(json!({"method": "item/agentMessage/delta", "params": {
        "threadId": "other", "turnId": "turn-1", "itemId": "m", "delta": "wrong thread"
    }}));
    session.receive(json!({"method": "item/agentMessage/delta", "params": {
        "threadId": "thread-1", "turnId": "turn-1", "itemId": "m", "delta": "Test "
    }}));
    assert!(session.transcript().contains("Test "));
    session.receive(json!({"method": "item/completed", "params": {
        "threadId": "thread-1", "turnId": "turn-1",
        "item": {"type": "agentMessage", "id": "m", "text": "Test passed."}
    }}));
    assert_eq!(session.transcript().matches("Test passed.").count(), 1);
    assert!(!session.transcript().contains("wrong thread"));
}

#[test]
fn a_late_start_ack_does_not_resurrect_a_finished_turn() {
    let mut session = ready(None);
    let start = start(&mut session);
    session.receive(json!({"method":"turn/started","params":{
        "threadId":"thread-1","turn":{"id":"turn-1","status":"inProgress"}
    }}));
    session.receive(json!({"method":"turn/completed","params":{
        "threadId":"thread-1","turn":{"id":"turn-1","status":"completed"}
    }}));
    session.receive(json!({"id":start["id"],"result":{"turn":{"id":"turn-1"}}}));
    assert!(session.submit("A second task"));
    take_request(&mut session, "turn/start");
}

#[test]
fn steers_and_interrupts_the_active_turn() {
    let mut session = ready(None);
    let start = start(&mut session);
    session.receive(json!({"id":start["id"],"result":{"turn":{"id":"turn-1"}}}));
    assert!(session.submit("Use a smaller patch"));
    let steer = take_request(&mut session, "turn/steer");
    assert_eq!(steer["params"]["expectedTurnId"], "turn-1");
    assert!(session.interrupt());
    let stop = take_request(&mut session, "turn/interrupt");
    assert_eq!(stop["params"]["turnId"], "turn-1");
}

#[test]
fn an_approval_is_answered_once_with_the_original_rpc_id() {
    let mut session = ready(None);
    session.receive(
        json!({"id":"approval-9","method":"item/commandExecution/requestApproval","params":{
            "threadId":"thread-1","turnId":"turn-1","itemId":"exec","command":"git diff"
        }}),
    );
    session.approve(false);
    session.approve(true);
    assert_eq!(
        session.drain_requests(),
        vec![json!({"id":"approval-9","result":{"decision":"decline"}})]
    );
}

#[test]
fn resumed_history_is_displayed_in_order() {
    let mut session = ready(Some("thread-1"));
    let request = take_request(&mut session, "thread/items/list");
    assert_eq!(request["params"]["sortDirection"], "desc");
    session.receive(json!({"id":request["id"],"result":{"data":[
        {"turnId":"turn-1","item":{"id":"m","type":"agentMessage","text":"Latest answer"}},
        {"turnId":"turn-1","item":{"id":"u","type":"userMessage","content":[{"type":"text","text":"Earlier question"}]}}
    ],"nextCursor":null}}));
    let transcript = session.transcript();
    assert!(
        transcript.find("Earlier question").unwrap() < transcript.find("Latest answer").unwrap()
    );
}

#[test]
fn a_directory_alone_is_not_enough_to_approve_a_command() {
    let mut session = ready(None);
    session.receive(
        json!({"id":"opaque","method":"item/commandExecution/requestApproval","params":{
            "threadId":"thread-1","turnId":"turn-1","itemId":"exec","cwd":"/tmp/project"
        }}),
    );
    session.approve(true);
    assert!(session.drain_requests().is_empty());
}

#[test]
fn a_command_can_only_be_approved_using_an_offered_decision() {
    let mut session = ready(None);
    session.receive(
        json!({"id":"opaque","method":"item/commandExecution/requestApproval","params":{
            "threadId":"thread-1","turnId":"turn-1","itemId":"exec",
            "command":"git diff","availableDecisions":["decline","cancel"]
        }}),
    );
    session.approve(true);
    assert!(session.drain_requests().is_empty());
}

#[test]
fn patch_approval_shows_the_patch_not_just_the_path() {
    let mut session = ready(None);
    session.receive(json!({"method":"item/started","params":{
        "threadId":"thread-1","turnId":"turn-1",
        "item":{"id":"patch","type":"fileChange","status":"inProgress","changes":[
            {"path":"file.txt","kind":{"type":"update"},"diff":"-old\n+new"}
        ]}
    }}));
    session.receive(
        json!({"id":"patch-approve","method":"item/fileChange/requestApproval","params":{
            "threadId":"thread-1","turnId":"turn-1","itemId":"patch"
        }}),
    );
    assert!(session.approvals[0].description.contains("-old\n+new"));
    assert!(session.approvals[0].can_accept);
}

#[test]
fn interrupt_waits_for_the_turn_id_and_sends_once() {
    let mut session = ready(None);
    let start = start(&mut session);
    assert!(session.interrupt());
    assert!(session.drain_requests().is_empty());

    session.receive(json!({"method":"turn/started","params":{
        "threadId":"thread-1","turn":{"id":"turn-1","status":"inProgress"}
    }}));
    let stop = take_request(&mut session, "turn/interrupt");
    assert_eq!(stop["params"]["turnId"], "turn-1");
    session.receive(json!({"id":stop["id"],"result":{}}));
    session.receive(json!({"id":start["id"],"result":{"turn":{"id":"turn-1"}}}));
    assert!(session.drain_requests().is_empty());
}

#[test]
fn thinking_summaries_stream_in_order_and_complete_without_duplication() {
    let mut session = ready(None);
    for (index, delta) in [(0, "Inspect the tests."), (1, "Check the caller.")] {
        session.receive(json!({"method":"item/reasoning/summaryTextDelta","params":{
            "threadId":"thread-1","turnId":"turn-1","itemId":"thought",
            "summaryIndex":index,"delta":delta
        }}));
    }
    assert!(
        session
            .transcript()
            .contains("Inspect the tests.\n\nCheck the caller.")
    );
    session.receive(json!({"method":"item/completed","params":{
        "threadId":"thread-1","turnId":"turn-1","item":{
            "type":"reasoning","id":"thought","summary":["Inspect the tests.","Check the caller."],
            "content":["private raw content is not a summary"]
        }
    }}));
    assert_eq!(
        session.transcript().matches("Inspect the tests.").count(),
        1
    );
    assert!(!session.transcript().contains("private raw content"));
}

#[test]
fn emitted_raw_reasoning_is_shown_until_a_summary_arrives() {
    let mut session = ready(None);
    session.receive(json!({"method":"item/reasoning/textDelta","params":{
        "threadId":"thread-1","turnId":"turn-1","itemId":"thought",
        "contentIndex":0,"delta":"Explicit backend reasoning."
    }}));
    assert!(session.transcript().contains("Explicit backend reasoning."));
    session.receive(json!({"method":"item/reasoning/summaryTextDelta","params":{
        "threadId":"thread-1","turnId":"turn-1","itemId":"thought",
        "summaryIndex":0,"delta":"Concise summary."
    }}));
    assert!(session.transcript().contains("Concise summary."));
    assert!(!session.transcript().contains("Explicit backend reasoning."));
    session.receive(json!({"method":"item/reasoning/textDelta","params":{
        "threadId":"thread-1","turnId":"turn-1","itemId":"thought",
        "contentIndex":0,"delta":"Late raw output."
    }}));
    assert!(!session.transcript().contains("Late raw output."));
}

#[test]
fn emitted_raw_reasoning_content_is_visible_without_a_summary() {
    let mut session = ready(None);
    session.receive(json!({"method":"item/completed","params":{
        "threadId":"thread-1","turnId":"turn-1","item":{
            "type":"reasoning","id":"thought","summary":[],
            "content":["Explicit backend content."]
        }
    }}));
    assert!(session.transcript().contains("Explicit backend content."));
}

#[test]
fn worker_starts_fresh_without_handshake_and_keeps_selected_low_effort() {
    let model = melly::catalog::Model::parse(&json!({"model":"gpt-6-luna",
        "defaultReasoningEffort":"medium", "supportedReasoningEfforts":[{"reasoningEffort":"low"}]})).unwrap();
    let mut session = Session::worker("/tmp/project".into(), model, true);
    let requests = session.drain_requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["sandbox"], "read-only");
    assert_eq!(request["params"]["approvalPolicy"], "never");
    assert!(request["params"].get("dynamicTools").is_none());
    assert!(!session.submit("too early"));
    session.receive(json!({"id":request["id"],"result":{"thread":{"id":"worker"},"model":"gpt-6-luna","reasoningEffort":"medium"}}));
    assert!(session.submit("Inspect one module"));
    let turn = take_request(&mut session, "turn/start");
    assert_eq!(turn["params"]["effort"], "low");
    session.receive(json!({"id":turn["id"],"result":{"turn":{"id":"work-turn"}}}));
    assert_eq!(session.active_turn_id(), Some("work-turn"));
}

#[test]
fn quota_snapshots_record_per_bucket_freshness() {
    let mut session = ready(None);
    assert!(session.quota_updated_at.is_empty());
    session.receive(
        json!({"method":"account/rateLimits/updated","params":{"rateLimits":{
        "limitId":"codex","primary":{"usedPercent":10}}}}),
    );
    assert!(session.quota_updated_at.contains_key("codex"));
}

#[test]
fn account_changes_invalidate_quota_and_request_a_fresh_snapshot() {
    let mut session = ready(None);
    session.receive(
        json!({"method":"account/rateLimits/updated","params":{"rateLimits":{
        "limitId":"codex","primary":{"usedPercent":10}}}}),
    );
    assert!(session.receive(json!({"method":"account/updated","params":{"authMode":"chatgpt"}})));
    assert!(session.quota_updated_at.is_empty());
    assert!(!session.quota_loaded);
    assert!(session.quota.is_empty());
    assert_eq!(
        take_request(&mut session, "account/rateLimits/read")["method"],
        "account/rateLimits/read"
    );
}

#[test]
fn final_raw_reasoning_never_replaces_an_already_streamed_summary() {
    let mut session = ready(None);
    session.receive(json!({"method":"item/reasoning/summaryTextDelta","params":{
        "threadId":"thread-1","itemId":"thought","summaryIndex":0,"delta":"Concise summary."
    }}));
    session.receive(
        json!({"method":"item/completed","params":{"threadId":"thread-1",
        "item":{"type":"reasoning","id":"thought","summary":[],"content":["Raw content."]}}}),
    );
    assert!(session.transcript().contains("Concise summary."));
    assert!(!session.transcript().contains("Raw content."));
}

#[test]
fn commands_requesting_additional_permissions_cannot_be_approved_without_review() {
    let mut session = ready(None);
    session.receive(json!({"id":"extra-permissions","method":"item/commandExecution/requestApproval","params":{
        "threadId":"thread-1","command":"cargo test","additionalPermissions":{"fileSystem":{"write":["/outside/workspace"]}}
    }}));
    assert!(!session.approvals.front().unwrap().can_accept);
    session.approve(true);
    assert!(session.drain_requests().is_empty());
    session.approve(false);
    assert_eq!(session.drain_requests()[0]["result"]["decision"], "decline");
}
