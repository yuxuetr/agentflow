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
- Trace history dependency: `GET /v1/runs/{id}/events/history`
- Live stream dependency: `GET /v1/runs/{id}/events`
- Server-side filter (P10.17.3): `?filter=<expr>` accepted on
  `GET /v1/runs/{id}/events/history`. Grammar mirrors the
  client-side `eventFilter.ts`. The UI now passes the operator's
  saved filter expression on initial run attach so long runs
  don't ship every event over the wire; client-side filtering
  remains active as a defensive for live SSE events.

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

## Durable preferences (P10.17.2)

Selected preference values sync to the server via the
`/v1/preferences` API (P6.4) so they roam with the operator
across browsers. Each value still writes to `localStorage` as a
fast first-paint cache; the server is the cross-browser source
of truth.

| Local key | Server key | Synced today |
| --- | --- | --- |
| `agentflow.ui.tenantId` (run console) | `ui.run-console.tenant` | **Yes** |
| `agentflow.ui.newForm.tenant` | `ui.new-form.tenant` | mapped; wiring follow-up |
| `agentflow.ui.newForm.profile` | `ui.new-form.profile` | mapped; wiring follow-up |
| `agentflow.ui.harness.newForm.tenant_id` | `ui.harness-new-form.tenant` | mapped; wiring follow-up |
| `agentflow.ui.harness.newForm.profile` | `ui.harness-new-form.profile` | mapped; wiring follow-up |
| `agentflow.ui.harness.newForm.runtime_kind` | `ui.harness-new-form.runtime` | mapped; wiring follow-up |
| `agentflow.ui.run.eventFilter.<run_id>` | `ui.event-filter.<run_id>` | mapped; wiring follow-up |

"Mapped" = `serverKeyForLocal()` in `src/preferences.ts` returns
the server key; "Synced" = a React component currently calls
`prefSync.syncToServer(...)` + overlays `prefSync.serverPrefs`
into local state. The run-console tenant is the proof-of-pattern
slice that landed under P10.17.2; replicating the same 3-line
pattern at the other call sites is mechanical and tracked as a
follow-up inside the same TODO.

### Never synced (intentional)

| Local key | Why local-only |
| --- | --- |
| `agentflow.ui.apiToken` | **Security.** The token is the only sensitive value in the UI; uploading it to a route that lists every preference would leak it. |
| `agentflow.ui.workflowDraft` / `agentflow.ui.newForm.workflow` | Workflow YAML drafts can be large (>16 KiB server cap) and may contain example tokens that would trip the server's [token-shape rejection](../agentflow-server/src/preferences.rs). |
| `agentflow.ui.newForm.inputs` | Same as workflow drafts — user-supplied JSON, can include user-pasted content. |
| `agentflow.ui.harness.newForm.user_input` | Prompt text often contains personal info. |
| `agentflow.ui.harness.newForm.workspace_root` | Filesystem path — machine-specific (`/Users/alice/...` ≠ `C:\Users\bob\...`). |

### Wire shape contract

- `GET /v1/preferences` — returns the full tenant-scoped map.
  The UI reads it once per `(apiToken, tenant)` pair. Subsequent
  tenant switches refetch.
- `PUT /v1/preferences` — body `{ "preferences": { ... } }`. The
  UI debounces writes (500 ms window) so fast typing in the
  tenant input collapses to one PUT.
- Server-side key constraint: `^[a-zA-Z0-9_.\-:]{1,128}$`. UI
  keys are picked to satisfy this (dot-segmented with hyphens).
- Server-side value cap: 16 KiB per JSON value. UI's synced
  values are all short strings; large drafts go to the
  intentionally-local list above.
- Token-shape rejection: the server refuses values that look
  like Bearer tokens / `sk-…` keys / long hex digests / opaque
  alphanumeric strings ≥ 40 chars. The UI's synced keys don't
  carry such values, so the rejection is a backstop; combined
  with the never-synced list it makes accidental token upload
  essentially impossible.

## Current Views

- Run summary: status, tenant, event count, workflow body.
- Run list: recent runs for the selected tenant.
- Provider/config panel: auth state, run directory, and latest provider/model event.
- DAG status (event-derived): a button grid of nodes observed in
  the event stream, coloured by the most recent event tone. No
  spatial layout — graphical DAG visualisation was intentionally
  cut in P10.13.1 (the `agentflow-viz` crate was deleted; see
  `docs/ROADMAP_v2.md` Theme D for the decision rationale).
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

### Playwright E2E (P10.17.4)

E2E specs in `agentflow-ui/e2e/` exercise the SPA against a real
running `agentflow-server`. Local quickstart:

```bash
cd agentflow-ui
npm install                      # one-time
npm run e2e:install              # one-time — installs Chromium
# Start agentflow-server in another terminal first…
npm run e2e
```

Full operator + CI guide in
[`agentflow-ui/e2e/README.md`](../agentflow-ui/e2e/README.md).
CI runs nightly (10:30 UTC) and on `workflow_dispatch`; it's
**not** in `quality.yml::release-gate.needs` because the build +
browser-install cost doesn't justify gating every PR on the
current two-spec coverage. See `.github/workflows/ui-e2e.yml`.
