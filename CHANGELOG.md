# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/jiayun/kojin/releases/tag/v0.1.0
