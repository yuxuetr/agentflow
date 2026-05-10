# AgentFlow Web UI Run Console

`agentflow-ui/` is the browser console for submitting, cancelling, and
debugging hybrid DAG, agent, and tool runs. It is a React + Vite + TypeScript
SPA embedded into `agentflow-server` as static assets under `/ui`.

## Architecture

- Frontend source: `agentflow-ui/src/`
- Embedded assets: `agentflow-ui/dist/`
- Server mount: `agentflow-server/src/ui.rs`
- Submit dependency: `POST /v1/runs`
- Cancel dependency: `POST /v1/runs/{id}:cancel`
- REST dependency: `GET /v1/runs/{id}`
- Run list dependency: `GET /v1/runs?tenant_id=default&limit=20`
- DAG dependency: `GET /v1/runs/{id}/graph`
- Trace history dependency: `GET /v1/runs/{id}/events/history`
- Live stream dependency: `GET /v1/runs/{id}/events`

The server embeds the `dist/` files with `include_str!`, so production
deployments do not need Node.js or a separate static file server. Rebuild the
frontend with Vite when changing the TypeScript source, then commit the updated
`dist/` assets with the server change.

Vite is configured to emit stable asset names:

- `/ui/assets/app.js`
- `/ui/assets/styles.css`

Keep those names stable unless `agentflow-server/src/ui.rs` changes too.

## Local Development

Run the backend as usual:

```bash
cargo run -p agentflow-server
```

Then open:

```text
http://localhost:8080/ui
```

For frontend-only iteration:

```bash
cd agentflow-ui
npm install
npm run dev
```

The Vite dev server should proxy or target an `agentflow-server` instance for
`/v1/runs/{id}` and `/v1/runs/{id}/events`.

## Run Console

- Submit run: paste config-first workflow YAML, set the tenant, then submit.
- Connect run: paste an existing run id or select a recent run.
- Cancel run: cancels queued/running runs via `/v1/runs/{id}:cancel`.
- Auth token: paste the bearer token configured by `AGENTFLOW_API_TOKEN`.
  The token is stored in browser local storage and sent in `Authorization`
  headers for REST and streaming requests.
- Live reconnect: the UI streams SSE with `fetch`, tracks the last observed
  sequence, and reconnects with `?after_seq=<seq>` after transient failures.

## Current Views

- Run summary: status, tenant, event count, workflow body.
- Run list: recent runs for the selected tenant.
- Provider/config panel: auth state, run directory, and latest provider/model event.
- DAG status: `agentflow-viz` graph JSON/Mermaid overlaid with persisted node status events.
- DAG node detail: selected node id/status and latest matching event.
- Agent timeline: ordered event stream, with agent/tool/failure status tones.
- Agent/tool policy detail: latest agent/tool/policy event.
- Event payload: selected event payload rendered as JSON.

## Current Boundaries

- The debugger streams one selected run at a time.
- DAG layout is generated from the stored workflow YAML. Runs submitted as
  opaque workflow references or invalid YAML fall back to inferred event labels.
- Trace replay is represented by `/events/history` plus the selected event
  detail pane. The TUI remains the headless SSH/CI alternative.
- Provider/config status is inferred from run metadata and LLM events; there is
  not yet a dedicated `/v1/config/status` endpoint.

## Verification

```bash
cargo test -p agentflow-server ui::tests --target-dir /tmp/agentflow-target
cd agentflow-ui && npm test
```
