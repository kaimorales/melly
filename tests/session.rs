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

    let open = take_request(
        &mut session,
        if resume.is_some() {
            "thread/resume"
        } else {
            "thread/start"
        },
    );
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
