# AgentFlow Web UI Run Console

`agentflow-ui/` is the browser console for submitting, cancelling, and
debugging hybrid DAG, agent, and tool runs. It is a React + Vite + TypeScript
SPA embedded into `agentflow-server` as static assets under `/ui`.

## Product positioning

**Direction (P10.17.1, committed):** the Web UI is a **debugger /
run console**, scoped to *making a single run or session
investigable*. It is intentionally NOT an operator dashboard.

The bar for new features:

| In scope | Out of scope |
| --- | --- |
| Submit + cancel runs | Cost / billing aggregation views |
| Inspect single-run DAG + event timeline + payloads | Cross-run retry-rate trends |
| Live SSE follow + reconnect with `?after_seq=` | Policy-decision summary tabs |
| Trace replay / compare for a known run id | Worker fleet utilization dashboards |
| Session list + detail (Harness) with approval cards | Multi-tenant cost / quota UI |
| Workflow validation hints + per-event filter / search | Headless operation requirement (CLI + trace replay remain the canonical headless path) |
| Preference UI (theme / default tenant / etc.) | Replacing CLI in any automation surface |

The bar isn't permanent — if real-world usage proves an operator
view is the friction point, a v2 RFC can revisit. But "let's add a
cost tab" / "let's add a worker utilization chart" without an RFC
is the kind of drift this decision exists to prevent.

### Why debugger-only

1. **Single-dev maintenance budget.** The UI is dog-fooded by the
   AgentFlow maintainers; a sprawling dashboard surface costs more
   in test + e2e maintenance than the audience justifies today.
2. **Operator dashboards have better tools.** Prometheus + Grafana
   (cost trends, utilization, retry rates), tenant-aware BI
   (billing), and PagerDuty (on-call alerting) all solve the
   operator-dashboard problem better than a bespoke SPA. The
   server already exposes Prometheus metrics for scraping.
3. **CLI + trace replay are the headless surface.** Adding the
   UI to the critical path conflicts with the
   "shouldn't be required for headless operation" line in
   `RoadMap.md`.

### v1.1 additive scope (within the debugger boundary)

- Harness session replay UI (visual analogue of
  `agentflow harness replay --speed 2x` from P10.10.2).
- Trace compare polish (better diffs, more event types covered).
- Long-run perf polish — including the
  [P10.17.3 server-side `?filter=` pre-filter](../TODOs.md)
  for runs with >10k events.
- Preference UI wiring to `/v1/preferences` (P10.17.2) so
  per-user prefs sync across browsers.
- Playwright e2e in CI (P10.17.4).

### Alternatives for the out-of-scope items

If you find yourself wanting an operator-dashboard view, reach
for the right tool first:

- **Cost / token usage trends:** Prometheus + Grafana scraping
  the server's `/metrics` endpoint; consider adding the
  per-tenant cost gauge to that scrape rather than the SPA.
- **Worker fleet utilization:** the gRPC control plane
  (`agentflow-worker`) already exposes admission counts;
  surface them via Prometheus, not the UI.
- **Policy decision summary:** trace persistence has the
  per-event policy decisions; aggregate them in BI / a
  scheduled query, not the UI.
- **Multi-tenant cost / quota:** outside agentflow's scope
  entirely — wire your own billing system.

### How to use this section

When proposing a UI change, ask "does this fit the in-scope
column?" If yes, ship it. If no, write a v2 RFC. New contributors
should read this section before opening a PR that adds a new
top-level tab.

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
