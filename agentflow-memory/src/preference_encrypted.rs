//! Age-based encryption-at-rest wrapper for the [`PreferenceStore`]
//! trait (P10.7.2).
//!
//! Wraps an inner [`PreferenceStore`] (typically [`SqlitePreferenceStore`])
//! and transparently encrypts the `value` payload on write + decrypts
//! on read using a single-user X25519 identity. The keys (tenant id,
//! user id, key string) stay plaintext on disk because the inner store
//! needs them for queries — only the *value* is opaque to anyone
//! without the identity file.
//!
//! ## On-disk shape
//!
//! Encrypted values are stored in the inner store's existing `value`
//! column as a JSON string with the form:
//!
//! ```text
//! "age:v1:<base64(age-ciphertext)>"
//! ```
//!
//! The `age:v1:` marker prefix lets readers verify they're looking at
//! ciphertext from this store rather than plaintext from
//! [`SqlitePreferenceStore`]. Future migrations can bump the version
//! suffix without breaking back-compat.
//!
//! ## KMS scope
//!
//! Single-user, local-only — the identity file lives at a caller-chosen
//! path (convention: `~/.agentflow/identity.age`, mode 0600). No cloud
//! KMS, no envelope re-keying, no per-record key wrapping. Sufficient
//! for the local profile per `docs/MEMORY_LAYERING.md` §3 "Encrypted at
//! rest is optional; the trait should support it but a plaintext
//! default is acceptable for the local profile."
//!
//! Cloud KMS / multi-user / envelope re-keying are deferred to a v2
//! design conversation per `docs/ROADMAP_v2.md` Theme B.
//!
//! ## Threat model
//!
//! Protects the `value` payload from someone with read access to the
//! SQLite file but not the identity file. Does NOT protect against:
//!
//! - An attacker with both the DB and the identity (single-user posture).
//! - Memory inspection of a running agent process.
//! - Timing / size side-channels (ciphertext size leaks payload size).
//!
//! Pair with a host-level disk-encryption story (FileVault, LUKS, etc.)
//! for defense-in-depth.

use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use age::secrecy::ExposeSecret;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

use crate::MemoryError;
use crate::layer::{PreferenceScope, PreferenceStore, PreferenceValue};
use crate::preference::SqlitePreferenceStore;

/// Marker prefix on every encrypted JSON-string value. Carries a
/// version suffix so future ciphertext-format changes can be
/// distinguished without a schema migration.
const VALUE_MARKER: &str = "age:v1:";

/// Age-encryption wrapper around any [`PreferenceStore`].
///
/// Defaults to wrapping [`SqlitePreferenceStore`] when no inner type
/// is specified.
pub struct AgeEncryptedPreferenceStore<S: PreferenceStore = SqlitePreferenceStore> {
  inner: S,
  identity: age::x25519::Identity,
  recipient: age::x25519::Recipient,
}

impl AgeEncryptedPreferenceStore<SqlitePreferenceStore> {
  /// Build a new encrypted store backed by the SQLite preference store
  /// at `db_path`, with the X25519 identity loaded from
  /// `identity_path`. Both paths must already exist — call
  /// [`generate_identity_file`] once to bootstrap a new identity.
  pub async fn open_sqlite<P: AsRef<Path>, I: AsRef<Path>>(
    db_path: P,
    identity_path: I,
  ) -> Result<Self, MemoryError> {
    let identity = load_identity_file(identity_path.as_ref())?;
    let inner = SqlitePreferenceStore::open(db_path).await?;
    Ok(Self::new(inner, identity))
  }
}

impl<S: PreferenceStore> AgeEncryptedPreferenceStore<S> {
  /// Wrap an existing store with a pre-loaded identity. Mostly used in
  /// tests; production callers go through [`Self::open_sqlite`] which
  /// reads the identity from disk.
  pub fn new(inner: S, identity: age::x25519::Identity) -> Self {
    let recipient = identity.to_public();
    Self {
      inner,
      identity,
      recipient,
    }
  }

  /// Borrow the inner store. Useful for callers that need access to
  /// store-specific helpers (e.g. SQLite migration / pragma tuning)
  /// without going through the trait.
  pub fn inner(&self) -> &S {
    &self.inner
  }

  /// Test-only helper: produce the recipient string (public key) so
  /// callers can confirm a deployment is using the expected identity.
  /// `Display` on a Recipient renders the `age1...` form.
  pub fn recipient_string(&self) -> String {
    self.recipient.to_string()
  }
}

/// Generate a fresh X25519 identity and persist it to `path` with mode
/// 0600 (Unix). Fails if `path` already exists — never silently
/// clobbers an existing key. Returns the generated identity so callers
/// can immediately construct a store without re-reading the file.
pub fn generate_identity_file<P: AsRef<Path>>(
  path: P,
) -> Result<age::x25519::Identity, MemoryError> {
  let path = path.as_ref();
  if path.exists() {
    return Err(MemoryError::StorageError(format!(
      "refusing to overwrite existing identity file at {}",
      path.display()
    )));
  }
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| {
      MemoryError::StorageError(format!(
        "creating identity-file parent dir {}: {e}",
        parent.display()
      ))
    })?;
  }
  let identity = age::x25519::Identity::generate();
  let secret = identity.to_string();
  std::fs::write(path, secret.expose_secret().as_bytes()).map_err(|e| {
    MemoryError::StorageError(format!(
      "writing identity file {}: {e}",
      path.display()
    ))
  })?;
  // 0600 on Unix so a casual `ls` doesn't leak the secret. No-op on
  // Windows; the host disk-encryption story is the operator's
  // responsibility there.
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
      MemoryError::StorageError(format!("chmod 0600 on {}: {e}", path.display()))
    })?;
  }
  Ok(identity)
}

/// Load an X25519 identity from `path`. Expects the canonical
/// single-line `AGE-SECRET-KEY-1...` representation.
pub fn load_identity_file<P: AsRef<Path>>(
  path: P,
) -> Result<age::x25519::Identity, MemoryError> {
  let path = path.as_ref();
  let content = std::fs::read_to_string(path).map_err(|e| {
    MemoryError::StorageError(format!(
      "reading identity file {}: {e}",
      path.display()
    ))
  })?;
  age::x25519::Identity::from_str(content.trim()).map_err(|e| {
    MemoryError::StorageError(format!(
      "parsing identity file {}: {e}",
      path.display()
    ))
  })
}

/// Encrypt one JSON value to the marker-prefixed string the inner
/// store persists.
fn encrypt_value(
  value: &Value,
  recipient: &age::x25519::Recipient,
) -> Result<String, MemoryError> {
  let plaintext = serde_json::to_vec(value)
    .map_err(|e| MemoryError::StorageError(format!("serialise plaintext: {e}")))?;
  let encryptor = age::Encryptor::with_recipients(std::iter::once(recipient as &dyn age::Recipient))
    .map_err(|e| MemoryError::StorageError(format!("age encryptor init: {e}")))?;
  let mut ciphertext = Vec::with_capacity(plaintext.len() + 256);
  let mut writer = encryptor
    .wrap_output(&mut ciphertext)
    .map_err(|e| MemoryError::StorageError(format!("age wrap: {e}")))?;
  writer
    .write_all(&plaintext)
    .map_err(|e| MemoryError::StorageError(format!("age write: {e}")))?;
  writer
    .finish()
    .map_err(|e| MemoryError::StorageError(format!("age finish: {e}")))?;
  let encoded = BASE64.encode(&ciphertext);
  Ok(format!("{VALUE_MARKER}{encoded}"))
}

/// Reverse of [`encrypt_value`]. Verifies the marker prefix so a
/// plaintext-from-`SqlitePreferenceStore` row can't be mis-read as a
/// failed decryption.
fn decrypt_value(
  stored: &Value,
  identity: &age::x25519::Identity,
) -> Result<Value, MemoryError> {
  let stored_str = stored.as_str().ok_or_else(|| {
    MemoryError::StorageError(
      "encrypted preference store expected a JSON string value; got a non-string"
        .to_string(),
    )
  })?;
  let payload = stored_str.strip_prefix(VALUE_MARKER).ok_or_else(|| {
    MemoryError::StorageError(format!(
      "value missing {VALUE_MARKER} marker prefix; \
       refusing to read what may be plaintext (use SqlitePreferenceStore for plaintext rows)"
    ))
  })?;
  let ciphertext = BASE64
    .decode(payload)
    .map_err(|e| MemoryError::StorageError(format!("base64 decode: {e}")))?;
  let decryptor = age::Decryptor::new(&ciphertext[..])
    .map_err(|e| MemoryError::StorageError(format!("age decryptor: {e}")))?;
  let mut plaintext = Vec::new();
  let mut reader = decryptor
    .decrypt(std::iter::once(identity as &dyn age::Identity))
    .map_err(|e| MemoryError::StorageError(format!("age decrypt: {e}")))?;
  reader
    .read_to_end(&mut plaintext)
    .map_err(|e| MemoryError::StorageError(format!("age read: {e}")))?;
  serde_json::from_slice(&plaintext)
    .map_err(|e| MemoryError::StorageError(format!("plaintext json parse: {e}")))
}

#[async_trait]
impl<S: PreferenceStore> PreferenceStore for AgeEncryptedPreferenceStore<S> {
  async fn get_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<Option<PreferenceValue>, MemoryError> {
    let Some(pv) = self.inner.get_preference(scope, key).await? else {
      return Ok(None);
    };
    let decrypted = decrypt_value(&pv.value, &self.identity)?;
    Ok(Some(PreferenceValue {
      value: decrypted,
      updated_at: pv.updated_at,
      version: pv.version,
    }))
  }

  async fn put_preference(
    &mut self,
    scope: &PreferenceScope,
    key: &str,
    value: Value,
  ) -> Result<(), MemoryError> {
    let ciphertext_string = encrypt_value(&value, &self.recipient)?;
    self
      .inner
      .put_preference(scope, key, Value::String(ciphertext_string))
      .await
  }

  async fn delete_preference(
    &mut self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<(), MemoryError> {
    // No crypto involvement on delete — the inner store already keys
    // on `(tenant, user, key)` (all plaintext).
    self.inner.delete_preference(scope, key).await
  }

  async fn list_preferences(
    &self,
    scope: &PreferenceScope,
  ) -> Result<Vec<(String, PreferenceValue)>, MemoryError> {
    let raw = self.inner.list_preferences(scope).await?;
    let mut out = Vec::with_capacity(raw.len());
    for (k, pv) in raw {
      let decrypted = decrypt_value(&pv.value, &self.identity)?;
      out.push((
        k,
        PreferenceValue {
          value: decrypted,
          updated_at: pv.updated_at,
          version: pv.version,
        },
      ));
    }
    Ok(out)
  }

  async fn prune_older_than(&mut self, older_than: Duration) -> Result<u64, MemoryError> {
    // Prune is a metadata-only operation (`updated_at < cutoff`); the
    // inner store handles it without needing to touch ciphertext.
    self.inner.prune_older_than(older_than).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layer::PreferenceScope;
  use serde_json::json;

  async fn fresh_encrypted_store() -> AgeEncryptedPreferenceStore<SqlitePreferenceStore> {
    let inner = SqlitePreferenceStore::in_memory()
      .await
      .expect("in-memory sqlite");
    let identity = age::x25519::Identity::generate();
    AgeEncryptedPreferenceStore::new(inner, identity)
  }

  #[tokio::test]
  async fn put_get_roundtrip_preserves_value() {
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "tone", json!("friendly"))
      .await
      .unwrap();
    let pv = store
      .get_preference(&scope, "tone")
      .await
      .unwrap()
      .expect("must exist");
    assert_eq!(pv.value, json!("friendly"));
    assert_eq!(pv.version, 1);
  }

  #[tokio::test]
  async fn put_get_roundtrip_preserves_complex_json() {
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    let payload = json!({
      "tone": "friendly",
      "preferences": {
        "topics": ["rust", "agentflow"],
        "max_response_length": 2000
      },
      "history_summary": "Daisy is working on AgentFlow's encryption layer."
    });
    store
      .put_preference(&scope, "context", payload.clone())
      .await
      .unwrap();
    let pv = store
      .get_preference(&scope, "context")
      .await
      .unwrap()
      .expect("must exist");
    assert_eq!(pv.value, payload);
  }

  #[tokio::test]
  async fn ciphertext_is_not_recognizable_as_the_plaintext() {
    // Pin the core "at rest" contract: the inner store sees ciphertext
    // bytes, NOT the plaintext payload. We read the inner row directly
    // (via the underlying trait) and assert the stored value carries
    // the marker prefix + does not contain the plaintext substring.
    let inner = SqlitePreferenceStore::in_memory().await.unwrap();
    let identity = age::x25519::Identity::generate();
    let mut store = AgeEncryptedPreferenceStore::new(inner, identity);
    let scope = PreferenceScope::local("alice");
    let plaintext_marker = "EXTREMELY_DISTINCTIVE_PLAINTEXT_STRING";
    store
      .put_preference(&scope, "secret", json!(plaintext_marker))
      .await
      .unwrap();
    // Reach through to the inner store with its own get to see the
    // raw on-disk shape (which would be the encrypted JSON string).
    let raw = store
      .inner()
      .get_preference(&scope, "secret")
      .await
      .unwrap()
      .expect("inner row must exist");
    let raw_str = raw.value.as_str().expect("inner stores JSON string");
    assert!(
      raw_str.starts_with(VALUE_MARKER),
      "inner value must carry marker prefix; got: {raw_str}"
    );
    assert!(
      !raw_str.contains(plaintext_marker),
      "inner value must not contain plaintext; got: {raw_str}"
    );
  }

  #[tokio::test]
  async fn put_twice_increments_version_through_the_wrapper() {
    // The wrapper doesn't manage versions itself — it delegates to
    // the inner store. Pin that the version-increment semantics
    // survive the encryption indirection (the inner row's
    // `updated_at` + `version` are not encrypted).
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "tone", json!("friendly"))
      .await
      .unwrap();
    store
      .put_preference(&scope, "tone", json!("formal"))
      .await
      .unwrap();
    let pv = store
      .get_preference(&scope, "tone")
      .await
      .unwrap()
      .unwrap();
    assert_eq!(pv.version, 2);
    assert_eq!(pv.value, json!("formal"));
  }

  #[tokio::test]
  async fn delete_then_get_returns_none() {
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "k", json!(1))
      .await
      .unwrap();
    store.delete_preference(&scope, "k").await.unwrap();
    assert!(
      store
        .get_preference(&scope, "k")
        .await
        .unwrap()
        .is_none()
    );
  }

  #[tokio::test]
  async fn list_decrypts_every_row() {
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    store.put_preference(&scope, "a", json!("a-val")).await.unwrap();
    store.put_preference(&scope, "b", json!("b-val")).await.unwrap();
    store.put_preference(&scope, "c", json!("c-val")).await.unwrap();
    let mut rows = store.list_preferences(&scope).await.unwrap();
    rows.sort_by(|x, y| x.0.cmp(&y.0));
    let pairs: Vec<(String, Value)> =
      rows.into_iter().map(|(k, pv)| (k, pv.value)).collect();
    assert_eq!(
      pairs,
      vec![
        ("a".into(), json!("a-val")),
        ("b".into(), json!("b-val")),
        ("c".into(), json!("c-val")),
      ]
    );
  }

  #[tokio::test]
  async fn decrypt_with_wrong_identity_fails() {
    // Two distinct identities → ciphertext encrypted to A is
    // unreadable by B. This is the core encryption-at-rest
    // contract; without this assertion we'd silently return
    // gibberish on a stolen DB without the right identity.
    let inner = SqlitePreferenceStore::in_memory().await.unwrap();
    let identity_a = age::x25519::Identity::generate();
    let identity_b = age::x25519::Identity::generate();
    let mut writer = AgeEncryptedPreferenceStore::new(inner, identity_a);
    let scope = PreferenceScope::local("alice");
    writer
      .put_preference(&scope, "k", json!("plaintext"))
      .await
      .unwrap();
    // Re-open the same DB through a different wrapper with the
    // wrong identity. We do this by swapping the identity field
    // — the inner DB is shared with the writer above (in-memory
    // SQLite, single connection pool).
    //
    // Realistically we'd need a way to share the same `SqlitePool`
    // between two wrappers. Here we just construct a NEW wrapper
    // around the same writer's inner store via Mutex-free
    // single-threaded re-use: deconstruct writer + reuse inner.
    let AgeEncryptedPreferenceStore { inner, .. } = writer;
    let reader = AgeEncryptedPreferenceStore::new(inner, identity_b);
    let result = reader.get_preference(&scope, "k").await;
    assert!(
      result.is_err(),
      "reader with wrong identity must fail; got: {result:?}"
    );
    let err = result.unwrap_err().to_string();
    assert!(
      err.to_ascii_lowercase().contains("decrypt")
        || err.to_ascii_lowercase().contains("age"),
      "diagnostic should mention age/decrypt; got: {err}"
    );
  }

  #[tokio::test]
  async fn get_rejects_plaintext_row_missing_marker() {
    // If someone writes plaintext directly into the inner store
    // (e.g. via a stale `SqlitePreferenceStore` handle), the
    // encrypted-wrapper read MUST refuse — silently returning the
    // plaintext as if it were decrypted would be a security
    // regression.
    let mut inner = SqlitePreferenceStore::in_memory().await.unwrap();
    let scope = PreferenceScope::local("alice");
    inner
      .put_preference(&scope, "leaked", json!("naked-plaintext"))
      .await
      .unwrap();
    let identity = age::x25519::Identity::generate();
    let store = AgeEncryptedPreferenceStore::new(inner, identity);
    let err = store
      .get_preference(&scope, "leaked")
      .await
      .expect_err("must fail on plaintext row");
    assert!(
      err.to_string().contains(VALUE_MARKER),
      "diagnostic should mention the missing marker; got: {err}"
    );
  }

  #[tokio::test]
  async fn prune_passthrough_uses_inner_store() {
    let mut store = fresh_encrypted_store().await;
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "k", json!("v"))
      .await
      .unwrap();
    // A 0-duration cutoff prunes nothing on a row that's <1s old,
    // confirming the call reaches the inner store and the
    // wrapper isn't intercepting the prune path.
    let removed = store.prune_older_than(Duration::from_secs(60)).await.unwrap();
    assert_eq!(removed, 0);
    assert!(
      store.get_preference(&scope, "k").await.unwrap().is_some()
    );
  }

  #[test]
  fn generate_and_load_identity_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.age");
    let generated = generate_identity_file(&path).expect("generate ok");
    let loaded = load_identity_file(&path).expect("load ok");
    // We can't directly compare Identity values; round-trip via
    // recipient (which has a stable `to_string` representation).
    assert_eq!(
      generated.to_public().to_string(),
      loaded.to_public().to_string()
    );
  }

  #[test]
  fn generate_identity_refuses_to_overwrite_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.age");
    generate_identity_file(&path).expect("first generate ok");
    // `Result::expect_err` requires `T: Debug`; `Identity` doesn't
    // implement Debug (secret-key derive intentionally elided),
    // so unpack via match.
    let err = match generate_identity_file(&path) {
      Err(e) => e,
      Ok(_) => panic!("second generate must refuse"),
    };
    assert!(
      err.to_string().contains("refusing to overwrite"),
      "diagnostic should explain refusal; got: {err}"
    );
  }

  #[cfg(unix)]
  #[test]
  fn generated_identity_file_has_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("identity.age");
    generate_identity_file(&path).expect("generate ok");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    // `mode` includes file type bits; mask to permission bits only.
    assert_eq!(
      mode & 0o777,
      0o600,
      "identity file must be 0600; got: {mode:o}"
    );
  }
}
