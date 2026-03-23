use kojin::{
    Broker, PostgresResultBackend, RedisBroker, RedisConfig, Signature, TaskMessage, chord,
};
use kojin_examples::{AddTask, PriorityTask};
use tracing_subscriber::EnvFilter;

fn add_sig(a: i32, b: i32) -> Signature {
    Signature::new("add", "default", serde_json::json!({"a": a, "b": b}))
}

fn order_sig(id: &str, delay_ms: u64) -> Signature {
    Signature::new(
        "process_order",
        "orders",
        serde_json::json!({"order_id": id, "delay_ms": delay_ms}),
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
    let scenario = std::env::var("SCENARIO").unwrap_or_else(|_| "basic".into());

    let config = RedisConfig::new(&redis_url);
    let broker = RedisBroker::new(config).await?;

    match scenario.as_str() {
        "basic" => {
            tracing::info!("enqueuing 20 AddTasks");
            for i in 0..20 {
                let msg = TaskMessage::new(
                    "add",
                    "default",
                    serde_json::to_value(&AddTask { a: i, b: i * 2 }).unwrap(),
                );
                broker.enqueue(msg).await?;
                tracing::info!("enqueued add({}, {})", i, i * 2);
            }
        }

        "chord" => {
            tracing::info!("enqueuing chord: 5 ProcessOrders -> AddTask callback");
            let tasks: Vec<Signature> = (1..=5)
                .map(|i| order_sig(&format!("ORD-{i:03}"), 500))
                .collect();
            let callback = add_sig(100, 200);
            let workflow = chord(tasks, callback);

            // chord.apply needs a result backend — use Postgres to match the worker
            let pg_url =
                std::env::var("POSTGRES_URL").expect("POSTGRES_URL required for chord scenario");
            let backend = PostgresResultBackend::connect(&pg_url).await?;
            backend.migrate().await?;
            let handle = workflow.apply(&broker, &backend).await?;
            tracing::info!("chord submitted: {}", handle.id);
        }

        "priority" => {
            tracing::info!("enqueuing priority tasks across queues");
            let queues = [("high", 3), ("medium", 5), ("low", 2)];
            for (priority, count) in queues {
                for i in 0..count {
                    let msg = TaskMessage::new(
                        "priority_task",
                        priority,
                        serde_json::to_value(&PriorityTask {
                            label: format!("task-{i}"),
                            priority: priority.into(),
                        })
                        .unwrap(),
                    );
                    broker.enqueue(msg).await?;
                    tracing::info!("enqueued [{priority}] task-{i}");
                }
            }
        }

        other => {
            eprintln!("unknown scenario: {other}");
            eprintln!("available: basic, chord, priority");
            std::process::exit(1);
        }
    }

    tracing::info!("producer done");
    Ok(())
}
