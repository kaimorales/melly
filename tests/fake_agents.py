#!/usr/bin/env python3
"""Interleaved deterministic three-worker peer, including a rejected third auto launch."""
import json
import os
import sys

threads = []
workers = {}
spawned = []
completed = False


def send(value):
    print(json.dumps(value), flush=True)


def event(method, thread, **fields):
    send({"method": method, "params": {"threadId": thread, "turnId": thread + "-turn", **fields}})


def finish(thread, text):
    event("item/completed", thread, item={"type": "agentMessage", "id": thread + "-answer", "text": text})
    event("thread/tokenUsage/updated", thread, tokenUsage={"total": {"totalTokens": 1000 * (threads.index(thread) + 1)},
        "last": {"totalTokens": 500}, "modelContextWindow": 100000})
    event("turn/completed", thread, turn={"id": thread + "-turn", "status": "completed"})


def call(id, tool, arguments):
    send({"id": id, "method": "item/tool/call", "params": {"threadId": "lead", "turnId": "lead-turn",
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
            if len(spawned) == 2:
                call("auto-denied", "spawn", spawn_args("extra.txt"))
                call("wait", "wait", {"ids": spawned, "timeout_ms": 120000})
        elif m.get("id") == "wait":
            finish("lead", "TEAM_COMPLETE")
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
        send({"id": m["id"], "result": {"turn": {"id": thread + "-turn"}}})
        if thread == "lead":
            call("auto-a", "spawn", spawn_args("a.txt"))
            call("auto-b", "spawn", spawn_args("b.txt"))
        elif completed:
            finish(thread, "WORKER_FOLLOWUP")
        else:
            workers[thread] = True
            event("item/started", thread, item={"type": "reasoning", "id": thread + "-thought", "summary": []})
            event("item/reasoning/summaryTextDelta", thread, itemId=thread + "-thought", summaryIndex=0, delta="Inspecting this worker's scope")
            if len(workers) == 3:
                for worker in reversed(workers):
                    finish(worker, "Checked " + worker)
                completed = True
        continue
    else:
        raise AssertionError(method)
    send({"id": m["id"], "result": result})
