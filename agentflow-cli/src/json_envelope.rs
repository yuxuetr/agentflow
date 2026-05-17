//! Canonical JSON output envelope for the `agentflow` CLI (P3.3).
//!
//! See `docs/CLI_JSON_OUTPUT.md` for the full contract. The envelope is the
//! preferred shape every new `--output json` / `--format json` mode should
//! emit. The four envelope fields are intentionally closed:
//!
//! ```json
//! {
//!   "version": "agentflow.cli/1",
//!   "command": "doctor",
//!   "result": { /* per-command payload */ },
//!   "errors": [ /* user-actionable error strings */ ]
//! }
//! ```
//!
//! `version` is the wire schema discriminator. It changes only when a
//! breaking change to the envelope itself ships — per-command `result`
//! changes are tracked by the respective command's own stability tier.
//!
//! `command` is the verbose subcommand path the operator typed (e.g.
//! `doctor`, `workflow validate`, `marketplace install`). It exists so that
//! a multiplexed log capturing JSON from several commands can be parsed
//! without needing to inspect the structural payload.
//!
//! `errors` is a list of strings, never `null`. Successful runs return an
//! empty list. Each entry is a human-readable, single-line message. The
//! envelope is not the place for structured error codes — those live
//! inside `result` when the per-command schema needs them.
//!
//! Migration plan (per P3.3 follow-ups in TODOs.md):
//!   1. New JSON output modes start on the envelope from day one.
//!   2. Existing JSON outputs that emit raw payloads get a follow-up flag
//!      (`--json-envelope` or version bump) before being defaulted.

use serde::{Deserialize, Serialize};

/// Stable wire-schema discriminator. Bump only on breaking envelope changes.
pub const ENVELOPE_VERSION: &str = "agentflow.cli/1";

/// Top-level CLI JSON envelope.
///
/// `T` is the per-command result payload type. Implementors should keep
/// `T` `Serialize + Deserialize` so round-trip tests can prove field
/// preservation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliJsonEnvelope<T> {
  /// Wire schema discriminator. See [`ENVELOPE_VERSION`].
  pub version: String,
  /// Subcommand path that produced this output (`"doctor"`,
  /// `"workflow validate"`, `"marketplace install"`, etc.). Multi-word
  /// commands are space-separated to match the user-visible command line.
  pub command: String,
  /// Per-command payload. May be `Value::Null` for commands whose JSON
  /// output is metadata-only.
  pub result: T,
  /// Zero or more user-actionable error strings. Empty for successful
  /// runs; never `null`.
  #[serde(default)]
  pub errors: Vec<String>,
}

impl<T> CliJsonEnvelope<T> {
  /// Wrap a successful per-command payload in the envelope.
  pub fn ok(command: impl Into<String>, result: T) -> Self {
    Self {
      version: ENVELOPE_VERSION.to_string(),
      command: command.into(),
      result,
      errors: Vec::new(),
    }
  }

  /// Wrap a per-command payload alongside user-actionable error strings.
  /// The envelope can carry both a partial `result` and `errors` — many
  /// commands surface partial output (e.g. doctor with one failing
  /// section) and the operator wants both at once.
  pub fn with_errors(command: impl Into<String>, result: T, errors: Vec<String>) -> Self {
    Self {
      version: ENVELOPE_VERSION.to_string(),
      command: command.into(),
      result,
      errors,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};

  #[test]
  fn ok_envelope_round_trips_with_empty_errors() {
    let envelope = CliJsonEnvelope::ok("doctor", json!({"status": "ok"}));
    let json = serde_json::to_string(&envelope).unwrap();
    let parsed: CliJsonEnvelope<Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.version, ENVELOPE_VERSION);
    assert_eq!(parsed.command, "doctor");
    assert_eq!(parsed.result, json!({"status": "ok"}));
    assert!(
      parsed.errors.is_empty(),
      "ok envelope must carry empty errors"
    );
  }

  #[test]
  fn with_errors_preserves_both_payload_and_errors() {
    let envelope = CliJsonEnvelope::with_errors(
      "workflow validate",
      json!({"valid": false, "issues": ["bad"]}),
      vec!["missing parameter 'foo'".to_string()],
    );
    let json = serde_json::to_string(&envelope).unwrap();
    let parsed: CliJsonEnvelope<Value> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.command, "workflow validate");
    assert_eq!(parsed.errors, vec!["missing parameter 'foo'".to_string()]);
    assert_eq!(parsed.result["valid"], false);
  }

  #[test]
  fn envelope_field_set_is_closed_to_four_keys() {
    // The envelope is a closed contract — if a future change adds a fifth
    // top-level key it must bump ENVELOPE_VERSION. This test catches the
    // accidental drift.
    let envelope = CliJsonEnvelope::ok("doctor", json!(null));
    let serialized: Value = serde_json::to_value(&envelope).unwrap();
    let mut keys: Vec<&str> = serialized
      .as_object()
      .expect("envelope must serialize as a JSON object")
      .keys()
      .map(String::as_str)
      .collect();
    keys.sort();
    assert_eq!(
      keys,
      vec!["command", "errors", "result", "version"],
      "the envelope contract is closed; new fields require a version bump"
    );
  }

  #[test]
  fn errors_default_to_empty_when_field_absent_on_read() {
    // Backward-compat path: producers that emit only {version, command,
    // result} (no `errors` field) parse cleanly with `errors = []`. The
    // `#[serde(default)]` annotation locks this in.
    let raw = json!({
      "version": ENVELOPE_VERSION,
      "command": "doctor",
      "result": {"status": "ok"},
    });
    let parsed: CliJsonEnvelope<Value> = serde_json::from_value(raw).unwrap();
    assert!(parsed.errors.is_empty());
  }

  #[test]
  fn version_constant_is_stable_wire_value() {
    // Lock the literal wire value — if this string changes, every JSON
    // consumer parsing the envelope needs to be notified. Bumping the
    // version is intentional but must never be incidental.
    assert_eq!(ENVELOPE_VERSION, "agentflow.cli/1");
  }
}
