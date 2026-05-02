//! Bearer-token authentication middleware.
//!
//! v0.3.0 ships with a minimal scheme: a single token configured via the
//! `AGENTFLOW_API_TOKEN` env var (or [`AuthConfig::expected_token`])
//! protects every authenticated route. Future revisions will swap this out
//! for OAuth or JWT — the public surface is intentionally a single async
//! middleware so that swap is local.
//!
//! Routes opt in by attaching the [`require_bearer_token`] layer; health
//! checks bypass auth so probes from kubelet / load balancers stay simple.

use axum::{
  extract::{Request, State},
  http::header::AUTHORIZATION,
  middleware::Next,
  response::Response,
};

use crate::error::ApiError;

/// Configuration for bearer-token auth. `None` here is *not* the same as
/// "auth disabled" — see [`AppState`](crate::AppState) for the disabled
/// path; this struct is only attached when auth is on.
#[derive(Clone, Debug)]
pub struct AuthConfig {
  /// Static token compared against the `Authorization: Bearer <token>` header.
  pub expected_token: String,
}

impl AuthConfig {
  /// Build from env var. Returns `None` when `AGENTFLOW_API_TOKEN` is unset
  /// or empty so callers can decide whether to fail-closed (production) or
  /// run open (local dev / tests).
  pub fn from_env() -> Option<Self> {
    let token = std::env::var("AGENTFLOW_API_TOKEN").ok()?;
    let trimmed = token.trim();
    (!trimmed.is_empty()).then(|| Self {
      expected_token: trimmed.to_string(),
    })
  }
}

/// Axum middleware that rejects requests without a valid bearer token.
///
/// Attached to a router branch so a route can opt in:
///
/// ```ignore
/// Router::new()
///   .route("/v1/runs", post(submit_run))
///   .route_layer(from_fn_with_state(auth_config, require_bearer_token));
/// ```
pub async fn require_bearer_token(
  State(auth): State<AuthConfig>,
  request: Request,
  next: Next,
) -> Result<Response, ApiError> {
  let header = request
    .headers()
    .get(AUTHORIZATION)
    .ok_or(ApiError::Unauthorized)?
    .to_str()
    .map_err(|_| ApiError::Unauthorized)?;

  let token = header
    .strip_prefix("Bearer ")
    .ok_or(ApiError::Unauthorized)?
    .trim();

  if token.is_empty() {
    return Err(ApiError::Unauthorized);
  }
  if !constant_time_eq(token.as_bytes(), auth.expected_token.as_bytes()) {
    return Err(ApiError::Forbidden);
  }

  Ok(next.run(request).await)
}

/// Constant-time byte comparison so a brute-force attacker can't
/// distinguish wrong-length vs wrong-content tokens by latency. Matches the
/// length first (cheap, non-secret-dependent), then xors every byte.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
  if a.len() != b.len() {
    return false;
  }
  let mut diff: u8 = 0;
  for (x, y) in a.iter().zip(b.iter()) {
    diff |= x ^ y;
  }
  diff == 0
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn constant_time_eq_handles_match_and_mismatch() {
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"abcd"));
  }

  #[test]
  fn from_env_treats_empty_as_unset() {
    // SAFETY: dedicated env var only inspected by this test.
    unsafe {
      std::env::set_var("AGENTFLOW_API_TOKEN", "");
    }
    assert!(AuthConfig::from_env().is_none());

    unsafe {
      std::env::set_var("AGENTFLOW_API_TOKEN", "  ");
    }
    assert!(AuthConfig::from_env().is_none());

    unsafe {
      std::env::set_var("AGENTFLOW_API_TOKEN", "secret");
    }
    let cfg = AuthConfig::from_env().unwrap();
    assert_eq!(cfg.expected_token, "secret");

    unsafe {
      std::env::remove_var("AGENTFLOW_API_TOKEN");
    }
  }
}
