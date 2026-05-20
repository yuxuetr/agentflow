//! P5.5 — worker admission policy.
//!
//! The control plane consults a [`WorkerAdmissionPolicy`] before
//! letting a worker claim tasks, report results, or heartbeat. The
//! policy decides three orthogonal questions:
//!
//! 1. **Identity** — is this worker on the allowlist? If a worker is
//!    not in `allowed_workers` it is rejected with
//!    [`AdmissionError::UnknownWorker`].
//! 2. **Credential** — if the worker has a pre-shared-key entry in
//!    `pre_shared_keys`, does the presented token match one of the
//!    valid PSKs for that worker? PSKs are stored as a `HashSet` per
//!    worker to support token rotation (overlap-add-then-remove): an
//!    operator adds a new token, the worker rolls over, then the
//!    operator removes the old token. In-flight tasks survive the
//!    rotation because admission is checked per-call, not per-task.
//! 3. **Capacity** — does admitting this worker / this claim push the
//!    fleet past `max_workers` or this worker past
//!    `max_concurrent_tasks_per_worker`?
//!
//! Stability tier: **experimental**. The pre-shared-key flavour ships
//! with v0.4.0 as a hardening building block. Signed JWT identity is
//! intentionally deferred until the wider auth story (single signing
//! authority, key rotation, audience scoping) stabilizes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::jwt::verify_worker_jwt_at;
use super::{
  JwtPolicy, SchedulerError, WorkerControlPlane, WorkerHeartbeat, WorkerId, WorkerProtocol,
  WorkerTask, WorkerTaskResult,
};

/// Pluggable thin shim around the JWT verifier so the policy layer
/// doesn't depend on the `verify_worker_jwt_at` symbol directly. Lets
/// future test doubles intercept the call site without a feature
/// gate.
fn verify_worker_jwt_for_check(
  token: &str,
  policy: &JwtPolicy,
  worker_id: &str,
  now_secs: i64,
) -> Result<(), super::jwt::JwtVerifyError> {
  verify_worker_jwt_at(token, policy, worker_id, now_secs).map(|_| ())
}

/// Reasons the control plane may reject a worker call.
///
/// `AdmissionError` is not transport-aware on purpose: each adapter
/// (gRPC, in-memory) maps these variants to its own wire shape (e.g.
/// `tonic::Status::permission_denied` for the gRPC surface).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdmissionError {
  #[error("worker '{worker_id}' is not on the admission allowlist")]
  UnknownWorker { worker_id: String },
  #[error("worker '{worker_id}' did not present a credential, but one is required")]
  MissingCredential { worker_id: String },
  /// `reason` is the specific verifier message ("psk did not match
  /// any rotation entry", "token expired at …", "token issuer
  /// mismatch …", etc.). The transport adapter forwards it to
  /// operators as a `tonic::Status::permission_denied` detail so the
  /// gateway logs identify *why* a token was rejected, not just that
  /// it was. The message is intentionally generic at the credential
  /// level (no token fragments) — JWT internals appear in the JWT
  /// verifier's own error chain, not here.
  #[error("worker '{worker_id}' presented an invalid credential: {reason}")]
  InvalidCredential { worker_id: String, reason: String },
  #[error("max worker fleet size reached ({max})")]
  WorkerFleetExhausted { max: usize },
  #[error("worker '{worker_id}' exceeded its concurrent-task quota ({max})")]
  WorkerQuotaExhausted { worker_id: String, max: u32 },
}

/// Credential a worker presents on every call.
///
/// The token is optional; whether one is required depends on the
/// per-worker PSK configuration. A `None` token against a worker that
/// has any PSK entries is rejected with
/// [`AdmissionError::MissingCredential`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerCredential {
  pub worker_id: WorkerId,
  pub token: Option<String>,
}

impl WorkerCredential {
  pub fn new(worker_id: WorkerId, token: Option<String>) -> Self {
    Self { worker_id, token }
  }

  /// Convenience: build a credential with no token presented.
  pub fn anonymous(worker_id: WorkerId) -> Self {
    Self {
      worker_id,
      token: None,
    }
  }
}

/// Admission policy consulted by [`WorkerControlPlane`] on every call.
///
/// All fields default to "no constraint" so a brand-new policy admits
/// every worker — that's the dev / single-process path the existing
/// tests rely on.
///
/// The credential flavour is per-worker: a worker may use **PSK** (in
/// [`Self::pre_shared_keys`]) or **JWT** (in [`Self::jwt_workers`])
/// but not both. A worker in neither set is anonymous — the existing
/// dev / single-process default. See `jwt.rs` for the JWT semantics
/// (issuer / audience / key rotation / leeway).
#[derive(Debug, Clone, Default)]
pub struct WorkerAdmissionPolicy {
  /// If `Some`, only these worker IDs may join. `None` = any valid
  /// worker id (the dev / single-process default).
  pub allowed_workers: Option<HashSet<WorkerId>>,
  /// Per-worker PSK rotation table. Each entry is a set so that
  /// `add new token → flip worker → remove old token` works without
  /// dropping in-flight tasks. Workers absent from this map are not
  /// required to present a token *unless* they are in
  /// [`Self::jwt_workers`].
  pub pre_shared_keys: HashMap<WorkerId, HashSet<String>>,
  /// Global JWT verification config (issuer / audience / key pool /
  /// leeway). When `Some`, workers listed in [`Self::jwt_workers`]
  /// present their token as a JWT and the control plane verifies it
  /// against this policy.
  pub jwt: Option<JwtPolicy>,
  /// Worker IDs that must authenticate with a JWT against
  /// [`Self::jwt`]. PSK and JWT are mutually exclusive per worker;
  /// a worker listed in both `pre_shared_keys` and `jwt_workers` is
  /// a config error and the PSK path is treated as authoritative to
  /// avoid silent downgrades.
  pub jwt_workers: HashSet<WorkerId>,
  /// Cap on distinct admitted workers (workers with a recent
  /// successful heartbeat). `None` = unbounded.
  pub max_workers: Option<usize>,
  /// Cap on simultaneously-claimed tasks per worker. `None` = unbounded.
  pub max_concurrent_tasks_per_worker: Option<u32>,
}

impl WorkerAdmissionPolicy {
  /// "Anything goes" policy — equivalent to `Default::default()`.
  pub fn open() -> Self {
    Self::default()
  }

  /// Check whether the worker may make admission-gated calls.
  ///
  /// `currently_active` is the count of distinct workers the control
  /// plane has admitted recently (excluding this one). Callers pass
  /// the count *before* the check so the policy can decide whether
  /// adding this worker would breach the cap.
  pub fn check(
    &self,
    credential: &WorkerCredential,
    currently_active: usize,
  ) -> Result<(), AdmissionError> {
    self.check_at(credential, currently_active, chrono::Utc::now().timestamp())
  }

  /// Variant of [`check`] that takes the "now" timestamp explicitly.
  /// Exists so JWT-flavour tests can exercise expiry / nbf paths
  /// deterministically without sleeping or mocking the clock at the
  /// jsonwebtoken layer.
  pub fn check_at(
    &self,
    credential: &WorkerCredential,
    currently_active: usize,
    now_secs: i64,
  ) -> Result<(), AdmissionError> {
    if let Some(allowed) = &self.allowed_workers
      && !allowed.contains(&credential.worker_id)
    {
      return Err(AdmissionError::UnknownWorker {
        worker_id: credential.worker_id.0.clone(),
      });
    }

    // PSK takes precedence over JWT when a worker is misconfigured to
    // be in both sets. The intent is "no silent downgrade" — an
    // operator who fat-fingers a worker into both lists gets the
    // stricter PSK check, not a weaker JWT-only path.
    if let Some(valid_tokens) = self.pre_shared_keys.get(&credential.worker_id) {
      let Some(presented) = credential.token.as_deref() else {
        return Err(AdmissionError::MissingCredential {
          worker_id: credential.worker_id.0.clone(),
        });
      };
      if !valid_tokens.contains(presented) {
        return Err(AdmissionError::InvalidCredential {
          worker_id: credential.worker_id.0.clone(),
          reason: "psk did not match any rotation entry".to_string(),
        });
      }
    } else if self.jwt_workers.contains(&credential.worker_id) {
      let Some(policy) = self.jwt.as_ref() else {
        // The worker is listed as needing JWT but the policy never
        // configured the verification keys / issuer / audience. This
        // is purely a server-side config error; reject loudly so the
        // operator sees the gap instead of accidentally admitting an
        // unauthenticated worker.
        return Err(AdmissionError::InvalidCredential {
          worker_id: credential.worker_id.0.clone(),
          reason: "worker is in jwt_workers but no JwtPolicy is configured".to_string(),
        });
      };
      let Some(presented) = credential.token.as_deref() else {
        return Err(AdmissionError::MissingCredential {
          worker_id: credential.worker_id.0.clone(),
        });
      };
      if let Err(err) =
        verify_worker_jwt_for_check(presented, policy, credential.worker_id.0.as_str(), now_secs)
      {
        return Err(AdmissionError::InvalidCredential {
          worker_id: credential.worker_id.0.clone(),
          reason: err.to_string(),
        });
      }
    }

    if let Some(max) = self.max_workers
      && currently_active >= max
    {
      // The "+1 would be" semantics: if `currently_active` already
      // accounts for this worker (it's a re-heartbeat), the caller
      // passes `currently_active - 1` so the policy never reject's
      // already-admitted workers.
      return Err(AdmissionError::WorkerFleetExhausted { max });
    }

    Ok(())
  }

  /// Check whether the worker may claim one more task right now.
  pub fn check_claim_quota(
    &self,
    worker_id: &WorkerId,
    in_flight: u32,
  ) -> Result<(), AdmissionError> {
    let Some(max) = self.max_concurrent_tasks_per_worker else {
      return Ok(());
    };
    if in_flight >= max {
      return Err(AdmissionError::WorkerQuotaExhausted {
        worker_id: worker_id.0.clone(),
        max,
      });
    }
    Ok(())
  }
}

/// Either an admission failure or a transport / state error from the
/// underlying control plane. The gRPC adapter maps `Admission` to
/// `permission_denied` and `Scheduler` to its current Status mapping.
#[derive(Debug, Error)]
pub enum ControlError {
  #[error(transparent)]
  Admission(#[from] AdmissionError),
  #[error(transparent)]
  Scheduler(#[from] SchedulerError),
}

/// Admission-gated façade over [`WorkerControlPlane`].
///
/// `AuthenticatedControlPlane` is the production-facing entry point
/// for distributed workers: every call goes through the admission
/// policy *and* updates the per-worker in-flight counter that backs
/// `max_concurrent_tasks_per_worker`. The unauthenticated
/// [`WorkerControlPlane`] is retained for the dev / single-process
/// path (it's what the scheduler smokes still exercise directly).
///
/// **Stability:** experimental until the wider distributed worker
/// promise stabilizes. See `docs/STABILITY.md` for the matrix.
#[derive(Debug, Clone)]
pub struct AuthenticatedControlPlane<P> {
  inner: WorkerControlPlane<P>,
  policy: Arc<WorkerAdmissionPolicy>,
  state: Arc<Mutex<AuthenticatedState>>,
}

#[derive(Debug, Default)]
struct AuthenticatedState {
  /// Workers we've successfully admitted at least once. Used to
  /// enforce `max_workers` and to recognize a returning worker as
  /// "already counted" on subsequent calls.
  admitted: HashSet<WorkerId>,
  /// In-flight (claimed-but-not-reported) tasks per worker. Drives
  /// the `max_concurrent_tasks_per_worker` cap.
  in_flight: HashMap<WorkerId, u32>,
}

impl<P> AuthenticatedControlPlane<P>
where
  P: WorkerProtocol + Clone,
{
  pub fn new(inner: WorkerControlPlane<P>, policy: WorkerAdmissionPolicy) -> Self {
    Self {
      inner,
      policy: Arc::new(policy),
      state: Arc::new(Mutex::new(AuthenticatedState::default())),
    }
  }

  /// Underlying control plane — useful for queue ingestion and run
  /// snapshots, neither of which is gated by admission.
  pub fn inner(&self) -> &WorkerControlPlane<P> {
    &self.inner
  }

  /// Run the admission check and, on success, mark the worker as
  /// admitted. Idempotent — re-admitting an existing worker is a
  /// no-op (it doesn't double-count toward `max_workers`).
  pub async fn admit(&self, credential: &WorkerCredential) -> Result<(), AdmissionError> {
    let mut state = self.state.lock().await;
    let already_admitted = state.admitted.contains(&credential.worker_id);
    let currently_active = state.admitted.len() - usize::from(already_admitted);
    self.policy.check(credential, currently_active)?;
    state.admitted.insert(credential.worker_id.clone());
    Ok(())
  }

  /// Admission-gated heartbeat. Marks the worker admitted on first
  /// successful call, then forwards to the inner control plane.
  pub async fn heartbeat(
    &self,
    credential: WorkerCredential,
    heartbeat: WorkerHeartbeat,
  ) -> Result<(), ControlError> {
    self.admit(&credential).await?;
    self.inner.heartbeat(heartbeat).await?;
    Ok(())
  }

  /// Admission-gated task claim. Checks both identity / credential /
  /// fleet caps *and* the per-worker concurrency cap before letting
  /// the worker pull another task off the queue.
  pub async fn claim_task(
    &self,
    credential: WorkerCredential,
  ) -> Result<Option<WorkerTask>, ControlError> {
    self.admit(&credential).await?;

    let in_flight = {
      let state = self.state.lock().await;
      state
        .in_flight
        .get(&credential.worker_id)
        .copied()
        .unwrap_or(0)
    };
    self
      .policy
      .check_claim_quota(&credential.worker_id, in_flight)?;

    let task = self.inner.claim_task(credential.worker_id.clone()).await?;
    if task.is_some() {
      let mut state = self.state.lock().await;
      *state.in_flight.entry(credential.worker_id).or_insert(0) += 1;
    }
    Ok(task)
  }

  /// Admission-gated result report. Always decrements the in-flight
  /// counter so retries / restarts don't permanently inflate the
  /// per-worker quota even when the inner protocol rejects the
  /// result.
  pub async fn report_result(
    &self,
    credential: WorkerCredential,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), ControlError> {
    self.admit(&credential).await?;

    let inner_result = self
      .inner
      .report_result(credential.worker_id.clone(), task_id, result)
      .await;

    let mut state = self.state.lock().await;
    if let Some(slot) = state.in_flight.get_mut(&credential.worker_id) {
      *slot = slot.saturating_sub(1);
    }
    Ok(inner_result?)
  }

  /// Number of distinct workers currently admitted. Tests use this
  /// to assert fleet-size enforcement; production paths shouldn't
  /// need it.
  pub async fn admitted_worker_count(&self) -> usize {
    self.state.lock().await.admitted.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn worker(label: &str) -> WorkerId {
    WorkerId::new(label).expect("valid worker label")
  }

  #[test]
  fn open_policy_admits_anyone() {
    let policy = WorkerAdmissionPolicy::open();
    let cred = WorkerCredential::anonymous(worker("any"));
    assert!(policy.check(&cred, 0).is_ok());
  }

  #[test]
  fn allowlist_rejects_unknown_worker() {
    let policy = WorkerAdmissionPolicy {
      allowed_workers: Some([worker("a"), worker("b")].into_iter().collect()),
      ..Default::default()
    };
    assert!(matches!(
      policy.check(&WorkerCredential::anonymous(worker("intruder")), 0),
      Err(AdmissionError::UnknownWorker { .. })
    ));
    assert!(
      policy
        .check(&WorkerCredential::anonymous(worker("a")), 0)
        .is_ok()
    );
  }

  #[test]
  fn psk_rejects_missing_or_wrong_token() {
    let mut psks = HashMap::new();
    psks.insert(worker("a"), HashSet::from(["good".to_string()]));
    let policy = WorkerAdmissionPolicy {
      pre_shared_keys: psks,
      ..Default::default()
    };
    assert!(matches!(
      policy.check(&WorkerCredential::anonymous(worker("a")), 0),
      Err(AdmissionError::MissingCredential { .. })
    ));
    assert!(matches!(
      policy.check(
        &WorkerCredential::new(worker("a"), Some("bad".to_string())),
        0
      ),
      Err(AdmissionError::InvalidCredential { .. })
    ));
    // The reason field carries the verifier-specific message — useful
    // for transports that surface it to operators.
    if let Err(AdmissionError::InvalidCredential { reason, .. }) = policy.check(
      &WorkerCredential::new(worker("a"), Some("bad".to_string())),
      0,
    ) {
      assert!(
        reason.contains("psk"),
        "PSK rejection reason should name the credential flavour: {reason}"
      );
    }
    assert!(
      policy
        .check(
          &WorkerCredential::new(worker("a"), Some("good".to_string())),
          0
        )
        .is_ok()
    );
  }

  #[test]
  fn psk_rotation_accepts_overlap_window() {
    // Operator stages a rotation by adding "v2" alongside "v1":
    // both tokens are valid until the rollout completes.
    let mut psks = HashMap::new();
    psks.insert(
      worker("a"),
      HashSet::from(["v1".to_string(), "v2".to_string()]),
    );
    let policy = WorkerAdmissionPolicy {
      pre_shared_keys: psks,
      ..Default::default()
    };
    assert!(
      policy
        .check(
          &WorkerCredential::new(worker("a"), Some("v1".to_string())),
          0
        )
        .is_ok()
    );
    assert!(
      policy
        .check(
          &WorkerCredential::new(worker("a"), Some("v2".to_string())),
          0
        )
        .is_ok()
    );
  }

  #[test]
  fn fleet_cap_rejects_when_full() {
    let policy = WorkerAdmissionPolicy {
      max_workers: Some(2),
      ..Default::default()
    };
    assert!(
      policy
        .check(&WorkerCredential::anonymous(worker("a")), 1)
        .is_ok()
    );
    assert!(matches!(
      policy.check(&WorkerCredential::anonymous(worker("b")), 2),
      Err(AdmissionError::WorkerFleetExhausted { max: 2 })
    ));
  }

  #[test]
  fn per_worker_concurrency_cap_rejects_when_full() {
    let policy = WorkerAdmissionPolicy {
      max_concurrent_tasks_per_worker: Some(4),
      ..Default::default()
    };
    assert!(policy.check_claim_quota(&worker("a"), 3).is_ok());
    assert!(matches!(
      policy.check_claim_quota(&worker("a"), 4),
      Err(AdmissionError::WorkerQuotaExhausted { max: 4, .. })
    ));
  }

  mod jwt_flavor {
    use super::*;
    use crate::scheduler::jwt::{JwtPolicy, WorkerJwtClaims};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    fn sign_hs256(secret: &[u8], claims: &WorkerJwtClaims) -> String {
      encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret),
      )
      .expect("sign HS256 test token")
    }

    fn jwt_policy() -> JwtPolicy {
      JwtPolicy::new("test-issuer", "agentflow-workers-prod")
        .with_hs256_secret(b"super-secret".to_vec())
        .with_leeway_seconds(5)
    }

    fn jwt_claims(sub: &str, now: i64) -> WorkerJwtClaims {
      WorkerJwtClaims {
        iss: "test-issuer".into(),
        aud: vec!["agentflow-workers-prod".into()],
        sub: sub.into(),
        exp: now + 300,
        iat: Some(now),
        nbf: None,
      }
    }

    #[test]
    fn jwt_worker_with_valid_token_admitted() {
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("jwt-a")].into_iter().collect(),
        ..Default::default()
      };
      let token = sign_hs256(b"super-secret", &jwt_claims("jwt-a", now));
      let cred = WorkerCredential::new(worker("jwt-a"), Some(token));
      assert!(policy.check_at(&cred, 0, now).is_ok());
    }

    #[test]
    fn jwt_worker_without_token_is_missing_credential() {
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("jwt-a")].into_iter().collect(),
        ..Default::default()
      };
      let cred = WorkerCredential::anonymous(worker("jwt-a"));
      assert!(matches!(
        policy.check_at(&cred, 0, now),
        Err(AdmissionError::MissingCredential { .. })
      ));
    }

    #[test]
    fn jwt_worker_with_wrong_subject_rejected_with_reason() {
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("jwt-a")].into_iter().collect(),
        ..Default::default()
      };
      // Token issued for jwt-b but presented by jwt-a.
      let token = sign_hs256(b"super-secret", &jwt_claims("jwt-b", now));
      let cred = WorkerCredential::new(worker("jwt-a"), Some(token));
      match policy.check_at(&cred, 0, now) {
        Err(AdmissionError::InvalidCredential { reason, .. }) => {
          assert!(
            reason.contains("subject mismatch"),
            "reason should name the subject mismatch: {reason}"
          );
        }
        other => panic!("expected InvalidCredential, got {other:?}"),
      }
    }

    #[test]
    fn jwt_worker_with_expired_token_rejected() {
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("jwt-a")].into_iter().collect(),
        ..Default::default()
      };
      let mut claims = jwt_claims("jwt-a", now);
      claims.exp = now - 600;
      let token = sign_hs256(b"super-secret", &claims);
      let cred = WorkerCredential::new(worker("jwt-a"), Some(token));
      match policy.check_at(&cred, 0, now) {
        Err(AdmissionError::InvalidCredential { reason, .. }) => {
          assert!(
            reason.contains("expired"),
            "reason should say expired: {reason}"
          );
        }
        other => panic!("expected InvalidCredential, got {other:?}"),
      }
    }

    #[test]
    fn jwt_worker_without_jwt_policy_is_server_config_error() {
      // Worker is listed in jwt_workers but the operator forgot to
      // attach a JwtPolicy. We refuse to admit (better fail-closed
      // than accidentally admit unauthenticated).
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: None,
        jwt_workers: [worker("jwt-a")].into_iter().collect(),
        ..Default::default()
      };
      let cred = WorkerCredential::new(worker("jwt-a"), Some("anything".into()));
      match policy.check_at(&cred, 0, now) {
        Err(AdmissionError::InvalidCredential { reason, .. }) => {
          assert!(
            reason.contains("no JwtPolicy"),
            "reason should call out the misconfiguration: {reason}"
          );
        }
        other => panic!("expected InvalidCredential, got {other:?}"),
      }
    }

    #[test]
    fn psk_takes_precedence_over_jwt_when_misconfigured() {
      // Worker is listed in BOTH pre_shared_keys and jwt_workers. We
      // run the PSK check (stricter / explicit shared-secret); a JWT
      // is silently rejected here even if it would otherwise verify.
      let now = 1_700_000_000;
      let mut psks = HashMap::new();
      psks.insert(worker("dual"), HashSet::from(["psk-only".to_string()]));
      let policy = WorkerAdmissionPolicy {
        pre_shared_keys: psks,
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("dual")].into_iter().collect(),
        ..Default::default()
      };
      let jwt_token = sign_hs256(b"super-secret", &jwt_claims("dual", now));
      let cred = WorkerCredential::new(worker("dual"), Some(jwt_token));
      // PSK gate runs first, sees the JWT string isn't in the PSK
      // pool, rejects.
      assert!(matches!(
        policy.check_at(&cred, 0, now),
        Err(AdmissionError::InvalidCredential { .. })
      ));
      // The PSK still admits its rightful token, confirming the gate
      // didn't degrade to JWT logic.
      let psk_cred = WorkerCredential::new(worker("dual"), Some("psk-only".to_string()));
      assert!(policy.check_at(&psk_cred, 0, now).is_ok());
    }

    #[test]
    fn unrelated_worker_still_anonymous_when_jwt_policy_present() {
      // Adding a JwtPolicy must not retroactively require auth from
      // workers who were anonymous before — only `jwt_workers`
      // workers are affected.
      let now = 1_700_000_000;
      let policy = WorkerAdmissionPolicy {
        jwt: Some(jwt_policy()),
        jwt_workers: [worker("jwt-only")].into_iter().collect(),
        ..Default::default()
      };
      let cred = WorkerCredential::anonymous(worker("anon-worker"));
      assert!(policy.check_at(&cred, 0, now).is_ok());
    }
  }
}
