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
- `AGENTFLOW_RUN_DIR=/data/runs` can be set to control workflow artifact storage.
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

### Read-replica routing (P10.15.2)

Read-heavy gateways can route `GET /v1/runs/{id}`,
`GET /v1/runs/{id}/events/history`, `GET /v1/harness/sessions`,
and similar `list_*` / `get_*` paths to a Postgres read replica
while writes (run submission, status updates, retention sweep
deletes) continue to hit the primary.

```bash
# Primary URL for writes + migrations:
export DATABASE_URL="postgres://gw:secret@primary.db.internal/agentflow"
# Replica URL for SELECTs:
export AGENTFLOW_DATABASE_READ_URL="postgres://gw:secret@replica.db.internal/agentflow"

agentflow serve
# Or via the CLI flag:
agentflow serve \
  --database-url "$DATABASE_URL" \
  --database-read-url "$AGENTFLOW_DATABASE_READ_URL"
```

When `AGENTFLOW_DATABASE_READ_URL` is unset (the default),
reads fall back to the primary — that's the single-node
deployment behavior and is fully backwards-compatible.

**Caveats:**

- **Replication lag.** A client that writes (`POST /v1/runs`)
  and immediately reads (`GET /v1/runs/{id}`) may observe the
  prior state because the replica hasn't caught up. The
  cleanup sweep, run-row creation, and harness session
  creation all read+write through the primary in the same
  call, so this only affects HTTP clients that submit then
  re-query in the same round trip.
- **Migrations always run against the primary.** The replica
  catches up via Postgres streaming replication; we never
  apply DDL against it directly.
- **Pool budgets are independent.** The replica pool defaults
  to 2× the primary's connection cap (16 vs 8) on the
  assumption that the gateway is read-heavy. Operators with
  unusual ratios can rebuild from `Database::connect_with_replica`
  directly.

### Submit and inspect a run

`POST /v1/runs` executes config-first workflow YAML through
`agentflow-core::Flow`. The server persists the queued row immediately,
switches it to `running` in the background, stores workflow events in the
`events` table, streams them over SSE, and sets the terminal status to
`succeeded` or `failed`.

Run artifacts are written under `AGENTFLOW_RUN_DIR/<run_id>` when
`AGENTFLOW_RUN_DIR` is set; otherwise the default is
`~/.agentflow/runs/<run_id>` (or a temp directory if the home directory cannot
be resolved). The chosen per-run path is returned as `run_dir` from
`GET /v1/runs/{id}`.

```bash
# Submit a workflow body. Returns { "run_id": "...", "status": "queued" }.
RUN=$(curl -sX POST http://localhost:3000/v1/runs \
  -H "Authorization: Bearer dev-secret" \
  -H "Content-Type: application/json" \
  -d @examples/server/fixed_dag_run.json)
RUN_ID=$(echo "$RUN" | jq -r .run_id)

# Poll for state.
curl -s -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID | jq .

# Subscribe to live events (Server-Sent Events). Press Ctrl-C to detach.
curl -N -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID/events
```

Expected event kinds for a successful fixed DAG include:
`workflow.started`, `node.started`, `node.output.captured`, `node.completed`,
and `workflow.completed`.

#### Per-run retention overrides (P10.14.1)

The `POST /v1/runs` body accepts an optional `retention_overrides`
object that pins a run's events and/or artifacts for longer than
the tenant + profile default:

```json
{
  "workflow": "...yaml...",
  "retention_overrides": {
    "events_days": 90,
    "artifacts_days": 365
  }
}
```

Semantics: the cleanup sweep uses
`max(global_default, override)` so a per-run override can only
ever *extend* retention. Pinning a run also pins its row
itself — otherwise the `ON DELETE CASCADE` from `runs` would
yank the pinned events/artifacts out from under the override.
Negative values are rejected with `bad_request`; `Some(0)` is
accepted and normalized to "no override". See
`docs/SERVER_BACKUP_RESTORE.md` for the operator-side retention
defaults.

To inspect run status (the previously-documented
`/v1/runs/{id}/graph` endpoint was removed in P10.13.1 along with
the `agentflow-viz` crate; use the SSE event stream or the run
detail endpoint instead):

```bash
curl -s -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID | jq .
```

To cancel a queued or running run:

```bash
curl -sX POST -H "Authorization: Bearer dev-secret" \
  http://localhost:3000/v1/runs/$RUN_ID:cancel | jq .
```

Cancellation is idempotent. A queued or running run is marked `cancelled`, the
background task receives a cancellation signal and is aborted, and a
`run.cancelled` event is appended for SSE/history consumers. Cancelling an
already terminal run returns its current status without error.

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
dispatches through the same executor used by `/v1/runs`. Direct skill run
execution remains a separate integration path; config-first workflows can run
skills today by using `skill_agent` / `agent` nodes in workflow YAML.

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
