# Agent: AI Agent Orchestration

Demonstrates distributed AI agent orchestration — enqueue Claude Code tasks to a Redis queue, process them on worker machines with concurrency control.

## Run

```bash
# Set your API key
export ANTHROPIC_API_KEY=sk-ant-...

# Single agent task
docker compose --profile agent up

# Or with a specific scenario
SCENARIO=review docker compose --profile agent up
SCENARIO=pipeline docker compose --profile agent up
```

## Architecture

```
┌───────────────────────────┐
│      producer-agent        │
│                           │
│  SCENARIO=single:         │
│    1 ClaudeCodeTask       │
│                           │
│  SCENARIO=review:         │
│    group! of 3 agents     │
│                           │
│  SCENARIO=pipeline:       │
│    chain! of 2 agents     │
└─────────────┬─────────────┘
              │ enqueue
              ▼
┌───────────────────────────┐
│           Redis            │
│                           │
│   queue:agents            │
│   [task][task][task]      │
└─────────────┬─────────────┘
              │ dequeue
              ▼
┌───────────────────────────────────────┐
│          worker-agent                  │
│                                       │
│  SemaphoreRunner(max_concurrent=3)    │
│  ┌─────────┐┌─────────┐┌─────────┐   │
│  │ claude  ││ claude  ││ claude  │   │
│  │ --bare  ││ --bare  ││ --bare  │   │
│  │ -p ...  ││ -p ...  ││ -p ...  │   │
│  └────┬────┘└────┬────┘└────┬────┘   │
│       │          │          │         │
│       ▼          ▼          ▼         │
│     RunOutput  RunOutput  RunOutput   │
└───────────────────────────────────────┘
```

## What Happens

1. **Redis** starts and passes healthcheck
2. **worker-agent** starts with `ANTHROPIC_API_KEY`, creates a `SemaphoreRunner(3)` wrapping `ProcessRunner`
3. **producer-agent** enqueues agent tasks based on `SCENARIO`:
   - `single`: 1 task — "List all public functions in this project"
   - `review`: group of 3 parallel code review tasks
   - `pipeline`: chain of write-tests → review-tests
4. Worker dequeues tasks, spawns `claude --bare -p <prompt> --output-format json`
5. `SemaphoreRunner` ensures at most 3 claude processes run simultaneously
6. Results (JSON with cost, duration, turns) are returned as `RunOutput`

## Scenarios

| Scenario | Workflow | Tasks | Pattern |
|----------|---------|-------|---------|
| `single` | Direct enqueue | 1 | Simple task → result |
| `review` | `group!` | 3 | Parallel fan-out (concurrent code reviews) |
| `pipeline` | `chain!` | 2 | Sequential (write → review) |

## Authentication

Two approaches (configure in `docker-compose.yml`):

### A. API Key (recommended for CI/scripts)

```bash
export ANTHROPIC_API_KEY=sk-ant-...
docker compose --profile agent up
```

The key is passed via `${ANTHROPIC_API_KEY}` in docker-compose environment.

### B. OAuth / Mounted Credentials

Mount your existing Claude auth into the container:

```yaml
# In docker-compose.yml, add to worker-agent:
volumes:
  - ${HOME}/.claude/.credentials.json:/root/.claude/.credentials.json:ro
```

## Claude CLI in Docker

The worker needs the `claude` CLI installed. Two approaches:

### Quick: Mount from host

```yaml
# Add to worker-agent volumes:
volumes:
  - /usr/local/bin/claude:/usr/local/bin/claude:ro
```

### Production: Install in Dockerfile

```dockerfile
# Add to the runtime stage:
RUN apt-get update && apt-get install -y nodejs npm \
    && npm install -g @anthropic-ai/claude-code \
    && apt-get remove -y nodejs npm && apt-get autoremove -y
```

## Environment Variables

### Worker (`kojin-worker-agent`)

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis broker URL |
| `ANTHROPIC_API_KEY` | *(unset)* | API key for Claude (auth option A) |
| `MAX_CONCURRENT_AGENTS` | `3` | Max simultaneous claude processes |
| `RUST_LOG` | `info` | Log level filter |

### Producer (`kojin-producer-agent`)

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | `redis://localhost:6379` | Redis broker URL |
| `SCENARIO` | `single` | Scenario: `single`, `review`, `pipeline` |
| `RUST_LOG` | `info` | Log level filter |

## Services

| Service | Image | Role |
|---------|-------|------|
| redis | redis:7-alpine | Message broker |
| worker-agent | kojin-examples | Agent worker with SemaphoreRunner |
| producer-agent | kojin-examples | Enqueues agent tasks, then exits |
