//! Tenant binding middleware (P2.6).
//!
//! Every `/v1/*` request runs through `extract_tenant_id`, which reads the
//! `X-Agentflow-Tenant` header and injects a typed [`TenantId`] extension.
//! Handlers extract it via `Extension(tenant): Extension<TenantId>` and use
//! it to scope DB reads + writes.
//!
//! Default tenant is `"default"` so single-tenant local-dev deployments
//! stay zero-config. When JWT / OIDC lands, the same middleware will
//! prefer a token-bound tenant claim and fall back to the header.

use axum::{extract::Request, http::HeaderName, middleware::Next, response::Response};

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

/// Axum middleware that pulls `X-Agentflow-Tenant` off the request and
/// inserts a [`TenantId`] extension. Missing or malformed headers fall
/// back to the canonical `"default"` tenant — single-tenant local-dev
/// callers never have to set the header.
pub async fn extract_tenant_id(mut request: Request, next: Next) -> Response {
  let tenant = request
    .headers()
    .get(&TENANT_HEADER)
    .and_then(|value| value.to_str().ok())
    .filter(|s| !s.trim().is_empty())
    .map(|s| TenantId(s.trim().to_string()))
    .unwrap_or_else(TenantId::default_for_local);
  request.extensions_mut().insert(tenant);
  next.run(request).await
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
