//! RabbitMQ (AMQP) broker for the kojin task queue.
//!
//! This crate provides [`AmqpBroker`], a [`Broker`](kojin_core::Broker)
//! implementation backed by RabbitMQ using the AMQP 0.9.1 protocol.
//!
//! # Topology
//!
//! - `kojin.direct` exchange → `kojin.queue.{name}` (routing key = queue name)
//! - `kojin.dlx` exchange → `kojin.dlq.{name}` (dead-letter)
//! - `kojin.delayed` exchange for scheduled tasks (requires the
//!   `rabbitmq-delayed-message-exchange` plugin)
//!
//! # Example
//!
//! ```rust,no_run
//! use kojin_amqp::{AmqpBroker, AmqpConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = AmqpConfig::new("amqp://guest:guest@localhost:5672/%2f");
//! let queues = vec!["default".into(), "emails".into()];
//! let broker = AmqpBroker::new(config, &queues).await?;
//! # Ok(())
//! # }
//! ```

mod broker;
pub mod config;
pub mod topology;

pub use broker::AmqpBroker;
pub use config::AmqpConfig;
