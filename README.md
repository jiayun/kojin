# kojin

[![Crates.io](https://img.shields.io/crates/v/kojin.svg)](https://crates.io/crates/kojin)
[![docs.rs](https://img.shields.io/docsrs/kojin)](https://docs.rs/kojin)
[![License](https://img.shields.io/crates/l/kojin.svg)](LICENSE-MIT)

Async distributed task queue for Rust — the equivalent of [Celery](https://docs.celeryq.dev/) / [Dramatiq](https://dramatiq.io/) (Python), [BullMQ](https://bullmq.io/) (Node.js), [Sidekiq](https://sidekiq.org/) (Ruby), and [Machinery](https://github.com/RichardKnop/machinery) (Go).

## Features

- **Async-first** — built on Tokio, designed for `async`/`await` from the ground up
- **`#[kojin::task]`** — proc-macro to define tasks from plain async functions
- **Pluggable broker** — trait-based broker abstraction (Redis included, bring your own)
- **Workflows** — chain, group, chord orchestration with `chain![]`, `group![]` macros
- **Result backends** — Memory, Redis, PostgreSQL for storing task results and coordinating workflows
- **Cron scheduling** — periodic task execution with standard cron expressions
- **Middleware** — composable pre/post-execution hooks (tracing, metrics, rate limiting, OpenTelemetry)
- **AMQP broker** — RabbitMQ support with automatic topology, dead-letter queues, and delayed scheduling
- **SQS broker** — Amazon SQS support with standard and FIFO queues, long polling, and delayed scheduling
- **Deduplication** — content-based or key-based dedup middleware with configurable TTL
- **Priority queues** — per-message priority (0–9) via AMQP broker
- **Dashboard** — built-in JSON API for monitoring queues, metrics, and task results
- **Graceful shutdown** — cooperative cancellation via `CancellationToken`
- **Weighted queues** — prioritize work across multiple queues
- **Configurable retries** — per-task retry limits with backoff strategies

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
kojin = "0.4"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
async-trait = "0.1"
```

Define a task, enqueue it, and run a worker:

```rust
use async_trait::async_trait;
use kojin::{Broker, KojinBuilder, MemoryBroker, Task, TaskContext, TaskMessage, TaskResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SendEmail {
    to: String,
    subject: String,
}

#[async_trait]
impl Task for SendEmail {
    const NAME: &'static str = "send_email";
    const QUEUE: &'static str = "emails";
    const MAX_RETRIES: u32 = 3;

    type Output = String;

    async fn run(&self, _ctx: &TaskContext) -> TaskResult<Self::Output> {
        println!("Sending email to {}", self.to);
        Ok(format!("Email sent to {}", self.to))
    }
}

#[tokio::main]
async fn main() {
    let broker = MemoryBroker::new();

    // Enqueue a task
    let msg = TaskMessage::new(
        "send_email",
        "emails",
        serde_json::to_value(&SendEmail {
            to: "user@example.com".into(),
            subject: "Hello!".into(),
        }).unwrap(),
    );
    broker.enqueue(msg).await.unwrap();

    // Build and run worker
    let worker = KojinBuilder::new(broker)
        .register_task::<SendEmail>()
        .concurrency(4)
        .queues(vec!["emails".into()])
        .build();

    worker.run().await;
}
```

## Workflows

Kojin supports Celery-style workflow primitives — `chain!`, `group!`, and `chord` — for composing tasks into DAGs. A result backend is required.

```rust
use kojin::{chain, group, chord, Signature, Canvas, MemoryResultBackend};

// Signatures describe a task invocation (name, queue, payload)
let fetch = Signature::new("fetch_url", "default", serde_json::json!({"url": "..."}));
let parse = Signature::new("parse_html", "default", serde_json::json!(null));
let store = Signature::new("store_result", "default", serde_json::json!(null));

// Chain — sequential: fetch → parse → store
let pipeline = chain![fetch.clone(), parse.clone(), store.clone()];

// Group — parallel: fetch three URLs concurrently
let batch = group![fetch.clone(), fetch.clone(), fetch.clone()];

// Chord — parallel + callback: fetch all, then aggregate
let aggregate = Signature::new("aggregate", "default", serde_json::json!(null));
let workflow = chord(vec![fetch.clone(), fetch.clone()], aggregate);

// Submit to the broker
let handle = pipeline.apply(&broker, &backend).await?;
```

See `kojin/examples/workflows.rs` for a complete runnable example.

## Cron Scheduling

With the `cron` feature flag, you can schedule periodic tasks using standard cron expressions:

```toml
[dependencies]
kojin = { version = "0.4", features = ["cron"] }
```

```rust
use kojin::{KojinBuilder, Signature, MemoryBroker};

let worker = KojinBuilder::new(MemoryBroker::new())
    .register_task::<CleanupTask>()
    .result_backend(backend)
    .cron(
        "nightly-cleanup",
        "0 3 * * *",  // every day at 03:00
        Signature::new("cleanup", "default", serde_json::json!(null)),
    )
    .build();

worker.run().await;
```

See `kojin/examples/cron.rs` for a complete runnable example.

## Middleware & Observability

Kojin ships with composable middleware for tracing, metrics, rate limiting, and OpenTelemetry:

```rust
use std::num::NonZeroU32;
use kojin::{KojinBuilder, MemoryBroker, MetricsMiddleware, TracingMiddleware, RateLimitMiddleware};

let metrics = MetricsMiddleware::new();

let worker = KojinBuilder::new(MemoryBroker::new())
    .register_task::<MyTask>()
    .middleware(TracingMiddleware)                                 // structured logs
    .middleware(metrics.clone())                                   // in-process counters
    .middleware(RateLimitMiddleware::per_second(NonZeroU32::new(100).unwrap())) // token-bucket
    .build();

// After processing, query counters:
println!("succeeded: {}", metrics.tasks_succeeded());
```

`OtelMiddleware` (behind the `otel` feature) emits `kojin.task.started`, `kojin.task.succeeded`, `kojin.task.failed` counters and a `kojin.task.duration` histogram to any configured OpenTelemetry `MeterProvider`.

See `kojin/examples/observability.rs` for a complete runnable example.

### Deduplication

With the `dedup` feature flag, you can prevent duplicate task execution using content-based or key-based deduplication:

```toml
[dependencies]
kojin = { version = "0.4", features = ["dedup"] }
```

```rust
use std::time::Duration;
use kojin::{DeduplicationMiddleware, KojinBuilder, MemoryBroker, TaskMessage};

// Add dedup middleware with a 5-minute TTL
let dedup = DeduplicationMiddleware::new(Duration::from_secs(300));

let worker = KojinBuilder::new(MemoryBroker::new())
    .register_task::<MyTask>()
    .middleware(dedup)
    .build();

// Key-based dedup — tasks with the same key within TTL are rejected
let msg = TaskMessage::new("my_task", "default", payload)
    .with_dedup_key("order-123");

// Content-based dedup — auto-generates key from task name + payload hash
let msg = TaskMessage::new("my_task", "default", payload)
    .with_content_dedup();
```

## AMQP Broker (RabbitMQ)

With the `amqp` feature flag, you can use RabbitMQ as a production broker:

```toml
[dependencies]
kojin = { version = "0.4", features = ["amqp"] }
```

```rust
use kojin::{AmqpBroker, AmqpConfig, KojinBuilder};

let config = AmqpConfig::new("amqp://guest:guest@localhost:5672/%2f");
let broker = AmqpBroker::new(config, &["default".into()]).await?;

let worker = KojinBuilder::new(broker)
    .register_task::<MyTask>()
    .build();
```

`AmqpBroker` automatically declares the full topology: a direct exchange (`kojin.direct`), per-queue dead-letter queues (`kojin.dlq.*`), and a delayed-message exchange (`kojin.delayed`) for scheduled tasks.

### Priority Queues

Enable per-message priority (0–9, higher = more urgent) by setting `max_priority` on the config:

```rust
let config = AmqpConfig::new("amqp://guest:guest@localhost:5672/%2f")
    .with_max_priority(10);

let broker = AmqpBroker::new(config, &["orders".into()]).await?;

// Enqueue a high-priority task
let msg = TaskMessage::new("process_order", "orders", payload)
    .with_priority(9);
broker.enqueue(msg).await?;
```

> **Note:** Changing `max_priority` on an existing queue requires deleting and recreating the queue in RabbitMQ, as `x-max-priority` is an immutable queue argument.

See `kojin/examples/amqp.rs` for a complete runnable example.

## SQS Broker (Amazon SQS)

With the `sqs` feature flag, you can use Amazon SQS as a broker:

```toml
[dependencies]
kojin = { version = "0.4", features = ["sqs"] }
```

```rust
use kojin::{SqsBroker, SqsConfig, KojinBuilder};

let sdk_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
let config = SqsConfig::new(vec!["https://sqs.us-east-1.amazonaws.com/123456789/my-queue".into()]);
let broker = SqsBroker::new(&sdk_config, config);

let worker = KojinBuilder::new(broker)
    .register_task::<MyTask>()
    .build();
```

Feature notes:
- **FIFO queues** — automatically detected from `.fifo` suffix; uses `MessageGroupId` and `MessageDeduplicationId`
- **Delayed scheduling** — uses SQS `DelaySeconds` (max 15 minutes); for longer delays, re-enqueues periodically
- **Long polling** — configurable `wait_time_seconds` (default 20s) for efficient message retrieval
- **No priority support** — SQS does not support message priority; use AMQP if you need priority queues

See `kojin/examples/sqs.rs` for a complete runnable example.

## Dashboard

With the `dashboard` feature flag, you get a built-in JSON API for monitoring:

```toml
[dependencies]
kojin = { version = "0.4", features = ["dashboard"] }
```

```rust
use std::sync::Arc;
use kojin::{DashboardState, MetricsMiddleware, MemoryBroker, spawn_dashboard};

let broker = MemoryBroker::new();
let metrics = MetricsMiddleware::new();

let state = DashboardState::new(Arc::new(broker.clone()))
    .with_metrics(metrics.clone());

let _handle = spawn_dashboard(state, 9090);
// GET /api/queues        — list all queues with lengths
// GET /api/queues/{name} — single queue detail
// GET /api/metrics       — tasks started/succeeded/failed
// GET /api/tasks/{id}    — task result (requires result backend)
```

See `kojin/examples/dashboard.rs` for a complete runnable example.

## Docker Compose Examples

Run distributed scenarios with a single command — see [`examples/README.md`](examples/README.md) for full details.

| Profile | Command | Architecture |
|---------|---------|-------------|
| Basic fan-out | `docker compose --profile basic up` | [docs](examples/docs/basic.md) |
| Chord + PostgreSQL | `docker compose --profile chord up` | [docs](examples/docs/chord.md) |
| Weighted queues | `docker compose --profile priority up` | [docs](examples/docs/priority.md) |
| RabbitMQ priority | `docker compose --profile amqp-priority up` | [docs](examples/docs/amqp-priority.md) |
| Dashboard | `docker compose --profile dashboard up` | [docs](examples/docs/dashboard.md) |

## Crate Architecture

| Crate | Description |
|-------|-------------|
| [`kojin`](https://crates.io/crates/kojin) | Facade crate — re-exports everything, provides `KojinBuilder` |
| [`kojin-core`](https://crates.io/crates/kojin-core) | Core traits (`Task`, `Broker`, `Middleware`), worker runtime, workflows, types |
| [`kojin-macros`](https://crates.io/crates/kojin-macros) | `#[kojin::task]` proc-macro |
| [`kojin-redis`](https://crates.io/crates/kojin-redis) | Redis broker + result backend via `deadpool-redis` |
| [`kojin-postgres`](https://crates.io/crates/kojin-postgres) | PostgreSQL result backend via `sqlx` |
| [`kojin-amqp`](https://crates.io/crates/kojin-amqp) | RabbitMQ broker via `lapin` — topology, DLQ, delayed messages |
| [`kojin-sqs`](https://crates.io/crates/kojin-sqs) | Amazon SQS broker — standard & FIFO queues, long polling |
| [`kojin-dashboard`](https://crates.io/crates/kojin-dashboard) | JSON API monitoring dashboard via `axum` |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
