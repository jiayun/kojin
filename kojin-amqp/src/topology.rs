use lapin::options::*;
use lapin::types::{AMQPValue, FieldTable, ShortString};
use lapin::{Channel, Connection, ExchangeKind};
use tracing::debug;

use crate::config::AmqpConfig;

/// Declare the core AMQP topology: exchanges, queues, and bindings.
pub async fn declare_topology(
    channel: &Channel,
    config: &AmqpConfig,
    queues: &[String],
    connection: &Connection,
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

    // Delayed message exchange (x-delayed-message plugin).
    // The failure is a hard AMQP error (COMMAND_INVALID) that kills the
    // entire connection, so we probe on a disposable connection first.
    {
        let probe = lapin::Connection::connect(
            &config.url,
            lapin::ConnectionProperties::default(),
        )
        .await;
        if let Ok(probe_conn) = probe {
            let probe_ch = probe_conn.create_channel().await.ok();
            if let Some(ch) = probe_ch {
                let mut delayed_args = FieldTable::default();
                delayed_args.insert(
                    "x-delayed-type".into(),
                    AMQPValue::LongString("direct".into()),
                );
                if let Err(e) = ch
                    .exchange_declare(
                        ShortString::from(config.delayed_exchange.as_str()),
                        ExchangeKind::Custom("x-delayed-message".into()),
                        opts,
                        delayed_args,
                    )
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        "failed to declare delayed exchange — scheduled tasks require the \
                         rabbitmq-delayed-message-exchange plugin"
                    );
                }
            }
            // probe connection is dropped here
        }
    }

    // Declare queues and DLQ counterparts
    for name in queues {
        declare_queue_pair(channel, config, name, connection).await?;
    }

    Ok(())
}

/// Declare a queue and its dead-letter counterpart, plus bindings.
pub async fn declare_queue_pair(
    channel: &Channel,
    config: &AmqpConfig,
    name: &str,
    connection: &Connection,
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

    // Bind to delayed exchange too — use a disposable channel because binding
    // to a non-existent exchange poisons the AMQP channel.
    let delayed_ch = connection.create_channel().await?;
    let _ = delayed_ch
        .queue_bind(
            queue_name.clone(),
            ShortString::from(config.delayed_exchange.as_str()),
            ShortString::from(name),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await; // ignore if delayed exchange doesn't exist

    debug!(queue = %queue_name, dlq = %dlq_name, "declared queue pair");
    Ok(())
}
