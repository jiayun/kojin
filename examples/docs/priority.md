# Priority: Weighted Queues

Demonstrates task prioritization using separate Redis queues. Workers poll queues in priority order — `high` before `medium` before `low`.

## Run

```bash
docker compose --profile priority up
```

## Architecture

```
┌─────────────────────┐
│  producer-priority   │
│  (SCENARIO=priority) │
└──────────┬──────────┘
           │ enqueue 10 PriorityTasks
           ▼
┌─────────────────────────┐
│          Redis           │
│                          │
│  queue:high    [3 tasks] │ ◄── polled first
│  queue:medium  [5 tasks] │ ◄── polled second
│  queue:low     [2 tasks] │ ◄── polled last
└─────┬──────┬──────┬─────┘
      │      │      │
      ▼      ▼      ▼
  ┌──────┐┌──────┐┌──────┐
  │worker││worker││worker│
  │  -1  ││  -2  ││  -3  │
  └──────┘└──────┘└──────┘
```

## What Happens

1. **Redis** starts and passes healthcheck
2. **3 worker replicas** start, each polling queues in order: `high`, `medium`, `low`, `default`, `orders`
3. **producer-priority** enqueues `PriorityTask` items across three queues:
   - **high**: 3 tasks
   - **medium**: 5 tasks
   - **low**: 2 tasks
4. Workers dequeue from `high` first — all 3 high-priority tasks are processed before medium
5. Then `medium` tasks are processed, then `low`
6. Producer exits after enqueuing; workers keep running

## Queue Ordering

The worker registers queues in this order:

```rust
.queues(vec![
    "default".into(),
    "orders".into(),
    "high".into(),
    "medium".into(),
    "low".into(),
])
```

When multiple queues have pending tasks, the worker polls them in list order. Tasks in earlier queues are dequeued first.

## Services

| Service | Image | Role |
|---------|-------|------|
| redis | redis:7-alpine | Message broker (3 priority queues) |
| worker (x3) | kojin-examples | Workers polling queues in priority order |
| producer-priority | kojin-examples | Enqueues tasks across high/medium/low queues |
