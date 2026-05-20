//! Prometheus `/metrics` endpoint emission (P10.14.2-FU1).
//!
//! The Grafana dashboard checked in under `dashboards/grafana/`
//! (P10.14.2) pins the metric-name contract this module satisfies.
//! Production code paths fire `metrics::counter!()` /
//! `metrics::histogram!()` / `metrics::gauge!()` macros; this
//! module installs the recorder that aggregates them into the
//! Prometheus text format scraped by `GET /metrics`.
//!
//! Wiring status:
//!
//! - **Live in this slice:** `agentflow_workflow_completed_total{status}`,
//!   `agentflow_workflow_duration_seconds`,
//!   `agentflow_nodes_failed_total{node_type}` — wired via
//!   [`WorkflowEventListener`].
//! - **Deferred to follow-up TODOs** (see `P10.14.2-FU2` /
//!   `-FU3` / `-FU4` in `TODOs.md`):
//!   - `agentflow_cleanup_*_deleted_total` — needs hook into
//!     `agentflow_server::cleanup::cleanup_expired`.
//!   - `agentflow_workers_admitted` /
//!     `agentflow_worker_tasks_inflight` — needs hook into
//!     `AuthenticatedControlPlane` state.
//!   - `agentflow_harness_sessions_active{status}` /
//!     `agentflow_harness_approvals_pending` — needs hook into
//!     the Harness Mode session repo.
//!   - `agentflow_health_status{component}` /
//!     `agentflow_memory_usage_bytes` /
//!     `agentflow_state_size_bytes` /
//!     `agentflow_workflow_runs_active{tenant}` — process /
//!     state inspectors, computed at scrape time.
//!
//! Until those land, the corresponding Grafana panels render as
//! empty — that's documented in `dashboards/README.md` under
//! "Current emission status."

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Snapshot handle installed by [`init_recorder`]. Held in a
/// `OnceLock` so the recorder install only happens once per
/// process — installing twice panics, so multi-`run()` callers
/// (tests that boot a server multiple times in one process) need
/// the idempotency.
static RECORDER: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the Prometheus recorder so subsequent
/// `metrics::counter!()` / `metrics::histogram!()` /
/// `metrics::gauge!()` calls anywhere in the workspace are
/// aggregated. Idempotent — subsequent calls are no-ops.
///
/// Returns `Ok(())` on first call, `Ok(())` on subsequent calls
/// (the existing handle stays in place), or `Err(String)` if the
/// recorder install fails for a reason other than "already
/// installed" (typically an internal builder error).
pub fn init_recorder() -> Result<(), String> {
  if RECORDER.get().is_some() {
    return Ok(());
  }
  let recorder = PrometheusBuilder::new()
    .set_buckets_for_metric(
      metrics_exporter_prometheus::Matcher::Suffix("_seconds".into()),
      // Histogram buckets tuned for workflow durations: 100ms
      // up to 10 minutes, covering the realistic span from a
      // fast template-only run to a long multi-LLM agent loop.
      // The `Inf` bucket catches the long tail.
      &[
        0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
      ],
    )
    .map_err(|e| format!("set bucket schedule: {e}"))?
    .build_recorder();
  let handle = recorder.handle();
  // `set_global_recorder` is one-shot per process. If a prior
  // install already happened (e.g. another test in the same
  // binary booted a server), we silently accept the existing
  // recorder rather than panic — `RECORDER.get()` already gated
  // the common case; this branch is the rare race.
  if metrics::set_global_recorder(recorder).is_err() {
    tracing::debug!("metrics recorder already installed; reusing existing");
    return Ok(());
  }
  RECORDER.set(handle).ok();
  Ok(())
}

/// Render the current snapshot as Prometheus text format.
/// Returns the empty string when the recorder hasn't been
/// installed yet — that's the operator-visible signal that
/// `init_recorder()` was never called (e.g. a test built the
/// router without going through `run()`).
pub fn render_text() -> String {
  RECORDER.get().map(|h| h.render()).unwrap_or_default()
}

/// Metric-name constants — pinned exactly to the contract
/// documented in `dashboards/README.md` and
/// `docs/KUBERNETES_DEPLOYMENT.md`. Centralising them here lets
/// the wire-shape compat tests assert the names without
/// stringly-typing each call site.
pub mod names {
  /// Counter, label `status` (`succeeded` / `failed` / `cancelled`).
  pub const WORKFLOW_COMPLETED_TOTAL: &str = "agentflow_workflow_completed_total";
  /// Histogram of full-run wall clock in seconds.
  pub const WORKFLOW_DURATION_SECONDS: &str = "agentflow_workflow_duration_seconds";
  /// Counter, label `node_type`.
  pub const NODES_FAILED_TOTAL: &str = "agentflow_nodes_failed_total";
}

/// Record the terminal status of a workflow run. Fires the
/// `agentflow_workflow_completed_total{status}` counter and the
/// `agentflow_workflow_duration_seconds` histogram together —
/// every terminal transition is observed exactly once, so the
/// rate-of-completions panel and the p50/p95/p99 panel both
/// stay correct.
pub fn observe_workflow_completion(status: &'static str, duration_seconds: f64) {
  metrics::counter!(names::WORKFLOW_COMPLETED_TOTAL, "status" => status).increment(1);
  metrics::histogram!(names::WORKFLOW_DURATION_SECONDS).record(duration_seconds);
}

/// Record a node failure with its capability label so the
/// "what broke just now" panel breaks out by `node_type`. Pass
/// `None` (the pre-P10.16.2 default) when the workflow event
/// doesn't carry a node_type — the metric still increments but
/// against an `unknown` label.
pub fn observe_node_failure(node_type: Option<&str>) {
  let node_type = node_type.unwrap_or("unknown").to_string();
  metrics::counter!(names::NODES_FAILED_TOTAL, "node_type" => node_type).increment(1);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn metric_names_match_contracted_constants() {
    // Pin the exact wire strings — the Grafana dashboard JSON
    // queries against them verbatim, so any rename here is a
    // breaking change for operators.
    assert_eq!(
      names::WORKFLOW_COMPLETED_TOTAL,
      "agentflow_workflow_completed_total"
    );
    assert_eq!(
      names::WORKFLOW_DURATION_SECONDS,
      "agentflow_workflow_duration_seconds"
    );
    assert_eq!(names::NODES_FAILED_TOTAL, "agentflow_nodes_failed_total");
  }

  #[test]
  fn render_text_returns_empty_when_recorder_uninstalled() {
    // A different process (`cargo test`) than the one that
    // installed the recorder gets an empty string instead of a
    // panic. Lets unit tests run without booting the full
    // server.
    if RECORDER.get().is_none() {
      assert_eq!(render_text(), "");
    }
  }

  #[test]
  fn init_recorder_is_idempotent_within_process() {
    // Calling twice is fine — the second call is a no-op.
    let _ = init_recorder();
    let _ = init_recorder();
    // After two installs we should still have a single handle.
    // Render shouldn't panic even if the install path raced.
    let _ = render_text();
  }

  #[test]
  fn observe_workflow_completion_increments_counter_and_histogram() {
    let _ = init_recorder();
    observe_workflow_completion("succeeded", 1.23);
    observe_workflow_completion("failed", 0.05);
    let text = render_text();
    // The exporter's text format names the counter with a
    // `_total` suffix (already in the constant) and emits a
    // `# TYPE` header line we can grep for.
    assert!(
      text.contains("agentflow_workflow_completed_total"),
      "counter must be emitted; got: {text}"
    );
    assert!(
      text.contains("status=\"succeeded\""),
      "status label must be in the output; got: {text}"
    );
    assert!(
      text.contains("agentflow_workflow_duration_seconds"),
      "histogram must be emitted; got: {text}"
    );
  }

  #[test]
  fn observe_node_failure_labels_with_node_type_or_unknown() {
    let _ = init_recorder();
    observe_node_failure(Some("llm"));
    observe_node_failure(None);
    let text = render_text();
    assert!(
      text.contains("agentflow_nodes_failed_total"),
      "counter must be emitted; got: {text}"
    );
    assert!(
      text.contains("node_type=\"llm\""),
      "known node_type label must appear; got: {text}"
    );
    assert!(
      text.contains("node_type=\"unknown\""),
      "fallback label `unknown` must appear for untagged failures; got: {text}"
    );
  }
}
