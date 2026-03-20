# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-03-20

### Added

- New crate: **`kojin-sqs`** — AWS SQS broker with standard & FIFO queue support, long polling, delayed scheduling
- `DeduplicationMiddleware` — content-based and key-based task dedup with configurable TTL (behind `dedup` feature)
- AMQP priority queues — per-message priority (0–9) via `x-max-priority` queue argument
- `AmqpConfig::with_max_priority()` builder method
- `TaskMessage::with_priority()` and `TaskMessage::with_dedup_key()` / `with_content_dedup()` methods
- `SqsConfig` builder with configurable wait time, max messages, and queue URLs
- Dashboard embedded HTML UI (`/ui` endpoint)
- `docs/message-schema.json` — JSON Schema for cross-language wire format
- `sqs` and `dedup` feature flags
- Examples: `sqs`

## [0.3.0] - 2026-03-19

### Added

- New crate: **`kojin-amqp`** — RabbitMQ broker via `lapin` with automatic topology declaration (direct exchange, dead-letter queues, delayed-message exchange)
- New crate: **`kojin-dashboard`** — JSON API monitoring dashboard via `axum` with endpoints for queues, metrics, DLQ inspection, and task results
- `OtelMiddleware` — OpenTelemetry integration emitting `kojin.task.*` counters and duration histogram (behind `otel` feature)
- `RateLimitMiddleware` — token-bucket rate limiting via `governor`, supports global and per-task limits (behind `rate-limit` feature)
- `AmqpConfig` builder with configurable exchange names and prefetch count
- `DashboardState` builder with optional result backend and metrics
- `spawn_dashboard()` for running the dashboard as a background Tokio task
- `amqp`, `dashboard`, `otel`, and `rate-limit` feature flags
- Examples: `observability`, `dashboard`, `amqp`

### Fixed

- AMQP topology: use `assert` instead of `declare` for delayed exchange to avoid hard errors when the `rabbitmq_delayed_message_exchange` plugin is not installed

## [0.2.0] - 2026-03-18

### Added

- **Workflow orchestration** — `Canvas` enum with `Chain`, `Group`, and `Chord` variants
- `Signature` — type-erased task invocation descriptor with `|` operator for chaining
- `chain![]`, `group![]` declarative macros and `chord()` function
- `Canvas::apply()` execution engine with automatic chain continuation and group completion
- `MemoryResultBackend` — in-memory result backend for testing
- `RedisResultBackend` — Redis result backend with atomic Lua group completion
- `PostgresResultBackend` — new `kojin-postgres` crate with `sqlx`, auto-migration
- `ResultBackend` group methods: `init_group`, `complete_group_member`, `get_group_results`
- `TaskMessage` workflow metadata: `parent_id`, `correlation_id`, `group_id`, `group_total`, `chord_callback`
- `Task::signature()` default method for building `Signature` from task instances
- Worker chain continuation via `kojin.chain_next` header
- Worker group/chord completion with automatic callback enqueuing
- Cron scheduling: `CronEntry`, `CronRegistry`, `scheduler_loop()` (behind `cron` feature)
- `KojinBuilder::result_backend()` and `KojinBuilder::cron()` methods
- `postgres` and `cron` feature flags
- `workflows` and `cron` examples

### Changed

- `Worker` now accepts `Option<Arc<dyn ResultBackend>>` for result storage
- `ResultBackend` trait extended with group methods (default impls, non-breaking)

## [0.1.0] - 2026-03-18

### Added

- `Task` trait with associated `NAME`, `QUEUE`, `MAX_RETRIES`, and `Output` type
- `Broker` trait with `enqueue` / `dequeue` / `acknowledge` / `reject` lifecycle
- `MemoryBroker` — in-process broker for testing and development
- `RedisBroker` — production Redis broker using `BRPOP` and `deadpool-redis`
- `#[kojin::task]` proc-macro to generate task structs from async functions
- `Worker` runtime with configurable concurrency and queue selection
- `Middleware` trait with `TracingMiddleware` and `MetricsMiddleware`
- `KojinBuilder` — ergonomic fluent API for constructing workers
- `WeightedQueue` — priority-based queue selection
- `BackoffStrategy` — configurable retry backoff (constant, exponential, linear)
- `TaskContext` with cancellation token and task metadata
- `JsonCodec` — default serde-based message serialization
- Graceful shutdown via `CancellationToken`

[0.4.0]: https://github.com/jiayun/kojin/releases/tag/v0.4.0
[0.3.0]: https://github.com/jiayun/kojin/releases/tag/v0.3.0
[0.2.0]: https://github.com/jiayun/kojin/releases/tag/v0.2.0
[0.1.0]: https://github.com/jiayun/kojin/releases/tag/v0.1.0
