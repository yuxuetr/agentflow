//! Signed-JWT identity flavour for worker admission (P10.16.1).
//!
//! This is the v1.x evolution of [`super::admission::WorkerCredential`]
//! beyond the bare PSK shipped in P5.5. JWT lets operators sit a real
//! identity provider in front of the worker fleet — Okta / Auth0 / Vault
//! / GCP Workload Identity all issue JWTs the control plane can verify
//! without holding the signing key (RS256/ES256), and operators who want
//! a single shared secret per (issuer, audience) tuple can still use the
//! simpler HS256 path.
//!
//! Design choices:
//!
//! - **PSK and JWT coexist.** The opt-in is per-worker via
//!   [`super::admission::WorkerAdmissionPolicy::jwt_workers`]. A worker
//!   listed there must present a JWT; a worker in `pre_shared_keys`
//!   uses PSK; a worker in both is a config error (the policy treats
//!   PSK as the authoritative answer to avoid surprising downgrades).
//! - **Algorithm support: HS256 + RS256.** ES256 is omitted in this
//!   first cut because operator demand hasn't surfaced and adding it is
//!   purely additive when it does. `jsonwebtoken::Algorithm` carries
//!   the full taxonomy if a future migration is wanted.
//! - **Key rotation via key pool.** Multiple verification keys per
//!   [`JwtPolicy`] support the standard "add new key → publish new
//!   tokens → drop old key" rotation pattern. The first key that
//!   verifies wins; rejection only fires when *every* key rejects.
//! - **Strict claim validation.** `iss` / `aud` / `sub` / `exp` are all
//!   mandatory. `iat` is recorded if present (not enforced); `nbf` is
//!   rejected if in the future. `sub` must equal the presented
//!   `worker_id` — this is the link that prevents one worker's token
//!   from being replayed by another.
//! - **Leeway.** Configurable clock-skew window (default 30s) applied
//!   to `exp` / `nbf`. Below the JWT RFC's typical guidance of 60s but
//!   tight enough that an expired token doesn't linger when the
//!   operator wants to forcibly rotate.
//!
//! This module is intentionally transport-agnostic: the gRPC adapter
//! maps a JWT failure to `tonic::Status::permission_denied` the same
//! way it maps a PSK failure, via the existing `AdmissionError` enum
//! (we fold JWT failures under `InvalidCredential` with a precise
//! reason so the operator-facing log line tells them what went wrong
//! without leaking the token shape).
//!
//! Stability tier: **experimental**, matching the rest of the worker
//! admission surface (see `docs/STABILITY.md`).

use std::collections::HashSet;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// One verification key in a [`JwtPolicy::keys`] pool. Each entry
/// carries the algorithm + key material; the policy tries them in
/// order and accepts the first that succeeds.
#[derive(Debug, Clone)]
pub enum JwtVerificationKey {
  /// HMAC with SHA-256. `secret` is shared between issuer and verifier
  /// — appropriate for self-hosted single-operator deployments where
  /// the same operator administers both the signing side and the
  /// control plane.
  Hs256 { secret: Vec<u8> },
  /// RSASSA-PKCS1-v1_5 with SHA-256. `public_key_pem` is the PEM-
  /// encoded RSA public key matching the IdP's signing key — the
  /// control plane never sees the private key. Appropriate for
  /// production IdPs (Okta / Auth0 / Vault / GCP Workload Identity).
  Rs256 { public_key_pem: String },
}

/// Operator-supplied JWT verification policy.
#[derive(Debug, Clone)]
pub struct JwtPolicy {
  /// Required `iss` claim — exact-match. The signing IdP's identifier.
  pub issuer: String,
  /// Required `aud` claim — exact-match. Typically the worker fleet
  /// name (e.g. `"agentflow-workers-prod"`).
  pub audience: String,
  /// Pool of verification keys. At least one must verify the token's
  /// signature *and* match its `alg` header for the token to be
  /// accepted. Multiple entries enable rotation without downtime.
  pub keys: Vec<JwtVerificationKey>,
  /// Clock-skew tolerance applied to `exp` and `nbf` (seconds).
  /// Defaults to 30s when not specified.
  pub leeway_seconds: u64,
}

impl JwtPolicy {
  pub fn new(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
    Self {
      issuer: issuer.into(),
      audience: audience.into(),
      keys: Vec::new(),
      leeway_seconds: 30,
    }
  }

  pub fn with_hs256_secret(mut self, secret: impl Into<Vec<u8>>) -> Self {
    self.keys.push(JwtVerificationKey::Hs256 {
      secret: secret.into(),
    });
    self
  }

  pub fn with_rs256_pem(mut self, public_key_pem: impl Into<String>) -> Self {
    self.keys.push(JwtVerificationKey::Rs256 {
      public_key_pem: public_key_pem.into(),
    });
    self
  }

  pub fn with_leeway_seconds(mut self, leeway_seconds: u64) -> Self {
    self.leeway_seconds = leeway_seconds;
    self
  }
}

/// Claims the control plane requires from a worker JWT. Extra claims
/// the IdP includes are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerJwtClaims {
  /// Issuer. Exact-match against [`JwtPolicy::issuer`].
  pub iss: String,
  /// Audience. Some IdPs emit `aud` as a string, others as a string
  /// array. The custom deserializer below accepts both shapes.
  #[serde(deserialize_with = "deserialize_audience")]
  pub aud: Vec<String>,
  /// Subject — must match the presented `worker_id`.
  pub sub: String,
  /// Expiry (seconds since epoch).
  pub exp: i64,
  /// Issued-at (seconds since epoch). Optional.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub iat: Option<i64>,
  /// Not-before (seconds since epoch). Optional.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub nbf: Option<i64>,
}

fn deserialize_audience<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  use serde::de::Error;
  let value = serde_json::Value::deserialize(deserializer)?;
  match value {
    serde_json::Value::String(s) => Ok(vec![s]),
    serde_json::Value::Array(items) => items
      .into_iter()
      .map(|v| match v {
        serde_json::Value::String(s) => Ok(s),
        other => Err(Error::custom(format!(
          "aud array entries must be strings, got {other:?}"
        ))),
      })
      .collect(),
    other => Err(Error::custom(format!(
      "aud must be a string or array of strings, got {other:?}"
    ))),
  }
}

/// Reasons JWT verification can fail. Mapped to
/// [`super::admission::AdmissionError::InvalidCredential`] with a
/// precise `reason` at the policy layer.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum JwtVerifyError {
  #[error("policy has no verification keys configured")]
  NoKeys,
  #[error("token signature did not verify against any configured key")]
  SignatureMismatch,
  #[error("token claims rejected: {reason}")]
  ClaimsRejected { reason: String },
  #[error("token issuer mismatch (expected {expected:?}, got {actual:?})")]
  IssuerMismatch { expected: String, actual: String },
  #[error("token audience mismatch (expected {expected:?}, none of {actual:?})")]
  AudienceMismatch {
    expected: String,
    actual: Vec<String>,
  },
  #[error("token subject mismatch (expected worker {expected:?}, got {actual:?})")]
  SubjectMismatch { expected: String, actual: String },
  #[error("token expired at {exp} (now {now}, leeway {leeway}s)")]
  Expired { exp: i64, now: i64, leeway: u64 },
  #[error("token not yet valid (nbf {nbf}, now {now}, leeway {leeway}s)")]
  NotYetValid { nbf: i64, now: i64, leeway: u64 },
  #[error("token malformed: {reason}")]
  Malformed { reason: String },
}

/// Verify a presented JWT against the policy for the given
/// `expected_subject`. Returns the parsed claims on success so the
/// caller can log metadata (issued-at, etc.) without re-parsing.
///
/// The function does its own claim validation rather than rely solely
/// on `jsonwebtoken::Validation` so the error messages stay actionable
/// (the library returns generic `InvalidIssuer` etc. without the
/// expected vs actual values an operator needs to debug).
pub fn verify_worker_jwt(
  token: &str,
  policy: &JwtPolicy,
  expected_subject: &str,
) -> Result<WorkerJwtClaims, JwtVerifyError> {
  verify_worker_jwt_at(
    token,
    policy,
    expected_subject,
    chrono::Utc::now().timestamp(),
  )
}

/// Internal variant that takes the "now" timestamp explicitly so the
/// test suite can exercise expiry / nbf paths deterministically.
pub fn verify_worker_jwt_at(
  token: &str,
  policy: &JwtPolicy,
  expected_subject: &str,
  now_secs: i64,
) -> Result<WorkerJwtClaims, JwtVerifyError> {
  if policy.keys.is_empty() {
    return Err(JwtVerifyError::NoKeys);
  }

  let header = jsonwebtoken::decode_header(token).map_err(|err| JwtVerifyError::Malformed {
    reason: format!("could not parse JWT header: {err}"),
  })?;

  // Try every key whose algorithm matches the header. The first one
  // that successfully decodes wins; we tally rejections so the final
  // error reports "signature mismatch" only if *every* candidate key
  // rejected the signature (rather than e.g. "no matching alg").
  let mut algorithm_compatible_keys = 0usize;
  let mut last_decode_error: Option<jsonwebtoken::errors::Error> = None;

  for key in &policy.keys {
    let (alg, decoding_key) = match key {
      JwtVerificationKey::Hs256 { secret } => (Algorithm::HS256, DecodingKey::from_secret(secret)),
      JwtVerificationKey::Rs256 { public_key_pem } => {
        let decoded = match DecodingKey::from_rsa_pem(public_key_pem.as_bytes()) {
          Ok(k) => k,
          Err(err) => {
            // A malformed RSA PEM in the policy is an operator
            // misconfiguration, not a token-side failure. Surface it
            // as Malformed so doctor-style audits show "fix your
            // policy" instead of "reject the worker."
            return Err(JwtVerifyError::Malformed {
              reason: format!("policy contained invalid RSA public key PEM: {err}"),
            });
          }
        };
        (Algorithm::RS256, decoded)
      }
    };
    if header.alg != alg {
      continue;
    }
    algorithm_compatible_keys += 1;

    // We disable the library's claim validation entirely and run our
    // own below — the library's `invalid_issuer` / `invalid_audience`
    // errors don't carry the expected-vs-actual context, so the
    // operator-facing error messages are noticeably worse than the
    // ones we emit. Signature verification is the *only* thing we
    // delegate to the library here.
    let mut validation = Validation::new(alg);
    validation.validate_exp = false;
    validation.validate_nbf = false;
    validation.validate_aud = false;
    validation.required_spec_claims.clear();
    validation.leeway = 0;

    match decode::<WorkerJwtClaims>(token, &decoding_key, &validation) {
      Ok(data) => return validate_claims(data.claims, policy, expected_subject, now_secs),
      Err(err) => {
        last_decode_error = Some(err);
      }
    }
  }

  if algorithm_compatible_keys == 0 {
    return Err(JwtVerifyError::Malformed {
      reason: format!(
        "no policy key matches the token's `alg` header ({:?}); policy keys: {}",
        header.alg,
        policy
          .keys
          .iter()
          .map(|k| match k {
            JwtVerificationKey::Hs256 { .. } => "HS256",
            JwtVerificationKey::Rs256 { .. } => "RS256",
          })
          .collect::<Vec<_>>()
          .join(", ")
      ),
    });
  }

  // Every algorithm-compatible key rejected the signature. We don't
  // surface the raw library error since it can leak fragments of the
  // token in some versions; a stable "signature did not verify"
  // message is good enough for the operator.
  let _ = last_decode_error;
  Err(JwtVerifyError::SignatureMismatch)
}

fn validate_claims(
  claims: WorkerJwtClaims,
  policy: &JwtPolicy,
  expected_subject: &str,
  now_secs: i64,
) -> Result<WorkerJwtClaims, JwtVerifyError> {
  if claims.iss != policy.issuer {
    return Err(JwtVerifyError::IssuerMismatch {
      expected: policy.issuer.clone(),
      actual: claims.iss,
    });
  }
  // The token can declare multiple audiences; we accept if any one
  // matches the policy.audience. This matches the JWT RFC §4.1.3
  // intent.
  let audiences: HashSet<&str> = claims.aud.iter().map(String::as_str).collect();
  if !audiences.contains(policy.audience.as_str()) {
    return Err(JwtVerifyError::AudienceMismatch {
      expected: policy.audience.clone(),
      actual: claims.aud,
    });
  }
  if claims.sub != expected_subject {
    return Err(JwtVerifyError::SubjectMismatch {
      expected: expected_subject.to_string(),
      actual: claims.sub,
    });
  }
  let leeway = policy.leeway_seconds as i64;
  if claims.exp + leeway <= now_secs {
    return Err(JwtVerifyError::Expired {
      exp: claims.exp,
      now: now_secs,
      leeway: policy.leeway_seconds,
    });
  }
  if let Some(nbf) = claims.nbf
    && nbf - leeway > now_secs
  {
    return Err(JwtVerifyError::NotYetValid {
      nbf,
      now: now_secs,
      leeway: policy.leeway_seconds,
    });
  }
  Ok(claims)
}

#[cfg(test)]
mod tests {
  use super::*;
  use jsonwebtoken::{EncodingKey, Header, encode};

  fn sign_hs256(secret: &[u8], claims: &WorkerJwtClaims) -> String {
    encode(
      &Header::new(Algorithm::HS256),
      claims,
      &EncodingKey::from_secret(secret),
    )
    .expect("sign HS256 token")
  }

  fn claims_at(exp_offset_secs: i64, now: i64) -> WorkerJwtClaims {
    WorkerJwtClaims {
      iss: "test-issuer".into(),
      aud: vec!["agentflow-workers-prod".into()],
      sub: "worker-a".into(),
      exp: now + exp_offset_secs,
      iat: Some(now),
      nbf: None,
    }
  }

  fn policy_with_secret(secret: &[u8]) -> JwtPolicy {
    JwtPolicy::new("test-issuer", "agentflow-workers-prod").with_hs256_secret(secret)
  }

  #[test]
  fn hs256_round_trip_accepts_valid_token() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"super-secret");
    let token = sign_hs256(b"super-secret", &claims_at(300, now));
    let result = verify_worker_jwt_at(&token, &policy, "worker-a", now);
    assert!(result.is_ok(), "expected ok, got {result:?}");
  }

  #[test]
  fn empty_key_pool_errors_clearly() {
    let now = 1_700_000_000;
    let policy = JwtPolicy::new("issuer", "aud");
    let token = sign_hs256(b"secret", &claims_at(60, now));
    assert!(matches!(
      verify_worker_jwt_at(&token, &policy, "worker-a", now),
      Err(JwtVerifyError::NoKeys)
    ));
  }

  #[test]
  fn wrong_secret_is_signature_mismatch() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"server-side-secret");
    let token = sign_hs256(b"different-secret", &claims_at(300, now));
    assert!(matches!(
      verify_worker_jwt_at(&token, &policy, "worker-a", now),
      Err(JwtVerifyError::SignatureMismatch)
    ));
  }

  #[test]
  fn wrong_issuer_is_issuer_mismatch_not_signature() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret");
    let mut claims = claims_at(300, now);
    claims.iss = "evil-issuer".into();
    let token = sign_hs256(b"secret", &claims);
    match verify_worker_jwt_at(&token, &policy, "worker-a", now) {
      Err(JwtVerifyError::IssuerMismatch { expected, actual }) => {
        assert_eq!(expected, "test-issuer");
        assert_eq!(actual, "evil-issuer");
      }
      other => panic!("expected IssuerMismatch, got {other:?}"),
    }
  }

  #[test]
  fn wrong_audience_is_audience_mismatch() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret");
    let mut claims = claims_at(300, now);
    claims.aud = vec!["agentflow-workers-staging".into()];
    let token = sign_hs256(b"secret", &claims);
    match verify_worker_jwt_at(&token, &policy, "worker-a", now) {
      Err(JwtVerifyError::AudienceMismatch { expected, .. }) => {
        assert_eq!(expected, "agentflow-workers-prod");
      }
      other => panic!("expected AudienceMismatch, got {other:?}"),
    }
  }

  #[test]
  fn multi_audience_token_is_accepted_when_one_matches() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret");
    let mut claims = claims_at(300, now);
    claims.aud = vec!["other-aud".into(), "agentflow-workers-prod".into()];
    let token = sign_hs256(b"secret", &claims);
    let result = verify_worker_jwt_at(&token, &policy, "worker-a", now);
    assert!(result.is_ok(), "expected ok, got {result:?}");
  }

  #[test]
  fn subject_must_match_presented_worker_id() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret");
    let token = sign_hs256(b"secret", &claims_at(300, now));
    match verify_worker_jwt_at(&token, &policy, "worker-b", now) {
      Err(JwtVerifyError::SubjectMismatch { expected, actual }) => {
        assert_eq!(expected, "worker-b");
        assert_eq!(actual, "worker-a");
      }
      other => panic!("expected SubjectMismatch, got {other:?}"),
    }
  }

  #[test]
  fn expired_token_rejected_after_leeway() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret").with_leeway_seconds(10);
    // exp + leeway = now + (-30) + 10 = now - 20 → expired
    let token = sign_hs256(b"secret", &claims_at(-30, now));
    match verify_worker_jwt_at(&token, &policy, "worker-a", now) {
      Err(JwtVerifyError::Expired { exp, leeway, .. }) => {
        assert_eq!(exp, now - 30);
        assert_eq!(leeway, 10);
      }
      other => panic!("expected Expired, got {other:?}"),
    }
  }

  #[test]
  fn just_expired_within_leeway_accepted() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret").with_leeway_seconds(60);
    // exp = now - 10; leeway = 60 → token is still valid for 50 more
    // seconds under the operator's clock-skew window.
    let token = sign_hs256(b"secret", &claims_at(-10, now));
    let result = verify_worker_jwt_at(&token, &policy, "worker-a", now);
    assert!(result.is_ok(), "expected ok within leeway, got {result:?}");
  }

  #[test]
  fn nbf_in_future_rejected() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret").with_leeway_seconds(5);
    let mut claims = claims_at(300, now);
    claims.nbf = Some(now + 60);
    let token = sign_hs256(b"secret", &claims);
    match verify_worker_jwt_at(&token, &policy, "worker-a", now) {
      Err(JwtVerifyError::NotYetValid { nbf, .. }) => {
        assert_eq!(nbf, now + 60);
      }
      other => panic!("expected NotYetValid, got {other:?}"),
    }
  }

  #[test]
  fn key_rotation_pool_accepts_either_key() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"new-secret").with_hs256_secret(b"old-secret".to_vec());
    // Token signed by the OLD key still verifies because the policy
    // carries it during the rotation overlap window.
    let token = sign_hs256(b"old-secret", &claims_at(300, now));
    let result = verify_worker_jwt_at(&token, &policy, "worker-a", now);
    assert!(result.is_ok(), "old-key token must verify, got {result:?}");
  }

  #[test]
  fn malformed_token_surfaced_as_malformed() {
    let now = 1_700_000_000;
    let policy = policy_with_secret(b"secret");
    match verify_worker_jwt_at("not.a.token", &policy, "worker-a", now) {
      Err(JwtVerifyError::Malformed { .. }) => {}
      other => panic!("expected Malformed, got {other:?}"),
    }
  }

  #[test]
  fn aud_string_form_deserializes_as_single_entry_vec() {
    // Many IdPs emit `aud` as a bare string (single audience). The
    // custom deserializer below promotes that to a one-element Vec so
    // the rest of the validator doesn't need to branch.
    let now = 1_700_000_000;
    let raw = format!(
      r#"{{"iss":"test-issuer","aud":"agentflow-workers-prod","sub":"worker-a","exp":{}}}"#,
      now + 300
    );
    let parsed: WorkerJwtClaims = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.aud, vec!["agentflow-workers-prod"]);
  }

  #[test]
  fn aud_array_form_deserializes_unchanged() {
    let now = 1_700_000_000;
    let raw = format!(
      r#"{{"iss":"test-issuer","aud":["a","b"],"sub":"worker-a","exp":{}}}"#,
      now + 300
    );
    let parsed: WorkerJwtClaims = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed.aud, vec!["a", "b"]);
  }
}
