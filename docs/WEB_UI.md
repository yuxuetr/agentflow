# AgentFlow Web UI Debugger

`agentflow-ui/` is the browser debugger for hybrid DAG, agent, and tool runs.
The first implementation is a React + Vite + TypeScript SPA embedded into
`agentflow-server` as static assets under `/ui`.

## Architecture

- Frontend source: `agentflow-ui/src/`
- Embedded assets: `agentflow-ui/dist/`
- Server mount: `agentflow-server/src/ui.rs`
- REST dependency: `GET /v1/runs/{id}`
- Run list dependency: `GET /v1/runs?tenant_id=default&limit=20`
- DAG dependency: `GET /v1/runs/{id}/graph`
- Trace history dependency: `GET /v1/runs/{id}/events/history`
- Live stream dependency: `GET /v1/runs/{id}/events`

The server embeds the `dist/` files with `include_str!`, so production
deployments do not need Node.js or a separate static file server. Rebuild the
frontend with Vite when changing the TypeScript source, then commit the updated
`dist/` assets with the server change.

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

## Current Views

- Run summary: status, tenant, event count, workflow body.
- Run list: recent runs for the selected tenant.
- DAG status: `agentflow-viz` graph JSON/Mermaid overlaid with persisted node status events.
- Agent timeline: ordered event stream, with agent/tool/failure status tones.
- Tool details: selected event payload rendered as JSON.

## Current Boundaries

- The debugger streams one selected run at a time.
- DAG layout is generated from the stored workflow YAML. Runs submitted as
  opaque workflow references or invalid YAML fall back to inferred event labels.
- Trace replay is represented by `/events/history` plus the selected event
  detail pane. The TUI remains the headless SSH/CI alternative.

## Verification

```bash
cargo test -p agentflow-server ui::tests --target-dir /tmp/agentflow-target
```
