//! Tenant binding middleware (P2.6, U1.1).
//!
//! Every `/v1/*` request runs through `extract_tenant_id`, which injects a
//! typed [`TenantId`] extension. Handlers extract it via
//! `Extension(tenant): Extension<TenantId>` and use it to scope DB reads +
//! writes.
//!
//! Since U1.1, tenant resolution has two tiers, in priority order:
//!
//! 1. **Token-bound** (`AuthConfig::tenant_tokens`, see `auth.rs`): when
//!    `require_bearer_token` authenticates the request with a token bound
//!    to a specific tenant, it inserts an [`AuthenticatedTenant`] request
//!    extension carrying that tenant. `extract_tenant_id` is layered
//!    *inside* the auth middleware (see `create_router`) so it always
//!    runs after — and can see — that extension. This tenant is
//!    authoritative: a client-supplied `X-Agentflow-Tenant` naming a
//!    *different* tenant is rejected (`ApiError::TenantMismatch`), not
//!    silently overridden, matching the same "explicit claim conflicts
//!    with the authoritative value → reject" rule Q1.4.3 already applies
//!    to a request body's `tenant_id` vs. the resolved tenant.
//! 2. **Client header** (pre-U1.1 behavior, unchanged): when no auth is
//!    configured at all, or the legacy unbound token matched (no
//!    `AuthenticatedTenant` extension, or one carrying `None`), the
//!    `X-Agentflow-Tenant` header is trusted as-is, defaulting to
//!    `"default"` when absent — single-tenant local-dev deployments stay
//!    zero-config, and single-shared-token deployments that never adopted
//!    per-tenant tokens are unaffected.

use axum::{extract::Request, http::HeaderName, middleware::Next, response::Response};

use crate::auth::AuthenticatedTenant;
use crate::error::ApiError;

/// Canonical header name. Lowercased per HTTP/2; clients can send either
/// case and middleware normalizes during extraction.
pub const TENANT_HEADER: HeaderName = HeaderName::from_static("x-agentflow-tenant");

/// Default tenant for single-tenant zero-config deployments.
pub const DEFAULT_TENANT: &str = "default";

/// Tenant scope for the current request. Cloneable so handlers and spawned
/// background tasks can both stamp it onto rows / events.
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

impl TenantId {
  pub fn as_str(&self) -> &str {
    &self.0
  }
  pub fn default_for_local() -> Self {
    Self(DEFAULT_TENANT.to_string())
  }
}

impl From<String> for TenantId {
  fn from(value: String) -> Self {
    Self(value)
  }
}

impl From<&str> for TenantId {
  fn from(value: &str) -> Self {
    Self(value.to_string())
  }
}

/// Axum middleware that resolves the request's [`TenantId`] and inserts
/// it as an extension — preferring a token-bound tenant (U1.1) over the
/// client-supplied `X-Agentflow-Tenant` header, and rejecting a header
/// that conflicts with the token binding rather than honoring it. See the
/// module doc for the full precedence rule.
pub async fn extract_tenant_id(mut request: Request, next: Next) -> Result<Response, ApiError> {
  let claimed = request
    .headers()
    .get(&TENANT_HEADER)
    .and_then(|value| value.to_str().ok())
    .filter(|s| !s.trim().is_empty())
    .map(|s| s.trim().to_string());

  let bound_tenant = request
    .extensions()
    .get::<AuthenticatedTenant>()
    .and_then(|authenticated| authenticated.0.clone());

  let tenant = match bound_tenant {
    Some(bound) => {
      if let Some(claimed) = &claimed
        && claimed != &bound
      {
        return Err(ApiError::TenantMismatch(format!(
          "request claims tenant '{claimed}' via {} but the authenticated token is bound to \
           tenant '{bound}'",
          TENANT_HEADER.as_str()
        )));
      }
      TenantId(bound)
    }
    None => claimed
      .map(TenantId)
      .unwrap_or_else(TenantId::default_for_local),
  };
  request.extensions_mut().insert(tenant);
  Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_tenant_id_is_default() {
    assert_eq!(TenantId::default_for_local().as_str(), "default");
  }

  #[test]
  fn tenant_header_canonical_name_is_lowercased() {
    assert_eq!(TENANT_HEADER.as_str(), "x-agentflow-tenant");
  }
}
