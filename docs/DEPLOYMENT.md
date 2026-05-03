# Deployment

AgentFlow currently has two runnable binaries:

- `agentflow-server`: long-running Axum gateway service. This is the primary container and Helm target.
- `agentflow`: CLI workflow, skill, MCP, trace, and configuration utility. It can be built from the same Dockerfile by changing build args, but it is not the default Kubernetes workload.

## Docker Image

Build the server image:

```bash
docker build -t agentflow:server .
```

Build the CLI image:

```bash
docker build \
  --build-arg PACKAGE=agentflow-cli \
  --build-arg BIN=agentflow \
  -t agentflow:cli .
```

The image runs as a non-root user and exposes port `3000` for `agentflow-server`.

## Docker Compose

The included Compose file starts PostgreSQL and the gateway:

```bash
docker compose up --build
curl http://localhost:3000/health
curl http://localhost:3000/health/live
curl http://localhost:3000/health/ready
```

Compose sets:

- `DATABASE_URL=postgres://agentflow:agentflow@postgres:5432/agentflow`
- `PORT=3000`
- `RUST_LOG=info`

## Helm

Install with an existing PostgreSQL connection secret:

```bash
kubectl create secret generic agentflow-db \
  --from-literal=DATABASE_URL='postgres://user:password@postgres:5432/agentflow'

helm install agentflow charts/agentflow \
  --set image.repository=agentflow \
  --set image.tag=server \
  --set existingSecret=agentflow-db
```

For local development only, Helm can create the secret from values:

```bash
helm install agentflow charts/agentflow \
  --set image.repository=agentflow \
  --set image.tag=server \
  --set secretEnv.DATABASE_URL='postgres://user:password@postgres:5432/agentflow'
```

Prefer `existingSecret` in shared environments so credentials do not live in Helm release values.

## Health Checks

`agentflow-server` exposes:

- `/health`: basic service health.
- `/health/live`: liveness probe.
- `/health/ready`: readiness probe.

The Helm chart wires liveness and readiness probes to those endpoints. The current readiness endpoint confirms the process is serving HTTP; startup still fails if `DATABASE_URL` cannot be connected.

## Volumes And Secrets

- The server requires `DATABASE_URL`.
- LLM provider keys and tool credentials should be provided through Kubernetes Secrets or external secret injection, not image layers.
- CLI containers that need `~/.agentflow` can mount it as a volume at `/home/agentflow/.agentflow`.
- Trace files should be backed by a persistent volume only when using file-backed trace storage.

## v0.3.0 N8: Control-plane HTTP surface

The gateway applies its `agentflow-db` migrations on startup
(`connect_and_migrate`). Six tables back the platform: `runs`, `steps`,
`events`, `artifacts`, `skill_installs`, `mcp_sessions`. To verify the
schema is up:

```bash
docker compose up -d postgres agentflow-server
docker compose exec postgres psql -U agentflow -d agentflow \
  -c "\dt"
```

### Authentication

Every `/v1/*` route requires `Authorization: Bearer <token>` when
`AGENTFLOW_API_TOKEN` is set. With the env var unset the server runs open
(useful for local dev — startup logs a warning).

```bash
export AGENTFLOW_API_TOKEN="dev-secret"
curl -H "Authorization: Bearer dev-secret" http://localhost:3000/v1/whoami
```

Health probes (`/health`, `/health/live`, `/health/ready`) bypass auth so
load balancers / kubelet probes work without secrets.

### Submit and inspect a run

```bash
# Submit a workflow body. Returns { "run_id": "...", "status": "queued" }.
RUN=$(curl -sX POST http://localhost:3000/v1/runs \
  -H "Authorization: Bearer dev-secret" \
  -H "Content-Type: application/json" \
  -d '{"workflow": "name: demo\nnodes: []"}')
RUN_ID=$(echo "$RUN" | jq -r .run_id)

# Poll for state.
curl -s -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID | jq .

# Subscribe to live events (Server-Sent Events). Press Ctrl-C to detach.
curl -N -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID/events
```

To resume a stream after a network blip, pass the last seq the client saw:

```bash
curl -N -H "Authorization: Bearer dev-secret" \
  "http://localhost:3000/v1/runs/$RUN_ID/events?after_seq=12"
```

### Skills

Mount a `skills.index.toml` and point `AGENTFLOW_SKILLS_INDEX` at it. Then:

```bash
curl -s -H "Authorization: Bearer dev-secret" http://localhost:3000/v1/skills | jq .
curl -sX POST -H "Authorization: Bearer dev-secret" \
  -H "Content-Type: application/json" \
  -d '{"input": "summarise this paragraph: ..."}' \
  http://localhost:3000/v1/skills/summariser:run | jq .
```

Skill invocation creates a `runs` row with `workflow = "@skill:<name>"` and
dispatches through the same executor used by `/v1/runs`. The actual skill
agent invocation lives in the executor — v0.3.0 N8 ships a stub that
flips the run to `succeeded` after a brief delay; v0.4.0 wires the real
runtime via `WorkflowEventListener`.

### Unified error envelope

Every error response is shaped `{ "error": { "code", "message", "details" } }`.
Stable codes: `unauthorized`, `forbidden`, `bad_request`, `not_found`,
`database_error`, `internal_error`, `server_misconfigured`. Branch on
`code` rather than message text — the message is informational and may
change between releases.

### Postgres test database for development

`agentflow-db` integration tests are gated by `AGENTFLOW_DATABASE_TEST_URL`
to keep `cargo test --workspace` hermetic. To run them locally against the
docker-compose Postgres:

```bash
docker compose up -d postgres
export AGENTFLOW_DATABASE_TEST_URL=postgres://agentflow:agentflow@localhost:5432/agentflow
cargo test -p agentflow-db -p agentflow-server
```
