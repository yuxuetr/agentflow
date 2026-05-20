# AgentFlow Operator Dashboards

Status: **dashboard JSON checked in (P10.14.2); metric emission tracked under P10.14.2-FU1**

Checked-in Grafana dashboards for operating an `agentflow-server`
deployment. Import any of the JSON files in `grafana/` directly into
Grafana 8+ (the dashboards are `schemaVersion: 38`).

## Files

| File | Purpose |
|------|---------|
| `grafana/agentflow-overview.json` | Operator overview: workflow runs, harness sessions, worker fleet, retention sweep, memory + state size. 9 panels across 4 rows. |

## Importing

```bash
# Via Grafana UI: Dashboards → Import → Upload JSON file.
# Pick a Prometheus datasource at the import prompt; the dashboard
# uses a `$DS_PROMETHEUS` variable so it survives datasource renames.

# Via grafanactl / API:
curl -X POST \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $GRAFANA_API_TOKEN" \
  "$GRAFANA_URL/api/dashboards/db" \
  -d "$(jq '{dashboard: ., overwrite: true, folderUid: ""}' grafana/agentflow-overview.json)"
```

## Metric contract

The dashboards expect the following Prometheus metric names from
`agentflow-server`'s `/metrics` endpoint. The contract is documented
in [`docs/KUBERNETES_DEPLOYMENT.md`](../docs/KUBERNETES_DEPLOYMENT.md)
§"Grafana Dashboard"; this file is the runtime artifact.

| Metric | Type | Labels | Purpose |
|--------|------|--------|---------|
| `agentflow_health_status` | gauge | `component` | 1 = up, 0 = down. Status panel. |
| `agentflow_workflow_runs_active` | gauge | `tenant` | Queued + running runs. Active-runs timeseries. |
| `agentflow_workflow_completed_total` | counter | `status` | Terminal-status throughput. `status ∈ {succeeded, failed, cancelled}`. |
| `agentflow_workflow_duration_seconds` | histogram | — | Full-run wall clock. Drives p50/p95/p99 panel. |
| `agentflow_nodes_failed_total` | counter | `node_type` | Per-node-type failure rate; the canonical "what broke" signal. |
| `agentflow_workers_admitted` | gauge | — | Currently-admitted worker count (per `WorkerAdmissionPolicy`). |
| `agentflow_worker_tasks_inflight` | gauge | `worker_id` | Per-worker in-flight task count. |
| `agentflow_memory_usage_bytes` | gauge | — | Server process resident memory. |
| `agentflow_state_size_bytes` | gauge | `run_id` | Per-run `FlowValue` state size. |
| `agentflow_cleanup_runs_deleted_total` | counter | — | Retention sweep — `runs` rows reaped. |
| `agentflow_cleanup_events_deleted_total` | counter | — | Retention sweep — `events` rows reaped. |
| `agentflow_cleanup_artifacts_deleted_total` | counter | — | Retention sweep — `artifacts` rows reaped. |
| `agentflow_harness_sessions_active` | gauge | `status` | Harness Mode sessions, broken out by `status ∈ {running, paused, completed, ...}`. |
| `agentflow_harness_approvals_pending` | gauge | — | Pending approval requests. Anything > 0 means an operator action is waiting. |

## Current emission status

**As of P10.14.2-FU1 (slice 1), `agentflow-server` exposes
`/metrics` as Prometheus text format.** The recorder is
installed during `agentflow_server::serve::run` boot;
`GET /metrics` is unauthenticated (same convention as
`/health`) so Prometheus scrapers don't need a bearer token.

Live series:

| Metric | Status | Source |
|--------|--------|--------|
| `agentflow_workflow_completed_total{status}` | ✅ live | `WorkflowEventListener` (terminal events) |
| `agentflow_workflow_duration_seconds` | ✅ live | `WorkflowEventListener` (terminal events) |
| `agentflow_nodes_failed_total{node_type}` | ✅ live | `WorkflowEventListener` (NodeFailed events) |
| `agentflow_cleanup_*_deleted_total` | ⏳ FU2 | needs hook into `cleanup_expired` |
| `agentflow_workers_admitted` | ⏳ FU3 | needs hook into `AuthenticatedControlPlane` |
| `agentflow_worker_tasks_inflight` | ⏳ FU3 | needs hook into `AuthenticatedControlPlane` |
| `agentflow_harness_sessions_active{status}` | ⏳ FU4 | needs hook into harness session repo |
| `agentflow_harness_approvals_pending` | ⏳ FU4 | needs hook into harness session repo |
| `agentflow_health_status{component}` | ⏳ FU5 | scrape-time inspector |
| `agentflow_memory_usage_bytes` | ⏳ FU5 | scrape-time inspector |
| `agentflow_state_size_bytes` | ⏳ FU5 | scrape-time inspector |
| `agentflow_workflow_runs_active{tenant}` | ⏳ FU5 | scrape-time inspector |

Until the deferred series wire up, the corresponding Grafana
panels show empty / zero values. The dashboard JSON is checked
in *now* against the forward contract so operators have
something to import as each slice lands.

The follow-up TODOs are tracked in `TODOs.md` under
`P10.14.2-FU2` / `-FU3` / `-FU4` / `-FU5`.

## Conventions

- Every panel references `${DS_PROMETHEUS}` (a dashboard variable)
  so the JSON survives datasource renames during import.
- Time ranges default to `now-1h` and the refresh dropdown lists
  the standard 5s … 1h options. Override at import time if your
  scrape interval is unusual.
- Stat panels use background coloring (red < threshold, green ≥
  threshold) so the system-health row is glance-readable from a
  wall-mounted ops display.
- The dashboard is tagged `agentflow`, `overview`, `operator` —
  use those tags to find related dashboards in folders.

## Adding a new dashboard

1. Save the dashboard from Grafana via "Settings → JSON Model →
   Copy to clipboard."
2. Strip the `id`, `iteration`, and `version` fields so re-imports
   don't conflict with existing dashboards by id.
3. Replace any hard-coded `datasource.uid` strings with
   `${DS_PROMETHEUS}` and add the corresponding variable to
   `templating.list` (copy from `agentflow-overview.json`).
4. Add a row to the table at the top of this file.
5. Open a PR; reviewer checks the JSON parses with `jq . file.json`.

## Conventions deliberately not adopted

- **No alert rules in the dashboards.** Alerts belong in
  Prometheus alertmanager config or in Grafana's dedicated alert
  rules surface, not embedded in dashboard JSON. Embedding them
  here couples the operator's notification policy to the
  visualization, which two different teams typically own.
- **No tenant-specific dashboards out of the box.** Multi-tenant
  splits are a Grafana variable (`tenant=$tenant`), not a separate
  JSON file. Add a `$tenant` variable to your local copy if you
  want a per-tenant view.
- **No SLO panels.** SLO tracking belongs in a dedicated SLO
  dashboard with burn-rate alerts; we'll add one when the
  underlying error-budget metric is part of the contract above.
