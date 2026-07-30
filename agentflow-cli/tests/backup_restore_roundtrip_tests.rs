//! T2.2 end-to-end: `agentflow backup` then `agentflow restore` round
//! trips real state.
//!
//! Two tiers:
//! - Filesystem-only, always runs (no external services): proves the
//!   tar re-anchoring works when the restore target has a *different*
//!   basename than the original backup source (a real bug caught by
//!   manual testing during development — the archive is anchored to
//!   the source directory's own name at backup time, so a naive
//!   `tar -C <parent> -xzf` extraction lands the content under the
//!   wrong name at restore time unless the extraction strips that
//!   leading path component).
//! - Postgres round trip, gated by `AGENTFLOW_DATABASE_TEST_URL` (see
//!   `agentflow-db/tests/migrations.rs` for the same convention).
//!   Creates and drops a throwaway database on the shared test server
//!   so a destructive `pg_restore --clean` can never disturb rows
//!   other parallel tests seed into the shared test database.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use uuid::Uuid;

fn which(exe: &str) -> bool {
  std::env::var_os("PATH")
    .map(|path| std::env::split_paths(&path).any(|dir| dir.join(exe).is_file()))
    .unwrap_or(false)
}

#[test]
fn fs_only_backup_then_restore_lands_content_at_differently_named_target() {
  if !which("tar") {
    eprintln!("skipping fs_only_backup_then_restore... — tar not on PATH");
    return;
  }

  let work = TempDir::new().unwrap();
  let source_dir = work.path().join("source_skills");
  fs::create_dir_all(&source_dir).unwrap();
  fs::write(source_dir.join("SKILL.md"), "hello skill").unwrap();

  let bundle_dir = work.path().join("bundle");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      bundle_dir.to_str().unwrap(),
      "--include",
      "skills_dir",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &source_dir)
    .assert()
    .success();
  assert!(bundle_dir.join("manifest.json").is_file());
  assert!(bundle_dir.join("skills_dir.tar.gz").is_file());

  // Restore target intentionally has a different basename than the
  // backup source — this is the realistic case (different host,
  // renamed env override) and is exactly what the strip-components
  // fix in `restore_dir` handles.
  let target_dir = work.path().join("restored_skills");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", bundle_dir.to_str().unwrap()])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .success()
    .stdout(predicate::str::contains("skills_dir"))
    .stdout(predicate::str::contains("executed"));

  assert_eq!(
    fs::read_to_string(target_dir.join("SKILL.md")).unwrap(),
    "hello skill",
    "restored content must land directly under the target dir, not under \
     a nested directory named after the original backup source"
  );

  // Re-restoring without --force must fail (target now exists)...
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", bundle_dir.to_str().unwrap()])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .failure()
    .stdout(predicate::str::contains("already exists"));

  // ...but --force clears and re-extracts successfully.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", bundle_dir.to_str().unwrap(), "--force"])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .success();
  assert_eq!(
    fs::read_to_string(target_dir.join("SKILL.md")).unwrap(),
    "hello skill"
  );
}

#[test]
fn dry_run_restore_never_writes_the_target_directory() {
  if !which("tar") {
    eprintln!("skipping dry_run_restore_never_writes... — tar not on PATH");
    return;
  }

  let work = TempDir::new().unwrap();
  let source_dir = work.path().join("source_skills");
  fs::create_dir_all(&source_dir).unwrap();
  fs::write(source_dir.join("SKILL.md"), "hello skill").unwrap();

  let bundle_dir = work.path().join("bundle");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      bundle_dir.to_str().unwrap(),
      "--include",
      "skills_dir",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &source_dir)
    .assert()
    .success();

  let target_dir = work.path().join("restored_skills");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", bundle_dir.to_str().unwrap(), "--dry-run"])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .success()
    .stdout(predicate::str::contains("dry_run"));

  assert!(
    !target_dir.exists(),
    "--dry-run must not create the restore target directory"
  );
}

#[test]
fn backup_manifest_records_a_sha256_for_each_artifact() {
  if !which("tar") {
    eprintln!("skipping backup_manifest_records_a_sha256_for_each_artifact — tar not on PATH");
    return;
  }

  let work = TempDir::new().unwrap();
  let source_dir = work.path().join("source_skills");
  fs::create_dir_all(&source_dir).unwrap();
  fs::write(source_dir.join("SKILL.md"), "hello skill").unwrap();

  let bundle_dir = work.path().join("bundle");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      bundle_dir.to_str().unwrap(),
      "--include",
      "skills_dir",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &source_dir)
    .assert()
    .success();

  let manifest: serde_json::Value =
    serde_json::from_str(&fs::read_to_string(bundle_dir.join("manifest.json")).unwrap()).unwrap();
  let sha256 = manifest["artifacts"][0]["sha256"]
    .as_str()
    .expect("U0.2: manifest artifact must record a sha256 field");
  assert!(
    sha256.starts_with("sha256:") && sha256.len() == "sha256:".len() + 64,
    "expected sha256:<64 hex chars>, got: {sha256}"
  );
}

// U0.2 (evaluation §5 "新发现"): `agentflow restore` used to feed
// `pg_restore --clean --if-exists` / `tar -xzf` directly from whatever
// bytes were on disk under the bundle directory, with no check that
// they still matched what `agentflow backup` originally wrote. This
// simulates a bundle tampered with (or corrupted) after backup: the
// manifest's recorded hash for `skills_dir.tar.gz` no longer matches
// the file on disk.
#[test]
fn tampered_artifact_is_rejected_by_default_but_restorable_with_skip_integrity_check() {
  if !which("tar") {
    eprintln!(
      "skipping tampered_artifact_is_rejected_by_default_but_restorable_with_skip_integrity_check \
       — tar not on PATH"
    );
    return;
  }

  let work = TempDir::new().unwrap();

  // Bundle 1: the "real" backup an operator would restore from.
  let source_dir = work.path().join("source_skills");
  fs::create_dir_all(&source_dir).unwrap();
  fs::write(source_dir.join("SKILL.md"), "original content").unwrap();
  let bundle_dir = work.path().join("bundle");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      bundle_dir.to_str().unwrap(),
      "--include",
      "skills_dir",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &source_dir)
    .assert()
    .success();

  // Bundle 2: a different, but equally valid, tarball — stands in for
  // "the artifact on disk was swapped/corrupted after backup ran".
  let tampered_source_dir = work.path().join("tampered_source_skills");
  fs::create_dir_all(&tampered_source_dir).unwrap();
  fs::write(tampered_source_dir.join("SKILL.md"), "tampered content").unwrap();
  let tampered_bundle_dir = work.path().join("tampered_bundle");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      tampered_bundle_dir.to_str().unwrap(),
      "--include",
      "skills_dir",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &tampered_source_dir)
    .assert()
    .success();

  // Overwrite bundle 1's artifact with bundle 2's — bundle 1's
  // manifest.json (and its recorded hash) is untouched.
  fs::copy(
    tampered_bundle_dir.join("skills_dir.tar.gz"),
    bundle_dir.join("skills_dir.tar.gz"),
  )
  .unwrap();

  let target_dir = work.path().join("restored_skills");

  // Without --skip-integrity-check: the mismatch must fail the step
  // and the target directory must never be touched.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", bundle_dir.to_str().unwrap()])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .failure()
    .stdout(predicate::str::contains("integrity check failed"));
  assert!(
    !target_dir.exists(),
    "a failed integrity check must not create the restore target"
  );

  // With --skip-integrity-check: proceeds (with a visible warning),
  // and actually restores the artifact that's really on disk (the
  // swapped-in one), not the one the manifest originally described —
  // proving the override genuinely bypasses the check rather than
  // silently no-op'ing.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "restore",
      bundle_dir.to_str().unwrap(),
      "--skip-integrity-check",
    ])
    .env("AGENTFLOW_SKILLS_DIR", &target_dir)
    .assert()
    .success()
    .stdout(predicate::str::contains("INTEGRITY CHECK FAILED"));
  assert_eq!(
    fs::read_to_string(target_dir.join("SKILL.md")).unwrap(),
    "tampered content"
  );
}

#[test]
fn restore_help_lists_documented_flags() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("--database-url"))
    .stdout(predicate::str::contains("--dry-run"))
    .stdout(predicate::str::contains("--force"))
    .stdout(predicate::str::contains("--include"))
    .stdout(predicate::str::contains("--skip-integrity-check"));
}

#[test]
fn restore_reports_a_clear_error_for_a_non_bundle_directory() {
  let work = TempDir::new().unwrap();
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["restore", work.path().to_str().unwrap()])
    .assert()
    .failure()
    .stderr(predicate::str::contains("manifest"));
}

// ── Postgres round trip (gated) ─────────────────────────────────────────

fn live_database_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

/// Swap the database name (final path segment) in a Postgres URL.
fn with_database_name(url: &str, db_name: &str) -> String {
  let (base, _old_db) = url
    .rsplit_once('/')
    .expect("AGENTFLOW_DATABASE_TEST_URL must include a database path segment");
  format!("{base}/{db_name}")
}

#[tokio::test]
async fn backup_then_restore_round_trips_postgres_and_filesystem_state() {
  use sqlx::Connection;

  let Some(admin_url) = live_database_url() else {
    eprintln!(
      "skipping backup_then_restore_round_trips_postgres_and_filesystem_state — set AGENTFLOW_DATABASE_TEST_URL"
    );
    return;
  };
  if !which("pg_dump") || !which("pg_restore") || !which("tar") {
    eprintln!(
      "skipping backup_then_restore_round_trips_postgres_and_filesystem_state — pg_dump/pg_restore/tar not on PATH"
    );
    return;
  }

  // A dedicated, uniquely-named database on the same server so a
  // destructive `pg_restore --clean` never touches the shared test
  // database other parallel tests (in this crate or others) use.
  let db_name = format!("agentflow_restore_it_{}", Uuid::new_v4().simple());
  let mut admin_conn = sqlx::PgConnection::connect(&admin_url)
    .await
    .expect("connect to admin database");
  sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
    .execute(&mut admin_conn)
    .await
    .expect("create throwaway test database");
  let test_url = with_database_name(&admin_url, &db_name);

  let run_id = Uuid::new_v4();
  seed_run_row(&test_url, run_id).await;

  // Seed a matching run_dir artifact tree so the filesystem-restore
  // path is exercised too, not just Postgres.
  let run_dir_root = TempDir::new().unwrap();
  let run_artifact_dir = run_dir_root.path().join(run_id.to_string());
  fs::create_dir_all(&run_artifact_dir).unwrap();
  fs::write(
    run_artifact_dir.join("output.txt"),
    "hello from the original run",
  )
  .unwrap();

  let bundle_dir = TempDir::new().unwrap();

  // 1. Backup.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "backup",
      "--output",
      bundle_dir.path().to_str().unwrap(),
      "--database-url",
      &test_url,
      "--include",
      "db",
      "--include",
      "run_dir",
    ])
    .env("AGENTFLOW_RUN_DIR", run_dir_root.path())
    .assert()
    .success();
  assert!(bundle_dir.path().join("db.dump").is_file());
  assert!(bundle_dir.path().join("run_dir.tar.gz").is_file());

  // 2. Destroy the "live" state so the later assertions prove restore
  // actually brought it back, rather than trivially passing.
  fs::remove_dir_all(&run_dir_root).unwrap();
  delete_run_row(&test_url, run_id).await;
  assert!(
    get_run_row(&test_url, run_id).await.is_none(),
    "precondition: row must be gone before restore"
  );

  // 3. Restore.
  let restore_run_dir = TempDir::new().unwrap();
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "restore",
      bundle_dir.path().to_str().unwrap(),
      "--database-url",
      &test_url,
      "--force",
    ])
    .env("AGENTFLOW_RUN_DIR", restore_run_dir.path())
    .assert()
    .success();

  // 4. Assert Postgres round-tripped.
  let restored_tenant = get_run_row(&test_url, run_id)
    .await
    .expect("run row must be restored");
  assert_eq!(restored_tenant, "restore-it");

  // 5. Assert the filesystem round-tripped.
  let restored_content = fs::read_to_string(
    restore_run_dir
      .path()
      .join(run_id.to_string())
      .join("output.txt"),
  )
  .expect("restored run_dir must contain the original output file");
  assert_eq!(restored_content, "hello from the original run");

  // Cleanup: drop the throwaway database. Best-effort — a leaked test
  // DB from a hard-killed test run is a manual sweep, not a leak that
  // corrupts subsequent runs (the UUID suffix keeps names unique).
  let _ = sqlx::query(&format!(
    "DROP DATABASE IF EXISTS \"{db_name}\" WITH (FORCE)"
  ))
  .execute(&mut admin_conn)
  .await;
}

async fn seed_run_row(url: &str, run_id: Uuid) {
  let db = agentflow_db::Database::connect_and_migrate(url, 2)
    .await
    .expect("migrate fresh test database");
  let repos = agentflow_db::Repositories::from_pool(db.pool.clone());
  use agentflow_db::RunRepo;
  repos
    .runs
    .create(agentflow_db::NewRun {
      id: run_id,
      workflow: "restore-it-workflow".to_string(),
      status: agentflow_db::RunStatus::Succeeded,
      run_dir: None,
      tenant_id: "restore-it".to_string(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .expect("seed run row");
}

async fn delete_run_row(url: &str, run_id: Uuid) {
  let db = agentflow_db::Database::connect(url, 2)
    .await
    .expect("connect to test database");
  sqlx::query("DELETE FROM runs WHERE id = $1")
    .bind(run_id)
    .execute(&db.pool)
    .await
    .expect("delete seeded run row");
}

async fn get_run_row(url: &str, run_id: Uuid) -> Option<String> {
  let db = agentflow_db::Database::connect(url, 2)
    .await
    .expect("connect to test database");
  let repos = agentflow_db::Repositories::from_pool(db.pool.clone());
  use agentflow_db::RunRepo;
  repos
    .runs
    .get(run_id)
    .await
    .expect("query run row")
    .map(|run| run.tenant_id)
}
