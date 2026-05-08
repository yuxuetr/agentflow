# AgentFlow Web UI Debugger

`agentflow-ui/` is the browser debugger for hybrid DAG, agent, and tool runs.
The first implementation is a React + Vite + TypeScript SPA embedded into
`agentflow-server` as static assets under `/ui`.

## Architecture

- Frontend source: `agentflow-ui/src/`
- Embedded assets: `agentflow-ui/dist/`
- Server mount: `agentflow-server/src/ui.rs`
- REST dependency: `GET /v1/runs/{id}`
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
- DAG status: last observed nodes inferred from streamed event payloads.
- Agent timeline: ordered event stream, with agent/tool/failure status tones.
- Tool details: selected event payload rendered as JSON.

## Current Boundaries

- The debugger connects to one run at a time by run id.
- `/v1/runs` list support is still pending, so the acceptance target of a full
  run list remains open.
- DAG layout currently infers node labels from event payloads. The follow-up
  milestone will consume `agentflow-viz` Mermaid/DOT output and highlight the
  active node from trace events.
- Trace replay is represented by the persisted event stream for the active run;
  a dedicated replay browser that replaces or complements the TUI remains open.

## Verification

```bash
cargo test -p agentflow-server ui::tests --target-dir /tmp/agentflow-target
```
