#!/usr/bin/env python3
"""Minimal JSON-RPC peer for the terminal smoke test."""

import json
import sys

THREAD = "fixture-thread"
TURN = "fixture-turn"


def send(message):
    print(json.dumps(message), flush=True)


for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialized":
        continue
    if method == "initialize":
        result = {}
    elif method == "account/read":
        result = {"account": {"type": "chatgpt"}}
    elif method == "model/list":
        result = {"data": [{"model": "gpt-6-luna"}], "nextCursor": None}
    elif method == "thread/start":
        assert message["params"]["model"] == "gpt-6-luna"
        result = {"thread": {"id": THREAD}, "model": "gpt-6-luna"}
    elif method == "turn/start":
        assert message["params"]["threadId"] == THREAD
        assert message["params"]["input"][0]["text"] == "Test the terminal connection"
        send({"id": message["id"], "result": {"turn": {"id": TURN}}})
        send({"method": "item/agentMessage/delta", "params": {
            "threadId": THREAD, "turnId": TURN, "itemId": "answer", "delta": "MELLY_SMOKE_OK"
        }})
        send({"method": "turn/completed", "params": {
            "threadId": THREAD, "turn": {"id": TURN, "status": "completed"}
        }})
        continue
    else:
        raise AssertionError(f"Unexpected method {method}")
    send({"id": message["id"], "result": result})
