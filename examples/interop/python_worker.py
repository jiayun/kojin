#!/usr/bin/env python3
"""
Cross-language interop example: Python worker consuming/producing
Kojin-format messages via Redis.

Requirements:
    pip install redis

This connects to the same Redis broker as a Rust Kojin worker and
demonstrates producing and consuming tasks using the standard wire format.
See docs/message-schema.json for the full schema.
"""

import json
import uuid
import time
from datetime import datetime, timezone

import redis


# -- Configuration --
REDIS_URL = "redis://localhost:6379"
KEY_PREFIX = "kojin"
QUEUE = "default"


def make_task_message(
    task_name: str,
    payload: dict,
    queue: str = QUEUE,
    max_retries: int = 3,
    priority: int | None = None,
    dedup_key: str | None = None,
) -> dict:
    """Create a Kojin-compatible TaskMessage."""
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    msg = {
        "id": str(uuid.uuid7()) if hasattr(uuid, "uuid7") else str(uuid.uuid4()),
        "task_name": task_name,
        "queue": queue,
        "payload": payload,
        "state": "pending",
        "retries": 0,
        "max_retries": max_retries,
        "created_at": now,
        "updated_at": now,
        "eta": None,
        "headers": {},
    }
    if priority is not None:
        msg["priority"] = min(priority, 9)
    if dedup_key is not None:
        msg["dedup_key"] = dedup_key
    return msg


def enqueue(r: redis.Redis, message: dict) -> str:
    """Push a task message to a Kojin queue (LPUSH, same as RedisBroker)."""
    queue_key = f"{KEY_PREFIX}:queue:{message['queue']}"
    serialized = json.dumps(message)
    r.lpush(queue_key, serialized)
    print(f"[enqueue] {message['task_name']} -> {queue_key} (id={message['id']})")
    return message["id"]


def dequeue(r: redis.Redis, queue: str = QUEUE, timeout: int = 5) -> dict | None:
    """Pop a task message from a Kojin queue (BRPOP, consuming from the right)."""
    queue_key = f"{KEY_PREFIX}:queue:{queue}"
    result = r.brpop(queue_key, timeout=timeout)
    if result:
        _, data = result
        message = json.loads(data)
        print(f"[dequeue] {message['task_name']} (id={message['id']})")
        return message
    return None


def process_task(message: dict) -> dict:
    """Example task processor — just echoes payload."""
    task_name = message["task_name"]
    payload = message["payload"]
    print(f"[process] Running {task_name} with payload: {payload}")

    # Simulate work
    time.sleep(0.1)

    return {"status": "completed", "result": payload}


# -- Main --
if __name__ == "__main__":
    r = redis.from_url(REDIS_URL)

    # 1. Produce a task (could be consumed by a Rust Kojin worker)
    msg = make_task_message(
        "send_email",
        {"to": "user@example.com", "subject": "Hello from Python"},
        priority=5,
    )
    enqueue(r, msg)

    # 2. Consume a task (could have been produced by a Rust Kojin worker)
    task = dequeue(r, timeout=2)
    if task:
        result = process_task(task)
        print(f"[result] {json.dumps(result)}")
    else:
        print("[dequeue] No task available")
