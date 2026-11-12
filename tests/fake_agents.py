#!/usr/bin/env python3
"""Interleaved deterministic three-worker peer, including a rejected third auto launch."""
import json
import os
import sys

threads = []
workers = {}
spawned = []
completed = False
resumed = False
turns = {}


def send(value):
    print(json.dumps(value), flush=True)


def event(method, thread, **fields):
    send({"method": method, "params": {"threadId": thread, "turnId": turns.get(thread, thread + "-turn"), **fields}})


def finish(thread, text):
    event("item/completed", thread, item={"type": "agentMessage", "id": thread + "-answer", "text": text})
    event("thread/tokenUsage/updated", thread, tokenUsage={"total": {"totalTokens": 1000 * (threads.index(thread) + 1)},
        "last": {"totalTokens": 500}, "modelContextWindow": 100000})
    event("turn/completed", thread, turn={"id": turns.get(thread, thread + "-turn"), "status": "completed"})


def call(id, tool, arguments):
    send({"id": id, "method": "item/tool/call", "params": {"threadId": "lead", "turnId": turns["lead"],
        "callId": id, "namespace": "melly_agents", "tool": tool, "arguments": arguments}})


def spawn_args(file):
    return {"objective": "Inspect the independent " + file + " fixture", "context": "Read only this file.",
        "expected_output": "A concise report of its contents.", "reason": "Independent file checks can run in parallel.", "mode": "read", "paths": [file]}


for line in sys.stdin:
    m = json.loads(line)
    if path := os.environ.get("MELLY_TEST_RPC_LOG"):
        with open(path, "a") as log:
            log.write(json.dumps(m) + "\n")
    method = m.get("method")
    if method == "initialized":
        continue
    if method is None:
        if m.get("id") in ("auto-a", "auto-b"):
            body = json.loads(m["result"]["contentItems"][0]["text"])
            spawned.append(body["id"])
            if resumed:
                call("wait", "wait", {"ids": spawned, "timeout_ms": 120000})
            elif len(spawned) == 2:
                call("auto-denied", "spawn", spawn_args("extra.txt"))
                call("wait", "wait", {"ids": spawned, "timeout_ms": 120000})
        elif m.get("id") == "wait":
            if not resumed:
                reports = json.loads(m["result"]["contentItems"][0]["text"])["agents"]
                assert all(a["observed_commands"] == [{"command":"cargo test","exit_code":0}] for a in reports)
            if resumed:
                call("lead-compact", "compact", {"id": "lead"})
            else:
                finish("lead", "TEAM_COMPLETE")
        elif m.get("id") == "lead-compact":
            assert m["result"]["success"]
            finish("lead", "RESUMED_TEAM_COMPLETE")
        continue
    if method == "initialize":
        assert m["params"]["capabilities"]["experimentalApi"]
        result = {}
    elif method == "account/read":
        result = {"account": {"type": "chatgpt"}}
    elif method == "account/rateLimits/read":
        result = {"rateLimits": {"limitId": "codex", "primary": {"usedPercent": 10}}}
    elif method == "model/list":
        result = {"data": [{"model": name, "defaultReasoningEffort": "low", "supportedReasoningEfforts": [{"reasoningEffort": "low"}]}
            for name in ["gpt-6.1-sol", "gpt-6-luna"]]}
    elif method == "thread/resume":
        thread = m["params"]["threadId"]
        if thread == "missing-lead":
            send({"id": m["id"], "error": {"code": -32600, "message": "no rollout found for thread id missing-lead"}})
            continue
        assert "dynamicTools" not in m["params"]
        if thread != "lead":
            assert m["params"]["sandbox"] == "read-only"
            assert m["params"]["approvalPolicy"] == "never"
        threads.append(thread)
        resumed = True
        result = {"thread": {"id": thread}, "model": "gpt-6.1-sol" if thread == "lead" else "gpt-6-luna"}
    elif method == "thread/items/list":
        thread = m["params"]["threadId"]
        result = {"data": [{"item": {"type": "agentMessage", "id": "history-" + thread, "phase": "final_answer", "text": "Restored " + thread}}]}
    elif method == "thread/compact/start":
        thread = m["params"]["threadId"]
        turn = turns[thread] = f"{thread}-compact"
        send({"id": m["id"], "result": {}})
        event("turn/started", thread, turn={"id": turn})
        event("item/completed", thread, item={"type": "contextCompaction", "id": turn})
        event("turn/completed", thread, turn={"id": turn, "status": "completed"})
        continue
    elif method == "thread/start":
        thread = "lead" if not threads else "worker-" + str(len(threads))
        if thread == "lead":
            assert m["params"]["dynamicTools"][0]["name"] == "melly_agents"
        else:
            assert "dynamicTools" not in m["params"]
        threads.append(thread)
        result = {"thread": {"id": thread}, "model": m["params"]["model"]}
    elif method == "turn/start":
        thread = m["params"]["threadId"]
        turn = turns[thread] = f"{thread}-turn-{m['id']}"
        send({"id": m["id"], "result": {"turn": {"id": turn}}})
        if thread == "lead":
            assert m["params"]["sandboxPolicy"] == {"type":"readOnly","networkAccess":False}
            assert "melly-conductor" in m["params"]["additionalContext"]
            call("auto-a", "spawn", spawn_args("a.txt"))
            if not resumed:
                call("auto-b", "spawn", spawn_args("b.txt"))
        elif resumed:
            finish(thread, "RESUMED_LUNA_DONE")
        elif completed:
            finish(thread, "WORKER_FOLLOWUP")
        else:
            workers[thread] = True
            event("item/started", thread, item={"type": "reasoning", "id": thread + "-thought", "summary": []})
            event("item/reasoning/summaryTextDelta", thread, itemId=thread + "-thought", summaryIndex=0, delta="Inspecting this worker's scope")
            if len(workers) == 2:
                for worker in reversed(workers):
                    event("item/completed", worker, item={"type":"commandExecution","id":worker+"-check","command":"cargo test","exitCode":0,"status":"completed"})
                    finish(worker, "Checked " + worker)
                completed = True
        continue
    else:
        raise AssertionError(method)
    send({"id": m["id"], "result": result})
