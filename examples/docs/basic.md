# Basic: Multi-Worker Fan-Out

Demonstrates distributed task processing where multiple workers compete for tasks from a single Redis queue.

## Run

```bash
docker compose --profile basic up
```

## Architecture

```
┌──────────────────┐
│  producer-basic   │
│  (SCENARIO=basic) │
└────────┬─────────┘
         │ enqueue 20 AddTasks
         ▼
┌──────────────────┐
│      Redis       │
│                  │
│  queue:default   │
│  [task][task]... │
└──┬─────┬─────┬──┘
   │     │     │    BRPOPLPUSH (competing consumers)
   ▼     ▼     ▼
┌──────┐┌──────┐┌──────┐
│worker││worker││worker│
│  -1  ││  -2  ││  -3  │
└──────┘└──────┘└──────┘
```

## What Happens

1. **Redis** starts and passes healthcheck
2. **3 worker replicas** start, each polling `queue:default` via `BRPOPLPUSH`
3. **producer-basic** enqueues 20 `AddTask` items (each adds two numbers with a 200ms delay)
4. Workers compete — Redis atomically pops one task per request, so no task is processed twice
5. Tasks distribute roughly evenly across workers (visible in Docker log prefixes)
6. Producer exits after enqueuing; workers keep running until stopped

## Services

| Service | Image | Role |
|---------|-------|------|
| redis | redis:7-alpine | Message broker |
| worker (x3) | kojin-examples | Task consumers polling `default` queue |
| producer-basic | kojin-examples | Enqueues 20 `AddTask` items, then exits |

## Scaling

Override the default 3 workers:

```bash
docker compose --profile basic up --scale worker=5
```
