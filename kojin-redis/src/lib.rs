//! Redis broker backend for the kojin task queue.
//!
//! Provides [`RedisBroker`] — a production-ready broker implementation using
//! Redis lists with `BRPOP` for reliable queue consumption and `deadpool-redis`
//! for connection pooling.

pub mod broker;
pub mod config;
pub mod keys;
pub mod scripts;

pub use broker::RedisBroker;
pub use config::RedisConfig;
