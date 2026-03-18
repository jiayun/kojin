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
- **Middleware** — composable pre/post-execution hooks (tracing, metrics, custom)
- **Graceful shutdown** — cooperative cancellation via `CancellationToken`
- **Weighted queues** — prioritize work across multiple queues
- **Configurable retries** — per-task retry limits with backoff strategies

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
kojin = "0.2"
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

See `examples/workflow_demo.rs` for a complete runnable example.

## Cron Scheduling

With the `cron` feature flag, you can schedule periodic tasks using standard cron expressions:

```toml
[dependencies]
kojin = { version = "0.2", features = ["cron"] }
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

See `examples/cron_demo.rs` for a complete runnable example.

## Crate Architecture

| Crate | Description |
|-------|-------------|
| [`kojin`](https://crates.io/crates/kojin) | Facade crate — re-exports everything, provides `KojinBuilder` |
| [`kojin-core`](https://crates.io/crates/kojin-core) | Core traits (`Task`, `Broker`, `Middleware`), worker runtime, workflows, types |
| [`kojin-macros`](https://crates.io/crates/kojin-macros) | `#[kojin::task]` proc-macro |
| [`kojin-redis`](https://crates.io/crates/kojin-redis) | Redis broker + result backend via `deadpool-redis` |
| [`kojin-postgres`](https://crates.io/crates/kojin-postgres) | PostgreSQL result backend via `sqlx` |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
