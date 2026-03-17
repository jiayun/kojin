# Kojin (工人) — Rust Production-Grade Task Queue Library

## Context

Task queues are the #1 identified gap in the Rust ecosystem (confirmed by both PyPI vs crates.io and awesome-list analyses). Existing solutions — apalis (workflow features beta), rusty-celery (maintenance unclear, AMQP only), fang (DB-backed, simple) — none match Celery/BullMQ's completeness. This is a high-demand, moderate-difficulty opportunity with clear commercial potential.

**Goal**: Build the Celery/BullMQ equivalent for Rust — async-first, type-safe, production-grade from day one.

---

## Workspace Structure

```
kojin/
├── Cargo.toml              # workspace root
├── kojin/                   # facade crate (re-exports with feature flags)
├── kojin-core/              # traits, types, state machine, errors
├── kojin-macros/            # #[kojin::task] proc-macro
├── kojin-redis/             # Redis broker + result backend
├── examples/
│   ├── basic.rs
│   └── workflows.rs
```

Later phases add: `kojin-postgres/`, `kojin-amqp/`, `kojin-dashboard/`

---

## Core Design

### Task Definition — Dual API

```rust
// Simple: proc-macro (80% use case)
#[kojin::task(queue = "emails", max_retries = 3)]
async fn send_email(ctx: &TaskContext, to: String, subject: String) -> TaskResult<()> {
    let mailer = ctx.data::<SmtpMailer>()?;
    mailer.send(&to, &subject).await?;
    Ok(())
}

// Power: manual trait impl
impl Task for ProcessImage {
    const NAME: &'static str = "process_image";
    const QUEUE: &'static str = "images";
    type Output = ImageResult;
    async fn run(&self, ctx: &TaskContext) -> Result<Self::Output, Self::Error> { ... }
}
```

### Key Traits

- **`Broker`** — transport only: `enqueue`, `dequeue`, `ack`, `nack`, `dead_letter`, `schedule`, `queue_len`
- **`ResultBackend`** — separate from broker: `store`, `get`, `wait`, `delete`
- **`Middleware`** — `before`/`after`/`around` pattern (simpler than Tower, purpose-built for tasks)
- **`Codec`** — pluggable serialization (JSON default, MessagePack option)

### Key Decisions

| Decision | Choice | Why |
|---|---|---|
| Workspace vs mono | Workspace | Isolate broker deps, follow tokio/axum pattern |
| Concurrency | tokio::spawn + Semaphore | Simple, leverages work-stealing |
| Redis dequeue | BRPOPLPUSH | Atomic, reliable (reaper recovers crashed tasks) |
| Task ID | UUID v7 | Time-ordered, globally unique |
| Result backend | Separate from broker | Different durability needs (Redis broker + Postgres results) |
| Middleware | Custom trait, not Tower | Simpler for task queue use case |

---

## Phased Implementation

### Phase 1: Core Foundation

1. **kojin-core**: Task/Broker/ResultBackend/Middleware traits, TaskMessage, TaskState enum, BackoffStrategy, error types
2. **kojin-macros**: `#[kojin::task]` proc-macro → generates struct + Task impl
3. **kojin-redis**: Redis broker (BRPOPLPUSH, sorted set for scheduled, list for DLQ), connection pooling via deadpool-redis
4. **Worker**: concurrency control, task dispatch, graceful shutdown, weighted queue consumption
5. **Built-in middleware**: RetryMiddleware, TimeoutMiddleware, TracingMiddleware, MetricsMiddleware
6. **kojin facade**: feature-flag re-exports
7. **Tests**: MemoryBroker for unit tests, testcontainers for Redis integration tests, trybuild for macro tests

### Phase 2: Workflows & Scheduling

1. **Canvas primitives**: `chain![]`, `group![]`, `chord()`, pipe `|` operator
2. **Result backend**: Redis impl + PostgreSQL impl
3. **Cron scheduling**: embedded scheduler using `cron` crate + Redis sorted set
4. **Workflow tracking**: parent_id/correlation_id in metadata, counter-based group completion

### Phase 3: Observability & More Brokers

1. **OpenTelemetry middleware** (traces + metrics)
2. **kojin-dashboard**: Axum web UI (queue depths, throughput, DLQ viewer, task search)
3. **kojin-amqp**: RabbitMQ broker via `lapin`
4. **Rate limiting middleware** (token bucket)

### Phase 4: Advanced

1. **kojin-sqs**: AWS SQS broker
2. **Task deduplication / idempotency**
3. **Kubernetes-native patterns**
4. **Cross-language interop** — language-agnostic JSON message schema over shared broker, enabling mixed Rust/Python (Celery, Dramatiq) worker pools; optionally Celery protocol v2 wire format for direct interop

---

## Worker Autoscaling & Cloud-Native Deployment

A common pain point with Redis-backed task queues (Celery, Dramatiq, BullMQ) is the lack of native autoscaling signals — dedicated worker instances for batch/low-priority queues run 24/7, wasting resources during idle time. Kojin addresses this at multiple levels.

### Weighted Queue Consumption (Phase 1)

Workers consume multiple queues with configurable priority weights, eliminating the need for dedicated batch instances:

```rust
app.run_worker_with(WorkerConfig::builder()
    .queue("critical", QueueWeight::High)   // prioritized
    .queue("batch", QueueWeight::Low)        // fills idle time
    .concurrency(16)
    .build())
    .await?;
```

A single worker pool handles both critical and batch tasks — batch work is automatically consumed during idle periods without dedicated infrastructure.

### Queue Metrics for Autoscaling (Phase 1)

`MetricsMiddleware` exports queue depth and worker utilization to external metric systems (CloudWatch, Prometheus), enabling container orchestrators to autoscale — including scale-to-zero:

```rust
Kojin::builder()
    .middleware(MetricsMiddleware::cloudwatch("kojin/queue_depth"))
    .build();
```

Example ECS autoscaling policy: scale up when queue depth > 100, scale to zero after 5 minutes at depth 0.

### Graceful Shutdown / SIGTERM Drain (Phase 1)

Critical for autoscaling — workers must handle scale-down without losing tasks:

```rust
app.run_worker_with(WorkerConfig::builder()
    .shutdown_timeout(Duration::from_secs(120)) // match ECS stopTimeout
    .build())
    .await?;
```

On SIGTERM: (1) stop dequeuing new tasks, (2) wait for in-flight tasks to complete within timeout, (3) nack unfinished tasks back to the queue for redelivery.

### Native SQS Autoscaling (Phase 4)

The `kojin-sqs` broker provides zero-config autoscaling on AWS — ECS natively scales on `ApproximateNumberOfMessages` without any custom metrics infrastructure.

---

## Differentiation vs apalis

1. Canvas workflow primitives as first-class (not beta)
2. DLQ as core concept
3. Proc-macro for ergonomic task definition
4. Result backend separated from broker
5. Simpler middleware (not Tower)
6. Production-grade defaults (visibility timeout, reaper, graceful shutdown)
7. Built-in autoscaling support (queue metrics export, weighted queues, graceful SIGTERM drain)

---

## Dependencies (minimal)

**kojin-core**: serde, serde_json, uuid (v7), chrono, thiserror, async-trait, tracing
**kojin-macros**: syn, quote, proc-macro2
**kojin-redis**: redis (tokio), deadpool-redis

~12 direct deps total for core + Redis.

---

## Verification

1. `cargo build --workspace` — compiles
2. `cargo test` — unit tests with MemoryBroker
3. `cargo test --features integration-tests` — Redis integration via testcontainers
4. `cargo clippy && cargo fmt --check`
5. Run `examples/basic.rs` — enqueue + process tasks end-to-end
6. Benchmark: `criterion` throughput test (enqueue/dequeue msg/s)
