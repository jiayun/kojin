# Dashboard: Monitoring UI

Demonstrates the built-in kojin dashboard — an HTTP API for monitoring queues, metrics, and task results in real time.

## Run

```bash
docker compose --profile dashboard up
```

Then open: http://localhost:9090

## Architecture

```
┌─────────┐       ┌──────────────────────────────┐
│  Redis  │◄──────│        worker-dashboard       │
│         │       │                                │
│  queues │       │  ┌────────────┐  ┌──────────┐ │
│         │       │  │   Worker   │  │Dashboard │ │
└─────────┘       │  │  (polling) │  │  :9090   │ │
                  │  └────────────┘  └─────┬────┘ │
                  └────────────────────────┼──────┘
                                           │
                            ┌──────────────┼──────────────┐
                            │              │              │
                            ▼              ▼              ▼
                      GET /api/       GET /api/      GET /api/
                       queues         metrics      tasks/{id}
                                                        │
                  ┌──────────────────┐                   │
                  │    Browser /     │◄──────────────────┘
                  │  curl / httpie   │   http://localhost:9090
                  └──────────────────┘
```

## What Happens

1. **Redis** starts and passes healthcheck
2. **worker-dashboard** starts with `DASHBOARD=1` and `DASHBOARD_PORT=9090`
3. The worker spawns both:
   - A **task worker** polling all queues
   - A **dashboard HTTP server** on port 9090
4. The dashboard exposes real-time metrics via REST API
5. **3 base workers** also start (polling the same queues)

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Web UI |
| `GET /healthz` | Health probe (for k8s readiness checks) |
| `GET /api/queues` | Queue sizes and pending task counts |
| `GET /api/queues/{name}` | Detail for a specific queue |
| `GET /api/queues/{name}/dlq` | Dead letter queue entries |
| `GET /api/metrics` | Throughput: started, succeeded, failed counts |
| `GET /api/tasks/{id}` | Task result (requires result backend) |

## Try It

```bash
# Queue overview
curl http://localhost:9090/api/queues

# Throughput metrics
curl http://localhost:9090/api/metrics

# Health check
curl http://localhost:9090/healthz
```

Combine with another profile to see live metrics:

```bash
docker compose --profile basic --profile dashboard up
# Then watch metrics update as tasks are processed
```

## Services

| Service | Image | Role |
|---------|-------|------|
| redis | redis:7-alpine | Message broker |
| worker (x3) | kojin-examples | Task consumers |
| worker-dashboard | kojin-examples | Worker + dashboard HTTP on :9090 |
