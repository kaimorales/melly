#!/usr/bin/env python3
"""Local peer with unique fresh IDs and interruptible turns for session switching."""
import json
import os
import sys
import time


def send(value):
    print(json.dumps(value), flush=True)


def event(method, thread, **fields):
    send({"method": method, "params": {"threadId": thread, **fields}})


for line in sys.stdin:
    message = json.loads(line)
    with open(os.environ["MELLY_TEST_RPC_LOG"], "a") as log:
        log.write(json.dumps(message) + "\n")
    method, params = message.get("method"), message.get("params", {})
    if method == "initialized":
        continue
    if method in ("initialize", "account/read"):
        result = {"account": {"type": "chatgpt"}} if method == "account/read" else {}
    elif method == "account/rateLimits/read":
        result = {"rateLimits": {"limitId": "codex", "primary": {"usedPercent": 10}}}
    elif method == "model/list":
        result = {"data": [{"model": name, "defaultReasoningEffort": "low", "supportedReasoningEfforts": [{"reasoningEffort": "low"}]}
                           for name in ["gpt-6.1-sol", "gpt-6-luna"]]}
    elif method in ("thread/start", "thread/resume"):
        thread = params.get("threadId", f"history-new-{time.time_ns()}")
        result = {"thread": {"id": thread}, "model": params.get("model", "gpt-6.1-sol")}
    elif method == "thread/items/list":
        result = {"data": [{"item": {"type": "agentMessage", "id": "history-answer", "phase": "final_answer",
                                    "text": "Restored " + params["threadId"]}}]}
    elif method == "turn/start":
        thread = params["threadId"]
        turn = "turn-" + str(message["id"])
        send({"id": message["id"], "result": {"turn": {"id": turn}}})
        event("turn/started", thread, turn={"id": turn})
        if "hold" not in params["input"][0]["text"]:
            event("item/completed", thread, turnId=turn, item={"type": "agentMessage", "id": turn,
                  "phase": "final_answer", "text": "HISTORY_TURN_COMPLETE"})
            event("turn/completed", thread, turn={"id": turn, "status": "completed"})
        continue
    elif method == "turn/interrupt":
        result = {}
        event("turn/completed", params["threadId"], turn={"id": params["turnId"], "status": "interrupted"})
    else:
        raise AssertionError(method)
    send({"id": message["id"], "result": result})
