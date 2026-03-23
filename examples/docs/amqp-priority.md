# AMQP Priority: RabbitMQ Message-Level Priority

Demonstrates native message-level priority using RabbitMQ's `x-max-priority` queue feature. Unlike the Redis `priority` profile (which uses separate queues), all tasks go into a **single queue** and RabbitMQ delivers higher-priority messages first.

## Run

```bash
docker compose --profile amqp-priority up
```

RabbitMQ management UI: http://localhost:15672 (guest/guest)

## Architecture

```
         Step 1: Producer enqueues all tasks, then exits

┌────────────────────┐
│   producer-amqp     │
└──────────┬─────────┘
           │ enqueue 108 tasks to ONE queue
           │ (100 low → 5 medium → 3 high)
           ▼
┌──────────────────────────────────────┐
│            RabbitMQ                   │
│                                      │
│  queue:priority (x-max-priority=9)   │
│  ┌──────────────────────────────┐    │
│  │ [low-0][low-1]...[low-99]   │    │  ◄── enqueued first
│  │ [medium-0]...[medium-4]     │    │  ◄── enqueued second
│  │ [high-0][high-1][high-2]    │    │  ◄── enqueued last
│  └──────────────────────────────┘    │
└──────────────────────────────────────┘

         Step 2: Worker starts, RabbitMQ delivers by priority

┌──────────────────────────────────────┐
│            RabbitMQ                   │
│                                      │
│  queue:priority (x-max-priority=9)   │
│  ┌──────────────────────────────┐    │
│  │ [high-0][high-1][high-2]    │    │  ◄── delivered first  (priority=9)
│  │ [medium-0]...[medium-4]     │    │  ◄── delivered second (priority=5)
│  │ [low-0][low-1]...[low-99]   │    │  ◄── delivered last   (priority=1)
│  └──────────────────────────────┘    │
│         │                            │
│         │  priority-ordered delivery │
└─────────┼────────────────────────────┘
          ▼
┌──────────────────┐
│   worker-amqp     │
│  (concurrency=1)  │
│                   │
│  processes:       │
│  high-0           │
│  high-1           │
│  high-2           │
│  medium-0         │
│  ...              │
│  low-99           │
└───────────────────┘
```

## What Happens

1. **RabbitMQ** starts with management plugin enabled
2. **producer-amqp** runs first, enqueues all 108 tasks in **low-first order**:
   - 100 low-priority tasks (priority=1)
   - 5 medium-priority tasks (priority=5)
   - 3 high-priority tasks (priority=9)
3. Producer exits — all tasks are now sitting in the queue
4. **worker-amqp** starts after producer completes, declares `priority` queue with `x-max-priority=9`
5. Despite low tasks being enqueued first, worker receives **high → medium → low**
6. Worker uses `concurrency=1` to clearly show the priority ordering in logs

## Key Difference vs Redis Priority

| | Redis (`priority` profile) | RabbitMQ (`amqp-priority` profile) |
|---|---|---|
| Mechanism | Separate queues (high/medium/low) | Single queue with `x-max-priority` |
| Ordering | Worker polls queues in order | Broker delivers by message priority |
| Granularity | Queue-level (coarse) | Message-level 0–9 (fine) |
| Configuration | Worker queue list order | `AmqpConfig::with_max_priority(9)` |

## Services

| Service | Image | Role |
|---------|-------|------|
| rabbitmq | rabbitmq:3-management-alpine | Message broker with priority queue support |
| worker-amqp | kojin-examples | Single-concurrency worker to show ordering |
| producer-amqp | kojin-examples | Enqueues 108 tasks in reverse priority order |

## Inspect

RabbitMQ management UI at http://localhost:15672 (guest/guest):
- **Queues** tab → `priority` queue shows `x-max-priority: 9`
- **Message rates** show consumption order
