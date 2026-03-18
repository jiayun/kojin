//! Redis broker and result backend for the kojin task queue.
//!
//! Provides [`RedisBroker`] — a production-ready broker implementation using
//! Redis lists with `BRPOP` for reliable queue consumption and `deadpool-redis`
//! for connection pooling.
//!
//! Also provides [`RedisResultBackend`] for storing task results in Redis.

pub mod broker;
pub mod config;
pub mod keys;
pub mod result_backend;
pub mod scripts;

pub use broker::RedisBroker;
pub use config::RedisConfig;
pub use result_backend::RedisResultBackend;
