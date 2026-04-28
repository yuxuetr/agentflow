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
