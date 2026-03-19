use lapin::options::*;
use lapin::types::{AMQPValue, FieldTable, ShortString};
use lapin::{Channel, ExchangeKind};
use tracing::debug;

use crate::config::AmqpConfig;

/// Declare the core AMQP topology: exchanges, queues, and bindings.
pub async fn declare_topology(
    channel: &Channel,
    config: &AmqpConfig,
    queues: &[String],
) -> lapin::Result<()> {
    let opts = ExchangeDeclareOptions {
        durable: true,
        ..Default::default()
    };

    // Direct exchange for task routing
    channel
        .exchange_declare(
            ShortString::from(config.exchange.as_str()),
            ExchangeKind::Direct,
            opts,
            FieldTable::default(),
        )
        .await?;
    debug!(exchange = %config.exchange, "declared direct exchange");

    // Dead-letter exchange
    channel
        .exchange_declare(
            ShortString::from(config.dlx_exchange.as_str()),
            ExchangeKind::Direct,
            opts,
            FieldTable::default(),
        )
        .await?;
    debug!(exchange = %config.dlx_exchange, "declared DLX exchange");

    // Delayed message exchange (x-delayed-message plugin)
    let mut delayed_args = FieldTable::default();
    delayed_args.insert(
        "x-delayed-type".into(),
        AMQPValue::LongString("direct".into()),
    );
    channel
        .exchange_declare(
            ShortString::from(config.delayed_exchange.as_str()),
            ExchangeKind::Custom("x-delayed-message".into()),
            opts,
            delayed_args,
        )
        .await
        .or_else(|e| -> lapin::Result<()> {
            tracing::warn!(
                error = %e,
                "failed to declare delayed exchange — scheduled tasks require the \
                 rabbitmq-delayed-message-exchange plugin"
            );
            Ok(())
        })?;

    // Declare queues and DLQ counterparts
    for name in queues {
        declare_queue_pair(channel, config, name).await?;
    }

    Ok(())
}

/// Declare a queue and its dead-letter counterpart, plus bindings.
pub async fn declare_queue_pair(
    channel: &Channel,
    config: &AmqpConfig,
    name: &str,
) -> lapin::Result<()> {
    let queue_name: ShortString = format!("kojin.queue.{name}").into();
    let dlq_name: ShortString = format!("kojin.dlq.{name}").into();

    // DLQ first (no dead-letter routing itself)
    let dlq_opts = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };
    channel
        .queue_declare(dlq_name.clone(), dlq_opts, FieldTable::default())
        .await?;
    channel
        .queue_bind(
            dlq_name.clone(),
            ShortString::from(config.dlx_exchange.as_str()),
            ShortString::from(name),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Main queue with dead-letter routing
    let mut args = FieldTable::default();
    args.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(config.dlx_exchange.clone().into()),
    );
    args.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(name.into()),
    );

    let queue_opts = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };
    channel
        .queue_declare(queue_name.clone(), queue_opts, args)
        .await?;
    channel
        .queue_bind(
            queue_name.clone(),
            ShortString::from(config.exchange.as_str()),
            ShortString::from(name),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Bind to delayed exchange too
    channel
        .queue_bind(
            queue_name.clone(),
            ShortString::from(config.delayed_exchange.as_str()),
            ShortString::from(name),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .ok(); // ignore if delayed exchange doesn't exist

    debug!(queue = %queue_name, dlq = %dlq_name, "declared queue pair");
    Ok(())
}
