# Chord: Workflow + PostgreSQL Result Backend

Demonstrates a chord workflow — a group of tasks that run in parallel, followed by a callback task that fires once all group members complete. Results are stored durably in PostgreSQL.

## Run

```bash
docker compose --profile chord up
```

## Architecture

```
┌───────────────────┐         ┌────────────────┐
│  producer-chord    │────────>│   PostgreSQL    │
│  (SCENARIO=chord)  │ apply   │                │
└────────┬──────────┘ chord   │  kojin_groups   │
         │                    │  kojin_results  │
         │ enqueue            └───────▲─────────┘
         ▼                            │ store results
┌──────────────────┐                  │
│      Redis       │          ┌───────┴─────────┐
│                  │          │    worker-pg     │
│  queue:orders    │─────────>│                  │
│  [ORD-001]       │  dequeue │  ProcessOrder x5 │
│  [ORD-002]       │          │  AddTask (cb)    │
│  [ORD-003]       │          └──────────────────┘
│  [ORD-004]       │
│  [ORD-005]       │
│                  │
│  queue:default   │
│  (callback here) │
└──────────────────┘
```

## What Happens

1. **Redis** and **PostgreSQL** start and pass healthchecks
2. **worker-pg** starts with both `REDIS_URL` and `POSTGRES_URL`
3. **producer-chord** creates a chord workflow:
   - **Group**: 5 `ProcessOrder` tasks (ORD-001 to ORD-005, each with 500ms delay)
   - **Callback**: 1 `AddTask(100, 200)` that fires after all orders complete
4. Producer writes chord group metadata to **PostgreSQL** (`kojin_groups` table) and enqueues tasks to **Redis**
5. Worker processes each `ProcessOrder` from `queue:orders`, stores results in PostgreSQL
6. After all 5 complete, kojin detects the group is done and enqueues the callback `AddTask` to `queue:default`
7. Worker processes the callback

## PostgreSQL Tables

Connect to inspect results:

```bash
psql postgres://kojin:kojin@localhost:5432/kojin
```

| Table | Contents |
|-------|----------|
| `kojin_results` | Individual task results (task_id, result JSON, expires_at) |
| `kojin_groups` | Chord group tracking (group_id, task_id, result, completed) |

## Services

| Service | Image | Role |
|---------|-------|------|
| redis | redis:7-alpine | Message broker (task queues) |
| postgres | postgres:16-alpine | Result backend (durable storage) |
| worker-pg | kojin-examples | Worker with Postgres result backend |
| producer-chord | kojin-examples | Creates chord workflow, then exits |
