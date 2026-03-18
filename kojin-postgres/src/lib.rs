//! PostgreSQL result backend for the kojin task queue.
//!
//! Provides [`PostgresResultBackend`] — a durable result backend using
//! PostgreSQL with `sqlx` for connection pooling and async queries.

mod result_backend;

pub use result_backend::PostgresResultBackend;
