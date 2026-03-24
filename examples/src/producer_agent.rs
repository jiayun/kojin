use kojin::{Broker, RedisBroker, RedisConfig, RunArgs, chain, claude_sig, group};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
    let scenario = std::env::var("SCENARIO").unwrap_or_else(|_| "single".into());

    let config = RedisConfig::new(&redis_url);
    let broker = RedisBroker::new(config).await?;

    let args = RunArgs::default().with_model("sonnet").with_max_turns(5);

    match scenario.as_str() {
        "single" => {
            tracing::info!("enqueuing single agent task");
            let sig = claude_sig("List all public functions in this project", args);
            let msg = sig.into_message();
            broker.enqueue(msg).await?;
        }

        "review" => {
            tracing::info!("enqueuing parallel code review (group of 3 agents)");
            let review = group![
                claude_sig(
                    "Review src/runner.rs for error handling issues",
                    args.clone()
                ),
                claude_sig("Review src/task.rs for API ergonomics", args.clone()),
                claude_sig(
                    "Review src/concurrency.rs for race conditions",
                    args.clone()
                ),
            ];

            // Group needs a result backend — use Redis
            let backend = kojin::RedisResultBackend::new(RedisConfig::new(&redis_url)).await?;
            let handle = review.apply(&broker, &backend).await?;
            tracing::info!(
                group_id = %handle.id,
                tasks = handle.task_ids.len(),
                "review group submitted"
            );
        }

        "pipeline" => {
            tracing::info!("enqueuing sequential pipeline (chain of 2 agents)");
            let pipeline = chain![
                claude_sig(
                    "Write unit tests for the add function in tasks.rs",
                    args.clone()
                ),
                claude_sig(
                    "Review the tests just written and suggest improvements",
                    args.clone()
                ),
            ];

            let backend = kojin::RedisResultBackend::new(RedisConfig::new(&redis_url)).await?;
            let handle = pipeline.apply(&broker, &backend).await?;
            tracing::info!(chain_id = %handle.id, "pipeline submitted");
        }

        other => {
            eprintln!("unknown scenario: {other}");
            eprintln!("available: single, review, pipeline");
            std::process::exit(1);
        }
    }

    tracing::info!("producer done");
    Ok(())
}
