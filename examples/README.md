# Kojin Examples

Separate worker and producer binaries for running kojin in distributed Docker Compose scenarios.

## Crate Structure

```
examples/
├── src/
│   ├── tasks.rs      # Shared task definitions (AddTask, ProcessOrder, PriorityTask)
│   ├── worker.rs     # Binary: starts kojin worker with Redis broker
│   └── producer.rs   # Binary: enqueues tasks, then exits
```

## Quick Start (Docker Compose)

From the repository root:

```bash
# Basic fan-out: 20 AddTasks distributed across 3 competing workers
docker compose --profile basic up

# Chord workflow with PostgreSQL result backend
docker compose --profile chord up

# Priority tasks across weighted queues
docker compose --profile priority up

# Worker with monitoring dashboard on http://localhost:9090
docker compose --profile dashboard up

# Combine profiles
docker compose --profile basic --profile dashboard up

# Override worker count (default is 3)
docker compose --profile basic up --scale worker=5

# Tear down everything
docker compose down -v
```

## Profiles

| Profile | Services Started | What It Demonstrates |
|---------|-----------------|---------------------|
| *(none)* | redis, worker (x3) | 3 idle workers connected to Redis |
| `basic` | + producer-basic | Fan-out: 20 `AddTask` items across 3 competing workers |
| `chord` | + postgres, worker-pg, producer-chord | Chord workflow with Postgres result backend |
| `priority` | + producer-priority | Tasks across `high`, `medium`, `low` queues |
| `dashboard` | + worker-dashboard | Monitoring UI on port 9090 |

## Environment Variables

### Worker (`kojin-worker`)

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis broker connection URL |
| `POSTGRES_URL` | *(unset)* | PostgreSQL result backend URL (enables if set) |
| `DASHBOARD` | *(unset)* | Enable dashboard HTTP server (any value) |
| `DASHBOARD_PORT` | `9090` | Dashboard listen port |
| `RUST_LOG` | `info` | Log level filter (`tracing-subscriber` EnvFilter) |

### Producer (`kojin-producer`)

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis broker connection URL |
| `SCENARIO` | `basic` | Scenario to run: `basic`, `chord`, `priority` |
| `RUST_LOG` | `info` | Log level filter |

## Running Locally (without Docker)

Requires a running Redis instance on `localhost:6379`.

```bash
# Terminal 1: start the worker
cargo run -p kojin-examples --bin kojin-worker

# Terminal 2: enqueue tasks
SCENARIO=basic cargo run -p kojin-examples --bin kojin-producer

# With Postgres result backend (requires running PostgreSQL)
POSTGRES_URL=postgres://user:pass@localhost:5432/kojin cargo run -p kojin-examples --bin kojin-worker

# With dashboard
DASHBOARD=1 cargo run -p kojin-examples --bin kojin-worker
# Then visit http://localhost:9090
```

## Architecture

The worker/producer split mirrors real-world deployments where task producers (API servers, cron jobs) and task consumers (workers) are separate processes, potentially on different machines. Both share the same task definitions from `tasks.rs` to ensure correct routing via `Task::NAME`.
