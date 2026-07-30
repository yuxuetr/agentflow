//! Bearer-token authentication middleware.
//!
//! v0.3.0 shipped a minimal scheme: a single token configured via the
//! `AGENTFLOW_API_TOKEN` env var (or [`AuthConfig::expected_token`])
//! protects every authenticated route. U1.1 adds an additive second
//! tier — `AGENTFLOW_API_TOKEN_TENANTS`, a list of tokens each bound
//! to exactly one tenant — so a deployment that needs real tenant
//! isolation can issue distinct credentials per tenant instead of
//! every caller sharing one token and self-reporting whatever tenant
//! it likes via `X-Agentflow-Tenant` (see `tenant.rs`). A future OAuth
//! /JWT integration can replace both tiers; the public surface is
//! intentionally a single async middleware so that swap stays local.
//!
//! Routes opt in by attaching the [`require_bearer_token`] layer; health
//! checks bypass auth so probes from kubelet / load balancers stay simple.

use agentflow_tools::SecurityProfile;
use axum::{
  extract::{Request, State},
  http::header::AUTHORIZATION,
  middleware::Next,
  response::Response,
};

use crate::error::ApiError;

/// A bearer token bound to exactly one tenant (U1.1). A request
/// authenticated with `token` acts as `tenant_id`, full stop — see
/// [`AuthenticatedTenant`] and `tenant::extract_tenant_id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantToken {
  pub token: String,
  pub tenant_id: String,
}

/// Configuration for bearer-token auth. `None` here is *not* the same as
/// "auth disabled" — see [`AppState`](crate::AppState) for the disabled
/// path; this struct is only attached when auth is on.
#[derive(Clone, Debug, Default)]
pub struct AuthConfig {
  /// Legacy unbound token, compared against the
  /// `Authorization: Bearer <token>` header. Empty means "not
  /// configured" (only `tenant_tokens` entries authenticate). A
  /// request authenticated this way acts as whatever tenant the
  /// client claims via `X-Agentflow-Tenant` (default `"default"`) —
  /// unchanged pre-U1.1 behavior, kept for single-tenant / local-dev
  /// deployments that never adopted per-tenant tokens.
  pub expected_token: String,
  /// U1.1: additional tokens, each bound to exactly one tenant.
  pub tenant_tokens: Vec<TenantToken>,
}

impl AuthConfig {
  /// Build from a raw token string. Empty or whitespace-only tokens are
  /// treated as absent so callers can keep local-dev auth optional while
  /// production startup can fail closed.
  pub fn from_token(token: Option<&str>) -> Option<Self> {
    let trimmed = token?.trim();
    (!trimmed.is_empty()).then(|| Self {
      expected_token: trimmed.to_string(),
      tenant_tokens: Vec::new(),
    })
  }

  /// Build from env var. Returns `None` when `AGENTFLOW_API_TOKEN` is unset
  /// or empty so callers can decide whether to fail-closed (production) or
  /// run open (local dev / tests).
  pub fn from_env() -> Option<Self> {
    let token = std::env::var("AGENTFLOW_API_TOKEN").ok();
    Self::from_token(token.as_deref())
  }
}

/// Resolve bearer auth for the active security profile.
///
/// `dev` and `local` keep historical no-token local startup behavior.
/// `production` fails closed when no non-empty token is configured
/// (legacy `token`, or at least one `tenant_tokens_raw` entry).
pub fn resolve_auth_config(
  profile: SecurityProfile,
  token: Option<&str>,
  tenant_tokens_raw: Option<&str>,
) -> Result<Option<AuthConfig>, AuthConfigError> {
  let expected_token = token
    .map(str::trim)
    .filter(|t| !t.is_empty())
    .map(str::to_string);
  let tenant_tokens = parse_tenant_tokens(tenant_tokens_raw)?;

  // Fail closed on ambiguous config rather than silently picking one
  // interpretation: a token can't simultaneously be "unbound, trusts
  // the client header" and "bound to exactly one tenant".
  if let Some(expected) = &expected_token
    && tenant_tokens.iter().any(|t| &t.token == expected)
  {
    return Err(AuthConfigError::AmbiguousTenantToken);
  }

  let has_any_token = expected_token.is_some() || !tenant_tokens.is_empty();
  if profile.defaults().auth.require_api_token && !has_any_token {
    return Err(AuthConfigError::MissingRequiredToken { profile });
  }
  if !has_any_token {
    return Ok(None);
  }
  Ok(Some(AuthConfig {
    expected_token: expected_token.unwrap_or_default(),
    tenant_tokens,
  }))
}

/// Resolve bearer auth from `AGENTFLOW_API_TOKEN` /
/// `AGENTFLOW_API_TOKEN_TENANTS` for the active profile.
pub fn resolve_auth_config_from_env(
  profile: SecurityProfile,
) -> Result<Option<AuthConfig>, AuthConfigError> {
  let token = std::env::var("AGENTFLOW_API_TOKEN").ok();
  let tenant_tokens_raw = std::env::var("AGENTFLOW_API_TOKEN_TENANTS").ok();
  resolve_auth_config(profile, token.as_deref(), tenant_tokens_raw.as_deref())
}

/// Parses `AGENTFLOW_API_TOKEN_TENANTS`: comma-separated
/// `token:tenant_id` pairs, e.g. `tokA:tenant-a,tokB:tenant-b`.
/// `None`/empty input yields an empty list, not an error.
fn parse_tenant_tokens(raw: Option<&str>) -> Result<Vec<TenantToken>, AuthConfigError> {
  let Some(raw) = raw else {
    return Ok(Vec::new());
  };
  let mut tokens = Vec::new();
  let mut seen = std::collections::HashSet::new();
  for (index, entry) in raw
    .split(',')
    .map(str::trim)
    .filter(|e| !e.is_empty())
    .enumerate()
  {
    let Some((token, tenant_id)) = entry.split_once(':') else {
      return Err(AuthConfigError::MalformedTenantTokenEntry { index });
    };
    let token = token.trim();
    let tenant_id = tenant_id.trim();
    if token.is_empty() || tenant_id.is_empty() {
      return Err(AuthConfigError::MalformedTenantTokenEntry { index });
    }
    if !seen.insert(token.to_string()) {
      return Err(AuthConfigError::DuplicateTenantToken);
    }
    tokens.push(TenantToken {
      token: token.to_string(),
      tenant_id: tenant_id.to_string(),
    });
  }
  Ok(tokens)
}

#[derive(Debug, thiserror::Error)]
pub enum AuthConfigError {
  #[error(
    "AGENTFLOW_API_TOKEN or AGENTFLOW_API_TOKEN_TENANTS is required when \
     AGENTFLOW_SECURITY_PROFILE is '{profile}'"
  )]
  MissingRequiredToken { profile: SecurityProfile },
  /// Never echoes the offending token value into the error message —
  /// this ends up in startup logs, which are not a safe place for a
  /// bearer credential.
  #[error(
    "AGENTFLOW_API_TOKEN_TENANTS entry #{index} is not in 'token:tenant_id' format \
     (comma-separated, e.g. 'tokA:tenant-a,tokB:tenant-b')"
  )]
  MalformedTenantTokenEntry { index: usize },
  #[error("AGENTFLOW_API_TOKEN_TENANTS binds the same token to more than one tenant")]
  DuplicateTenantToken,
  #[error(
    "a token in AGENTFLOW_API_TOKEN_TENANTS is identical to AGENTFLOW_API_TOKEN — bind it to \
     exactly one tenant, or remove it from one of the two variables"
  )]
  AmbiguousTenantToken,
}

/// Inserted into request extensions by [`require_bearer_token`] so
/// `tenant::extract_tenant_id` (layered *inside* auth — see
/// `create_router`) can tell whether the presented token is bound to
/// a specific tenant (U1.1). Absent entirely when no auth is
/// configured at all (pre-U1.1 behavior: trust the client header).
#[derive(Clone, Debug)]
pub struct AuthenticatedTenant(pub Option<String>);

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
  mut request: Request,
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

  // U1.1: a token bound to a specific tenant takes precedence over
  // the legacy unbound token, so `tenant::extract_tenant_id` (layered
  // inside this middleware — see `create_router`) can enforce that
  // binding instead of trusting whatever `X-Agentflow-Tenant` the
  // client sends.
  if let Some(bound) = auth
    .tenant_tokens
    .iter()
    .find(|t| constant_time_eq(token.as_bytes(), t.token.as_bytes()))
  {
    request
      .extensions_mut()
      .insert(AuthenticatedTenant(Some(bound.tenant_id.clone())));
    return Ok(next.run(request).await);
  }

  if !auth.expected_token.is_empty()
    && constant_time_eq(token.as_bytes(), auth.expected_token.as_bytes())
  {
    request.extensions_mut().insert(AuthenticatedTenant(None));
    return Ok(next.run(request).await);
  }

  Err(ApiError::Forbidden)
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

  #[test]
  fn token_parser_trims_and_rejects_empty_values() {
    assert!(AuthConfig::from_token(None).is_none());
    assert!(AuthConfig::from_token(Some("")).is_none());
    assert!(AuthConfig::from_token(Some("  ")).is_none());

    let cfg = AuthConfig::from_token(Some("  secret  ")).unwrap();
    assert_eq!(cfg.expected_token, "secret");
  }

  #[test]
  fn production_profile_requires_non_empty_token() {
    let err = resolve_auth_config(SecurityProfile::Production, None, None).unwrap_err();
    assert!(matches!(
      err,
      AuthConfigError::MissingRequiredToken {
        profile: SecurityProfile::Production
      }
    ));

    let err = resolve_auth_config(SecurityProfile::Production, Some("  "), None).unwrap_err();
    assert!(matches!(
      err,
      AuthConfigError::MissingRequiredToken {
        profile: SecurityProfile::Production
      }
    ));

    let cfg = resolve_auth_config(SecurityProfile::Production, Some("secret"), None)
      .unwrap()
      .unwrap();
    assert_eq!(cfg.expected_token, "secret");
  }

  #[test]
  fn local_and_dev_profiles_keep_auth_optional() {
    assert!(
      resolve_auth_config(SecurityProfile::Local, None, None)
        .unwrap()
        .is_none()
    );
    assert!(
      resolve_auth_config(SecurityProfile::Dev, None, None)
        .unwrap()
        .is_none()
    );
  }

  // U1.1: token → tenant binding.

  #[test]
  fn production_profile_accepts_tenant_tokens_alone_with_no_legacy_token() {
    let cfg = resolve_auth_config(
      SecurityProfile::Production,
      None,
      Some("tokA:tenant-a,tokB:tenant-b"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(cfg.expected_token, "");
    assert_eq!(
      cfg.tenant_tokens,
      vec![
        TenantToken {
          token: "tokA".into(),
          tenant_id: "tenant-a".into()
        },
        TenantToken {
          token: "tokB".into(),
          tenant_id: "tenant-b".into()
        },
      ]
    );
  }

  #[test]
  fn tenant_tokens_parser_rejects_entries_missing_a_colon() {
    let err = resolve_auth_config(SecurityProfile::Local, None, Some("not-a-pair")).unwrap_err();
    assert!(matches!(
      err,
      AuthConfigError::MalformedTenantTokenEntry { index: 0 }
    ));
  }

  #[test]
  fn tenant_tokens_parser_rejects_empty_token_or_tenant_half() {
    assert!(matches!(
      resolve_auth_config(SecurityProfile::Local, None, Some(":tenant-a")).unwrap_err(),
      AuthConfigError::MalformedTenantTokenEntry { index: 0 }
    ));
    assert!(matches!(
      resolve_auth_config(SecurityProfile::Local, None, Some("tokA:")).unwrap_err(),
      AuthConfigError::MalformedTenantTokenEntry { index: 0 }
    ));
  }

  #[test]
  fn tenant_tokens_parser_rejects_the_same_token_bound_twice() {
    let err = resolve_auth_config(
      SecurityProfile::Local,
      None,
      Some("tokA:tenant-a,tokA:tenant-b"),
    )
    .unwrap_err();
    assert!(matches!(err, AuthConfigError::DuplicateTenantToken));
  }

  #[test]
  fn a_token_cannot_be_both_the_legacy_token_and_tenant_bound() {
    let err = resolve_auth_config(
      SecurityProfile::Local,
      Some("shared-token"),
      Some("shared-token:tenant-a"),
    )
    .unwrap_err();
    assert!(matches!(err, AuthConfigError::AmbiguousTenantToken));
  }

  #[test]
  fn tenant_tokens_alone_satisfy_the_production_require_token_gate_without_legacy_token() {
    // Regresses a design mistake this fix must avoid: an operator who
    // migrates fully to per-tenant tokens (no `AGENTFLOW_API_TOKEN` at
    // all) must not be told production requires a token they don't have.
    assert!(
      resolve_auth_config(SecurityProfile::Production, None, Some("tokA:tenant-a"))
        .unwrap()
        .is_some()
    );
  }
}
