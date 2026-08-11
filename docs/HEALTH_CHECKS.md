# Health Check System

**Since:** v0.2.0+ (`agentflow-core::health`); real `/health/ready` wiring since W5.3
**Status:** Production-ready, intentionally minimal

## Overview

`agentflow-core::health::HealthChecker` is a small registry of named async
checks that fold into an overall healthy/unhealthy verdict. It has no
built-in checks, no metadata, no metrics, and no timestamps — it is
deliberately just enough to answer "is this dependency reachable right
now?" Callers register whatever checks matter to them.

`agentflow-server` is the one real consumer today: `GET /health/ready`
builds a `HealthChecker`, registers a `"database"` check that runs
`SELECT 1` against the primary read pool, and returns `503` with the
per-check results when it fails.

## API

```rust
pub enum HealthStatus {
  Healthy,
  Degraded,
  Unhealthy,
}

pub struct HealthCheckResult {
  pub name: String,
  pub status: HealthStatus,
  pub message: Option<String>,
}

pub struct HealthReport {
  pub is_healthy: bool,
  pub checks: Vec<HealthCheckResult>,
}

pub struct HealthChecker { /* ... */ }

impl HealthChecker {
  pub fn new() -> Self;

  pub async fn add_check<F>(&self, name: impl Into<String>, check: F)
  where
    F: Fn() -> Pin<Box<dyn Future<Output = agentflow_core::Result<HealthStatus>> + Send>>
       + Send + Sync + 'static;

  pub async fn check_health(&self) -> HealthReport;
}
```

That is the entire surface. `add_check`'s closure returns
`agentflow_core::Result<HealthStatus>` (i.e. `Result<HealthStatus,
AgentFlowError>`) — an `Err` is folded into `HealthStatus::Unhealthy` with
the error's `Display` output as the message. `HealthReport::is_healthy` is
`true` iff no registered check resolved to `Unhealthy` (a `Degraded` check
still counts as healthy for this aggregate — callers that want stricter
readiness semantics should inspect `checks` directly).

There is no `remove_check`, no `set_metadata`, no built-in
`add_memory_check`/`add_metrics_check`, and `HealthReport` has no
`status()` method or `timestamp`/`metadata` fields. Earlier drafts of this
document described a larger surface that was never implemented — this
revision matches the real 79-line `agentflow-core/src/health.rs`.

## Usage

```rust
use agentflow_core::{AgentFlowError, HealthChecker, HealthStatus};

let checker = HealthChecker::new();
let pool = my_pg_pool.clone();
checker
  .add_check("database", move || {
    let pool = pool.clone();
    Box::pin(async move {
      sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map(|_| HealthStatus::Healthy)
        .map_err(|e| AgentFlowError::NetworkError {
          message: format!("database readiness check failed: {e}"),
        })
    })
  })
  .await;

let report = checker.check_health().await;
if !report.is_healthy {
  for check in &report.checks {
    tracing::warn!(name = %check.name, status = ?check.status, "unhealthy dependency");
  }
}
```

## `agentflow-server`'s `/health*` routes

| Route          | Behavior                                                                                          | K8s probe convention                        |
|-----------------|-----------------------------------------------------------------------------------------------------|----------------------------------------------|
| `GET /health`      | Unconditional `200 {"status":"ok","service":"agentflow-server"}`.                                | Generic health check                         |
| `GET /health/live`  | Same unconditional `200` as `/health`.                                                            | Liveness — must never depend on dependencies |
| `GET /health/ready` | Builds a `HealthChecker`, runs the `"database"` `SELECT 1` check, `200` if healthy else `503`.   | Readiness — should reflect real dependencies |

`/health` and `/health/live` deliberately stay unconditional per the
standard Kubernetes convention: a liveness probe that depends on an
external service (the database) causes cascading restarts when that
service degrades, which is exactly the failure mode liveness probes exist
to avoid. Only readiness should gate traffic on dependency health.

A `503` response body from `/health/ready` looks like:

```json
{
  "status": "unhealthy",
  "service": "agentflow-server",
  "checks": [
    { "name": "database", "status": "unhealthy", "message": "database readiness check failed: ..." }
  ]
}
```

All three routes are unauthenticated (same convention as `/metrics`) so
orchestrator/load-balancer probes don't need a bearer token.

## See Also

- [Timeout Control](TIMEOUT_CONTROL.md) — operation timeout management
- [Checkpoint Recovery](CHECKPOINT_RECOVERY.md) — workflow state persistence
- [Retry Mechanism](RETRY_MECHANISM.md) — automatic retry with backoff

---

**Last Updated:** 2026-08-12 (W5.3 — rewritten to match the real API and
document the `/health/ready` database check; the previous revision
described `add_memory_check`/`add_metrics_check`/`report.status()`/
`set_metadata`, none of which exist in `agentflow-core::health`)
