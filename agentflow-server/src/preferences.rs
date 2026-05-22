//! Durable UI preference routes (P6.4).
//!
//! Two routes:
//!
//! - `GET /v1/preferences` — returns every preference the active
//!   tenant has stored as `{ "preferences": { "<key>": <value>, ... } }`.
//!   Empty object when the tenant has none.
//! - `PUT /v1/preferences` — body `{ "preferences": { "<key>": <value>,
//!   ... } }`. Each entry is upserted atomically (single transaction);
//!   `updated_at` is stamped server-side.
//!
//! Both routes are tenant-scoped via the `X-Agentflow-Tenant` header
//! (P2.6) and gated by the same auth middleware as the rest of `/v1/*`.
//!
//! Token-shape rejection: every incoming value is screened for patterns
//! that look like an API token (Bearer headers, `sk-` prefixes, long
//! hex strings). Matching values are refused with 400 so an operator
//! can't accidentally upload credentials and expose them through the
//! list route.

use axum::{Extension, Json, extract::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::AppState;
use crate::error::{ApiError, JsonReq};
use crate::tenant::TenantId;
use agentflow_db::UserPreferenceRepo;

#[derive(Debug, Deserialize, Serialize)]
pub struct PreferencesEnvelope {
  /// Map keyed by preference name (e.g. `"theme"`, `"event_filter"`).
  pub preferences: BTreeMap<String, serde_json::Value>,
}

/// `GET /v1/preferences` — list every preference the active tenant has
/// stored. Returns an empty `preferences` object for first-time callers.
pub async fn list_preferences(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
) -> Result<Json<PreferencesEnvelope>, ApiError> {
  let rows = state
    .repos
    .user_preferences
    .list_for_tenant(tenant.as_str())
    .await?;
  let mut preferences = BTreeMap::new();
  for row in rows {
    preferences.insert(row.key, row.value);
  }
  Ok(Json(PreferencesEnvelope { preferences }))
}

/// `PUT /v1/preferences` — upsert every entry in the body's
/// `preferences` map. The entire batch is one transaction; a single
/// rejected value aborts the whole call so callers don't observe a
/// half-applied state.
///
/// Rules:
///
///   - Each key must match `^[a-zA-Z0-9_.\-:]{1,128}$`. Unbounded /
///     symbol-heavy keys would let a caller cram arbitrary text into
///     the index — and we want predictable lookup.
///   - Each value must serialise to ≤ 16 KiB of JSON. UI preferences
///     are small by nature; anything larger is almost certainly the
///     wrong endpoint.
///   - Values are screened for token-shaped strings (see
///     `looks_like_token`). Matches → 400.
pub async fn put_preferences(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
  JsonReq(body): JsonReq<PreferencesEnvelope>,
) -> Result<Json<PreferencesEnvelope>, ApiError> {
  let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
  for (key, value) in body.preferences {
    if !is_valid_preference_key(&key) {
      return Err(ApiError::BadRequest(format!(
        "preference key '{key}' is invalid; must match ^[a-zA-Z0-9_.\\-:]{{1,128}}$"
      )));
    }
    let serialized =
      serde_json::to_vec(&value).map_err(|err| ApiError::BadRequest(err.to_string()))?;
    if serialized.len() > MAX_VALUE_BYTES {
      return Err(ApiError::BadRequest(format!(
        "preference value for '{key}' is too large ({} bytes; max {MAX_VALUE_BYTES})",
        serialized.len()
      )));
    }
    if let Some(reason) = looks_like_token(&value) {
      return Err(ApiError::BadRequest(format!(
        "preference value for '{key}' looks like a secret token ({reason}); refusing to store"
      )));
    }
    entries.push((key, value));
  }

  state
    .repos
    .user_preferences
    .upsert_many(tenant.as_str(), entries)
    .await?;

  // Read back so the response reflects committed state (catches the
  // edge case where a PUT only contained existing keys with no new
  // changes — UI sees the persisted set).
  let rows = state
    .repos
    .user_preferences
    .list_for_tenant(tenant.as_str())
    .await?;
  let mut preferences = BTreeMap::new();
  for row in rows {
    preferences.insert(row.key, row.value);
  }
  Ok(Json(PreferencesEnvelope { preferences }))
}

const MAX_VALUE_BYTES: usize = 16 * 1024;

fn is_valid_preference_key(key: &str) -> bool {
  let len = key.len();
  if !(1..=128).contains(&len) {
    return false;
  }
  key
    .bytes()
    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-' || b == b':')
}

/// Defensive heuristics for catching token-shaped strings on the way
/// in. Conservative on purpose — false positives are easier to debug
/// than a leaked secret. Returns `Some(reason)` when the value looks
/// like a token; `None` for safe values.
fn looks_like_token(value: &serde_json::Value) -> Option<&'static str> {
  let serde_json::Value::String(s) = value else {
    return None;
  };
  let trimmed = s.trim();
  if trimmed.is_empty() {
    return None;
  }
  let lower = trimmed.to_lowercase();
  if lower.starts_with("bearer ") {
    return Some("Bearer-prefixed");
  }
  // OpenAI / Anthropic / etc. common prefixes.
  for prefix in ["sk-", "sk_", "ant-", "api_", "ghp_", "ghs_", "gho_"] {
    if trimmed.starts_with(prefix) && trimmed.len() >= prefix.len() + 16 {
      return Some("API-key-shaped");
    }
  }
  // Long uniform hex strings (32+ chars).
  if trimmed.len() >= 32 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
    return Some("long hex digest");
  }
  // Long uniform base64-ish strings.
  if trimmed.len() >= 40
    && trimmed.chars().all(|c| {
      c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '_' || c == '-' || c == '='
    })
    && !trimmed.chars().any(|c| c == ' ')
  {
    let alpha = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    let digit = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
    if alpha > 0 && digit > 0 {
      return Some("long opaque token-shaped string");
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn key_validator_rejects_problem_characters() {
    assert!(is_valid_preference_key("theme"));
    assert!(is_valid_preference_key("ui.run-list.page-size"));
    assert!(is_valid_preference_key("filter:default"));
    assert!(!is_valid_preference_key(""));
    assert!(!is_valid_preference_key("with space"));
    assert!(!is_valid_preference_key("with$dollar"));
    assert!(!is_valid_preference_key(&"x".repeat(129)));
  }

  #[test]
  fn token_screen_catches_obvious_secrets() {
    assert!(looks_like_token(&json!("Bearer abcdef1234567890abcdef")).is_some());
    assert!(looks_like_token(&json!("sk-abcdefghijklmnopqrstuvwxyz0123456789")).is_some());
    assert!(
      looks_like_token(&json!(
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
      ))
      .is_some()
    );
    assert!(
      looks_like_token(&json!("ghp_a1b2c3d4e5f6g7h8i9j0klmn1234567890")).is_some(),
      "GitHub personal-access-token prefix",
    );
    assert!(
      looks_like_token(&json!("abcdef1234567890ABCDEF1234567890abcdef1234567890")).is_some(),
      "long opaque alphanumeric string",
    );
  }

  #[test]
  fn token_screen_accepts_normal_values() {
    assert!(looks_like_token(&json!("dark")).is_none());
    assert!(looks_like_token(&json!("filter:status=running")).is_none());
    assert!(looks_like_token(&json!(25)).is_none());
    assert!(looks_like_token(&json!({"theme": "dark"})).is_none());
    assert!(looks_like_token(&json!("")).is_none());
    // Short hex-looking values (UUID short-form) — under threshold.
    assert!(looks_like_token(&json!("abc1234")).is_none());
  }
}
