#!/usr/bin/env python3
"""Deterministic App Server peer; no network or model calls."""
import json
import os
from pathlib import Path
import sys
import time

THREAD = "fixture-thread"
TURN = "fixture-turn"


def send(message):
    print(json.dumps(message), flush=True)


def notify(method, **params):
    send({"method": method, "params": {"threadId": THREAD, "turnId": TURN, **params}})


for line in sys.stdin:
    message = json.loads(line)
    if path := os.environ.get("MELLY_TEST_RPC_LOG"):
        with Path(path).open("a") as log:
            log.write(json.dumps(message) + "\n")
    method = message.get("method")
    if method == "initialized":
        continue
    if method == "initialize":
        result = {}
    elif method == "account/read":
        result = {"account": {"type": "chatgpt"}}
    elif method == "account/rateLimits/read":
        result = {"rateLimits": {"limitId": "codex", "primary": {
            "usedPercent": 14, "windowDurationMins": 300}}}
    elif method == "model/list":
        result = {"data": [{"model": model, "displayName": name,
            "defaultReasoningEffort": "medium", "description": "Fixture model",
            "supportedReasoningEfforts": [{"reasoningEffort": effort, "description": effort}
                for effort in ["low", "medium", "high"]]}
            for model, name in [("gpt-6-luna", "GPT-6 Luna"), ("gpt-6-astra", "GPT-6 Astra")]], "nextCursor": None}
    elif method == "thread/start":
        result = {"thread": {"id": THREAD}, "model": message["params"]["model"], "reasoningEffort": "medium"}
    elif method == "skills/list":
        result = {"data": [{"skills": [{"name": "review", "path": "/fixture/review/SKILL.md",
            "description": "Review a change", "enabled": True}], "errors": []}]}
    elif method == "thread/compact/start":
        send({"id": message["id"], "result": {}})
        notify("turn/started", turn={"id": "compact", "status": "inProgress"})
        notify("item/started", item={"type": "contextCompaction", "id": "compaction"})
        time.sleep(0.1)
        notify("item/completed", item={"type": "contextCompaction", "id": "compaction"})
        notify("turn/completed", turn={"id": "compact", "status": "completed"})
        continue
    elif method == "turn/start":
        assert message["params"]["threadId"] == THREAD
        assert message["params"]["input"][0]["text"] == "Test the terminal connection"
        assert message["params"]["summary"] == "auto"
        send({"id": message["id"], "result": {"turn": {"id": TURN}}})
        notify("item/started", item={"id": "reason", "type": "reasoning", "summary": []})
        notify("item/reasoning/summaryTextDelta", itemId="reason", summaryIndex=0, delta="Checking the terminal connection.")
        time.sleep(0.15)
        notify("item/completed", item={"id": "reason", "type": "reasoning", "summary": ["Checking the terminal connection."]})
        notify("thread/tokenUsage/updated", tokenUsage={"last": {"totalTokens": 12000},
            "total": {"totalTokens": 26000, "inputTokens": 24000, "cachedInputTokens": 18000,
                "outputTokens": 2000, "reasoningOutputTokens": 1500}, "modelContextWindow": 200000})
        notify("item/agentMessage/delta", itemId="answer", delta="MELLY_SMOKE_OK")
        notify("turn/completed", turn={"id": TURN, "status": "completed"})
        continue
    else:
        raise AssertionError(f"Unexpected method {method}")
    send({"id": message["id"], "result": result})
