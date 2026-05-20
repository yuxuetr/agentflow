//! Hermetic CLI coverage for `agentflow memory prune` (P10.7.1).
//!
//! The CLI binary is exercised against a real on-disk SQLite file
//! that's seeded via the public memory-crate API. This proves the
//! end-to-end wiring (clap → command dispatch → SqlitePreferenceStore
//! / SqliteEntityFactStore → row-count return → output rendering).
//! No external services required.

use std::time::Duration;

use agentflow_memory::{
  EntityFact, EntityFactStore, PreferenceScope, PreferenceStore, SqliteEntityFactStore,
  SqlitePreferenceStore,
};
use assert_cmd::Command;
use serde_json::Value;

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

#[tokio::test]
async fn cli_memory_prune_preference_removes_old_rows_and_emits_envelope() {
  let dir = tempfile::tempdir().expect("tempdir");
  let db = dir.path().join("memory.db");

  // Seed: one row that will age past the cutoff, one fresh row.
  {
    let mut store = SqlitePreferenceStore::open(&db)
      .await
      .expect("open seed store");
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "theme", serde_json::json!("dark"))
      .await
      .unwrap();
    // Sleep just past the 1s cutoff so `theme.updated_at` lands
    // strictly before `now() - 1s` while `lang.updated_at` lands
    // strictly after. Same trick as the in-crate unit test.
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    store
      .put_preference(&scope, "lang", serde_json::json!("en"))
      .await
      .unwrap();
    // Store drops, SQLite pool flushes — the CLI subprocess will
    // see the persisted state.
  }

  let db_str = db.to_str().unwrap().to_string();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "memory",
        "prune",
        "--layer",
        "preference",
        "--db",
        &db_str,
        "--older-than",
        "1s",
        "--format",
        "json-envelope",
      ])
      .assert()
      .success()
  })
  .await
  .expect("join");

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let parsed: Value = serde_json::from_str(&stdout).expect("envelope must be valid JSON");
  assert_eq!(parsed["version"], "agentflow.cli/1");
  assert_eq!(parsed["command"], "memory prune");
  assert!(
    parsed["errors"]
      .as_array()
      .expect("errors is array")
      .is_empty()
  );
  assert_eq!(parsed["result"]["layer"], "preference");
  assert_eq!(
    parsed["result"]["removed_rows"], 1,
    "exactly 1 row (theme) should be pruned: {stdout}"
  );
  assert_eq!(parsed["result"]["older_than"], "1s");

  // Verify the surviving row by re-opening the DB.
  let store = SqlitePreferenceStore::open(&db).await.unwrap();
  let scope = PreferenceScope::local("alice");
  let remaining = store.list_preferences(&scope).await.unwrap();
  let keys: Vec<&str> = remaining.iter().map(|(k, _)| k.as_str()).collect();
  assert_eq!(
    keys,
    vec!["lang"],
    "only the fresh row should survive on disk"
  );
}

#[tokio::test]
async fn cli_memory_prune_entity_facts_skips_active_rows() {
  let dir = tempfile::tempdir().expect("tempdir");
  let db = dir.path().join("memory.db");

  // Seed: one active fact + one invalidated fact. The CLI must
  // prune ONLY the invalidated one, even when the cutoff is 0
  // (=== "any invalidated row, regardless of age"). This pins the
  // safety invariant from the EntityFactStore trait docs:
  // `prune_invalidated` is restricted to invalidated rows.
  {
    let mut store = SqliteEntityFactStore::open(&db)
      .await
      .expect("open seed store");
    let active = EntityFact::new("e1", "f-active", "color", serde_json::json!("blue"), 0.9);
    let stale = EntityFact::new("e1", "f-stale", "city", serde_json::json!("nyc"), 0.9);
    store.record_fact(active).await.unwrap();
    store.record_fact(stale).await.unwrap();
    store
      .invalidate_fact("e1", "f-stale", "user moved")
      .await
      .unwrap();
  }

  let db_str = db.to_str().unwrap().to_string();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "memory",
        "prune",
        "--layer",
        "entity_facts",
        "--db",
        &db_str,
        "--older-than",
        "0s",
        "--format",
        "json-envelope",
      ])
      .assert()
      .success()
  })
  .await
  .expect("join");

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let parsed: Value = serde_json::from_str(&stdout).unwrap();
  assert_eq!(parsed["result"]["layer"], "entity_facts");
  assert_eq!(
    parsed["result"]["removed_rows"], 1,
    "exactly 1 row (the invalidated one) should be pruned: {stdout}"
  );

  // The active fact must still be present after the prune.
  let store = SqliteEntityFactStore::open(&db).await.unwrap();
  let facts = store.get_facts("e1", false).await.unwrap();
  let attrs: Vec<&str> = facts.iter().map(|f| f.attribute.as_str()).collect();
  assert!(
    attrs.contains(&"color"),
    "active fact must survive: got attrs {attrs:?}"
  );
}

#[test]
fn cli_memory_prune_rejects_unsupported_layer() {
  let dir = tempfile::tempdir().expect("tempdir");
  let db = dir.path().join("memory.db");
  // Touch the file so the path-exists check passes and the layer
  // validation is what surfaces the error.
  std::fs::write(&db, b"").expect("touch db");

  let db_str = db.to_str().unwrap().to_string();
  let assert = cli_bin()
    .args([
      "memory",
      "prune",
      "--layer",
      "session",
      "--db",
      &db_str,
      "--older-than",
      "30d",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  // clap itself rejects an unknown value because of the
  // value_parser allowlist — the error must name the supported set.
  assert!(
    stderr.contains("session") || stderr.contains("preference"),
    "stderr should name supported layers: {stderr}"
  );
}

#[test]
fn cli_memory_prune_rejects_missing_db_with_actionable_message() {
  let dir = tempfile::tempdir().expect("tempdir");
  let missing = dir.path().join("does-not-exist.db");
  let assert = cli_bin()
    .args([
      "memory",
      "prune",
      "--layer",
      "preference",
      "--db",
      missing.to_str().unwrap(),
      "--older-than",
      "30d",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("does not exist"),
    "stderr should explain the missing-db error: {stderr}"
  );
  // The remediation pointer matters more than the wording — the
  // operator should immediately know what to fix.
  assert!(stderr.contains("--db"), "{stderr}");
}

#[test]
fn cli_memory_prune_rejects_bare_integer_duration() {
  let dir = tempfile::tempdir().expect("tempdir");
  let db = dir.path().join("memory.db");
  std::fs::write(&db, b"").expect("touch db");
  let assert = cli_bin()
    .args([
      "memory",
      "prune",
      "--layer",
      "preference",
      "--db",
      db.to_str().unwrap(),
      "--older-than",
      "30",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("must end in a unit"),
    "stderr should explain the missing-unit error: {stderr}"
  );
}
