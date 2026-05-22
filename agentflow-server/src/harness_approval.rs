//! Server-side approval flow for Harness Mode (P-H.5 slice 2).
//!
//! Bridges the in-process [`ApprovalProvider`] trait (consumed by
//! `HookedTool` when a tool call requires human-in-the-loop) with the
//! HTTP API surface. The provider parks every pending request on a
//! `oneshot` channel keyed by `(session_id, request_id)`; the HTTP
//! decide route resolves the channel from the other side.
//!
//! Two routes:
//!
//! - `GET  /v1/harness/sessions/{id}/approvals` — list pending requests
//!   for the session. UIs poll this when they don't have a live SSE
//!   subscription (the `approval_requested` events on SSE are the
//!   primary push channel; the list endpoint is a backstop).
//! - `POST /v1/harness/sessions/{id}/approvals/{request_id}` — decide a
//!   pending request. Body is an [`ApprovalDecisionRequest`]; the
//!   handler resolves the parked oneshot, removes the entry from the
//!   pending map, and lets the executor proceed.
//!
//! The `approval_requested` and `approval_decided` envelopes are emitted
//! by `HookedTool` itself via the shared `SinkChain`, so this module
//! does **not** persist those events directly — that's deliberate so the
//! seq-counter contract stays single-source-of-truth inside
//! `agentflow-harness`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{
  Json,
  extract::{Path, State},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use agentflow_db::HarnessSessionRepo;
use agentflow_harness::{
  ApprovalDecision, ApprovalOutcome, ApprovalProvider, ApprovalRequest, ApprovalScope, HarnessError,
};

use crate::AppState;
use crate::error::{ApiError, JsonReq};

/// Process-local pending-approval registry shared between
/// [`ServerApprovalProvider`] and the HTTP decide route.
///
/// Each entry holds the original [`ApprovalRequest`] (so the list route
/// can surface it verbatim) and a `oneshot::Sender` the route resolves
/// to wake the parked provider future.
#[derive(Clone, Default)]
pub struct PendingApprovalRegistry {
  inner: Arc<Mutex<RegistryState>>,
}

#[derive(Default)]
struct RegistryState {
  /// Keyed by `(session_id, request_id)` — pairs identify entries
  /// uniquely across sessions without making session_id a top-level
  /// map (session deletion paths just iterate + filter).
  entries: HashMap<(String, String), PendingEntry>,
}

struct PendingEntry {
  request: ApprovalRequest,
  responder: oneshot::Sender<ApprovalDecision>,
}

impl std::fmt::Debug for PendingApprovalRegistry {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let len = self.inner.lock().map(|s| s.entries.len()).unwrap_or(0);
    f.debug_struct("PendingApprovalRegistry")
      .field("pending", &len)
      .finish()
  }
}

impl PendingApprovalRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// Register a pending request and return the responder side of the
  /// oneshot. The caller (typically [`ServerApprovalProvider::request`])
  /// awaits the receiver after registering.
  fn park(&self, request: ApprovalRequest) -> oneshot::Receiver<ApprovalDecision> {
    let (tx, rx) = oneshot::channel();
    let key = (request.session_id.clone(), request.id.clone());
    let mut state = self.inner.lock().expect("approval registry mutex poisoned");
    state.entries.insert(
      key,
      PendingEntry {
        request,
        responder: tx,
      },
    );
    rx
  }

  /// Drop a parked request without resolving it. Called when the
  /// provider future was cancelled / timed out so the responder doesn't
  /// linger forever.
  fn drop_pending(&self, session_id: &str, request_id: &str) {
    let mut state = self.inner.lock().expect("approval registry mutex poisoned");
    state
      .entries
      .remove(&(session_id.to_string(), request_id.to_string()));
  }

  /// Snapshot of pending requests for `session_id`. Sorted by
  /// `requested_at` ascending so the oldest pending decision surfaces
  /// first in UI lists.
  pub fn list(&self, session_id: &str) -> Vec<ApprovalRequest> {
    let state = self.inner.lock().expect("approval registry mutex poisoned");
    let mut entries: Vec<ApprovalRequest> = state
      .entries
      .iter()
      .filter(|((sess, _), _)| sess == session_id)
      .map(|(_, entry)| entry.request.clone())
      .collect();
    entries.sort_by_key(|req| req.requested_at);
    entries
  }

  /// Resolve a pending request from the HTTP side. Returns `Ok(())` if
  /// the responder was waiting, `Err(...)` otherwise so the route can
  /// return a meaningful 404.
  pub fn decide(
    &self,
    session_id: &str,
    request_id: &str,
    decision: ApprovalDecision,
  ) -> Result<(), ApprovalResolveError> {
    let mut state = self.inner.lock().expect("approval registry mutex poisoned");
    let Some(entry) = state
      .entries
      .remove(&(session_id.to_string(), request_id.to_string()))
    else {
      return Err(ApprovalResolveError::NotFound);
    };
    entry
      .responder
      .send(decision)
      .map_err(|_| ApprovalResolveError::ProviderGone)?;
    Ok(())
  }

  /// Snapshot of the total pending count. Cheap; used by tests +
  /// operational diagnostics.
  pub fn pending_count(&self) -> usize {
    self.inner.lock().map(|s| s.entries.len()).unwrap_or(0)
  }
}

#[derive(Debug, thiserror::Error)]
pub enum ApprovalResolveError {
  #[error("no pending approval matches")]
  NotFound,
  #[error("approval provider future was dropped before decision arrived")]
  ProviderGone,
}

/// Implementation of [`ApprovalProvider`] that parks every approval
/// request on the shared [`PendingApprovalRegistry`] and waits for a
/// matching HTTP decide call to resolve it.
///
/// Honors `ApprovalRequest::expires_at` (or falls back to
/// `default_timeout`). On timeout the entry is dropped and a
/// `HarnessError::ApprovalTimeout` bubbles up to the hook layer.
pub struct ServerApprovalProvider {
  registry: PendingApprovalRegistry,
  default_timeout: Duration,
}

impl ServerApprovalProvider {
  pub fn new(registry: PendingApprovalRegistry, default_timeout: Duration) -> Self {
    Self {
      registry,
      default_timeout,
    }
  }
}

#[async_trait]
impl ApprovalProvider for ServerApprovalProvider {
  fn name(&self) -> &str {
    "server"
  }

  async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, HarnessError> {
    let session_id = request.session_id.clone();
    let request_id = request.id.clone();
    let deadline = request
      .expires_at
      .map(|exp| (exp - Utc::now()).to_std().unwrap_or_default())
      .filter(|d| !d.is_zero())
      .unwrap_or(self.default_timeout);

    let receiver = self.registry.park(request);

    match tokio::time::timeout(deadline, receiver).await {
      Ok(Ok(decision)) => Ok(decision),
      Ok(Err(_recv_dropped)) => {
        // Sender was dropped without sending. Treat as a stop-the-run
        // signal: HookedTool will surface DenyAndStop.
        self.registry.drop_pending(&session_id, &request_id);
        Ok(ApprovalDecision {
          request_id,
          decision: ApprovalOutcome::DenyAndStop,
          scope: ApprovalScope::Once,
          decided_by: "server:dropped".to_string(),
          decided_at: Utc::now(),
          reason: Some("approval channel closed before decision".to_string()),
        })
      }
      Err(_timeout) => {
        self.registry.drop_pending(&session_id, &request_id);
        Err(HarnessError::ApprovalTimeout {
          timeout_ms: deadline.as_millis() as u64,
        })
      }
    }
  }
}

/// `GET /v1/harness/sessions/{id}/approvals` — list pending requests
/// for the session, oldest first.
pub async fn list_pending_approvals(
  State(state): State<AppState>,
  Path(session_id): Path<Uuid>,
) -> Result<Json<PendingApprovalsResponse>, ApiError> {
  // 404 if the session doesn't exist so the route doesn't silently
  // return an empty list for typos.
  let _session = state
    .repos
    .harness_sessions
    .get(session_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", session_id)))?;

  let pending = state.approval_registry.list(&session_id.to_string());
  Ok(Json(PendingApprovalsResponse { approvals: pending }))
}

#[derive(Debug, Serialize)]
pub struct PendingApprovalsResponse {
  pub approvals: Vec<ApprovalRequest>,
}

/// Body for `POST /v1/harness/sessions/{id}/approvals/{request_id}`.
#[derive(Debug, Deserialize)]
pub struct ApprovalDecisionRequest {
  pub decision: ApprovalOutcome,
  #[serde(default)]
  pub scope: Option<ApprovalScope>,
  #[serde(default)]
  pub decided_by: Option<String>,
  #[serde(default)]
  pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApprovalDecisionResponse {
  pub session_id: Uuid,
  pub request_id: String,
  pub resolved: bool,
}

/// `POST /v1/harness/sessions/{id}/approvals/{request_id}` — decide a
/// pending approval. The handler resolves the parked oneshot; the
/// `approval_decided` envelope is emitted by `HookedTool` on the
/// provider's return path, so the route does not persist the event
/// itself.
pub async fn decide_approval(
  State(state): State<AppState>,
  Path((session_id, request_id)): Path<(Uuid, String)>,
  JsonReq(body): JsonReq<ApprovalDecisionRequest>,
) -> Result<Json<ApprovalDecisionResponse>, ApiError> {
  let _session = state
    .repos
    .harness_sessions
    .get(session_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", session_id)))?;

  let decision = ApprovalDecision {
    request_id: request_id.clone(),
    decision: body.decision,
    scope: body.scope.unwrap_or(ApprovalScope::Once),
    decided_by: body.decided_by.unwrap_or_else(|| "user:http".to_string()),
    decided_at: Utc::now(),
    reason: body.reason,
  };

  match state
    .approval_registry
    .decide(&session_id.to_string(), &request_id, decision)
  {
    Ok(()) => Ok(Json(ApprovalDecisionResponse {
      session_id,
      request_id,
      resolved: true,
    })),
    Err(ApprovalResolveError::NotFound) => Err(ApiError::NotFound(format!(
      "no pending approval {} for session {}",
      request_id, session_id
    ))),
    Err(ApprovalResolveError::ProviderGone) => Err(ApiError::BadRequest(format!(
      "approval {} cannot be decided: provider future already dropped",
      request_id
    ))),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_harness::{ApprovalRisk, ApprovalScope};

  fn sample_request(session_id: &str, request_id: &str) -> ApprovalRequest {
    ApprovalRequest {
      id: request_id.to_string(),
      session_id: session_id.to_string(),
      step_index: 0,
      tool: "shell".to_string(),
      source: None,
      permissions: Vec::new(),
      idempotency: Default::default(),
      params_summary: serde_json::Value::Null,
      risk: ApprovalRisk::Medium,
      reason: "test".to_string(),
      requested_at: Utc::now(),
      expires_at: None,
    }
  }

  #[tokio::test]
  async fn park_then_decide_resolves_receiver() {
    let registry = PendingApprovalRegistry::new();
    let rx = registry.park(sample_request("s1", "r1"));
    assert_eq!(registry.pending_count(), 1);

    registry
      .decide(
        "s1",
        "r1",
        ApprovalDecision {
          request_id: "r1".to_string(),
          decision: ApprovalOutcome::Allow,
          scope: ApprovalScope::Once,
          decided_by: "test".to_string(),
          decided_at: Utc::now(),
          reason: None,
        },
      )
      .unwrap();

    let decision = rx.await.expect("oneshot resolved");
    assert!(matches!(decision.decision, ApprovalOutcome::Allow));
    assert_eq!(registry.pending_count(), 0);
  }

  #[tokio::test]
  async fn list_filters_by_session_and_sorts_oldest_first() {
    let registry = PendingApprovalRegistry::new();
    let mut older = sample_request("s1", "r-old");
    older.requested_at = Utc::now() - chrono::Duration::seconds(5);
    let _rx1 = registry.park(older);
    let _rx2 = registry.park(sample_request("s1", "r-new"));
    let _rx3 = registry.park(sample_request("s2", "r-other"));

    let pending = registry.list("s1");
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, "r-old");
    assert_eq!(pending[1].id, "r-new");

    let other = registry.list("s2");
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, "r-other");
  }

  #[tokio::test]
  async fn decide_unknown_request_returns_not_found() {
    let registry = PendingApprovalRegistry::new();
    let err = registry
      .decide(
        "s1",
        "missing",
        ApprovalDecision {
          request_id: "missing".to_string(),
          decision: ApprovalOutcome::Allow,
          scope: ApprovalScope::Once,
          decided_by: "test".to_string(),
          decided_at: Utc::now(),
          reason: None,
        },
      )
      .unwrap_err();
    assert!(matches!(err, ApprovalResolveError::NotFound));
  }

  #[tokio::test]
  async fn drop_pending_clears_entry_without_responder_send() {
    let registry = PendingApprovalRegistry::new();
    let _rx = registry.park(sample_request("s1", "r1"));
    assert_eq!(registry.pending_count(), 1);
    registry.drop_pending("s1", "r1");
    assert_eq!(registry.pending_count(), 0);
  }

  #[tokio::test]
  async fn provider_times_out_when_no_decision_arrives() {
    let registry = PendingApprovalRegistry::new();
    let provider = ServerApprovalProvider::new(registry.clone(), Duration::from_millis(50));
    let request = sample_request("s1", "r1");

    let err = provider
      .request(request)
      .await
      .expect_err("provider must time out");
    assert!(matches!(err, HarnessError::ApprovalTimeout { .. }));
    // Registry should be cleaned up so no stale entries linger.
    assert_eq!(registry.pending_count(), 0);
  }

  #[tokio::test]
  async fn provider_returns_decision_on_route_resolve() {
    let registry = PendingApprovalRegistry::new();
    let provider = Arc::new(ServerApprovalProvider::new(
      registry.clone(),
      Duration::from_secs(5),
    ));
    let provider_for_task = provider.clone();
    let handle = tokio::spawn(async move {
      provider_for_task
        .request(sample_request("s1", "r1"))
        .await
        .unwrap()
    });

    // Wait until the provider has actually parked the request before
    // calling decide (the spawned task may not have started yet).
    while registry.pending_count() == 0 {
      tokio::task::yield_now().await;
    }

    registry
      .decide(
        "s1",
        "r1",
        ApprovalDecision {
          request_id: "r1".to_string(),
          decision: ApprovalOutcome::Allow,
          scope: ApprovalScope::Session,
          decided_by: "test".to_string(),
          decided_at: Utc::now(),
          reason: None,
        },
      )
      .unwrap();

    let decision = handle.await.unwrap();
    assert!(matches!(decision.decision, ApprovalOutcome::Allow));
    assert!(matches!(decision.scope, ApprovalScope::Session));
  }
}
