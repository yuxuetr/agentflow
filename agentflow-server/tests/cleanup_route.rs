//! Integration tests for `cleanup_expired`. Requires a live Postgres
//! pointed to by `AGENTFLOW_DATABASE_TEST_URL`; without it the tests
//! exit early so workspace `cargo test` stays hermetic.

use std::fs;

use agentflow_db::{Database, NewRun, Repositories, RunRepo, RunStatus};
use agentflow_server::{CleanupConfig, cleanup_expired};
use agentflow_tools::SecurityProfile;
use chrono::{Duration, Utc};
use tempfile::TempDir;
use uuid::Uuid;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

fn repos(db: &Database) -> Repositories {
  Repositories::from_pool(db.pool.clone())
}

async fn fresh_db() -> Option<Database> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  sqlx::query("TRUNCATE runs RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .ok()?;
  Some(db)
}

async fn insert_run(db: &Database, status: RunStatus, finished_offset_days: Option<i64>) -> Uuid {
  let id = Uuid::new_v4();
  repos(&db)
    .runs
    .create(NewRun {
      id,
      tenant_id: "default".into(),
      workflow: "name: stub".into(),
      status: RunStatus::Running,
      run_dir: None,
    })
    .await
    .unwrap();
  if !matches!(status, RunStatus::Running | RunStatus::Queued) {
    // Mark terminal and back-date finished_at.
    repos(&db)
      .runs
      .update_status(id, status, None)
      .await
      .unwrap();
    if let Some(days) = finished_offset_days {
      let target = Utc::now() - Duration::days(days);
      sqlx::query("UPDATE runs SET finished_at = $1 WHERE id = $2")
        .bind(target)
        .bind(id)
        .execute(&db.pool)
        .await
        .unwrap();
    }
  }
  id
}

#[tokio::test]
async fn cleanup_dry_run_targets_old_terminal_runs_without_deleting() {
  let Some(db) = fresh_db().await else {
    eprintln!("skipping cleanup_dry_run_targets_old_terminal_runs_without_deleting");
    return;
  };

  let old_id = insert_run(&db, RunStatus::Succeeded, Some(60)).await;
  let young_id = insert_run(&db, RunStatus::Succeeded, Some(1)).await;
  let active_id = insert_run(&db, RunStatus::Running, None).await;

  let cfg = CleanupConfig::for_profile(SecurityProfile::Local).with_dry_run(true);
  let report = cleanup_expired(&db, None, &cfg).await.unwrap();

  assert!(report.dry_run);
  assert!(report.run_ids_targeted.contains(&old_id));
  assert!(!report.run_ids_targeted.contains(&young_id));
  assert!(!report.run_ids_targeted.contains(&active_id));

  // No actual rows deleted — dry-run is read-only.
  let still_there = repos(&db).runs.get(old_id).await.unwrap();
  assert!(
    still_there.is_some(),
    "dry-run must not delete rows: {old_id}"
  );
}

#[tokio::test]
async fn cleanup_deletes_terminal_runs_past_retention() {
  let Some(db) = fresh_db().await else {
    eprintln!("skipping cleanup_deletes_terminal_runs_past_retention");
    return;
  };

  let old_id = insert_run(&db, RunStatus::Succeeded, Some(60)).await;
  let young_id = insert_run(&db, RunStatus::Succeeded, Some(1)).await;
  let active_id = insert_run(&db, RunStatus::Running, None).await;

  let cfg = CleanupConfig::for_profile(SecurityProfile::Local);
  let report = cleanup_expired(&db, None, &cfg).await.unwrap();
  assert!(!report.dry_run);
  assert!(report.runs_deleted >= 1);

  assert!(repos(&db).runs.get(old_id).await.unwrap().is_none());
  assert!(repos(&db).runs.get(young_id).await.unwrap().is_some());
  assert!(repos(&db).runs.get(active_id).await.unwrap().is_some());
}

#[tokio::test]
async fn cleanup_filesystem_sweep_removes_orphaned_dirs() {
  let Some(db) = fresh_db().await else {
    eprintln!("skipping cleanup_filesystem_sweep_removes_orphaned_dirs");
    return;
  };

  let tmp = TempDir::new().unwrap();
  let orphan_id = Uuid::new_v4();
  let orphan_dir = tmp.path().join(orphan_id.to_string());
  fs::create_dir_all(&orphan_dir).unwrap();
  fs::write(orphan_dir.join("artifact.txt"), "hi").unwrap();
  // Back-date the directory mtime via the `touch -t` shell utility so
  // the retention age gate fires. On platforms without `touch` the
  // test self-skips; that keeps CI portable.
  let old_path = orphan_dir.display().to_string();
  let touched = std::process::Command::new("touch")
    .args(["-t", "200001010000", &old_path])
    .status();
  if !touched.map(|s| s.success()).unwrap_or(false) {
    eprintln!("skipping (couldn't backdate dir mtime via `touch`)");
    return;
  }

  let active_id = insert_run(&db, RunStatus::Running, None).await;
  let active_dir = tmp.path().join(active_id.to_string());
  fs::create_dir_all(&active_dir).unwrap();

  let cfg = CleanupConfig::for_profile(SecurityProfile::Local);
  let report = cleanup_expired(&db, Some(tmp.path()), &cfg).await.unwrap();
  assert!(report.run_dirs_skipped_active >= 1);
  assert!(!orphan_dir.exists(), "orphan dir must be deleted");
  assert!(active_dir.exists(), "active run's dir must be retained");
}
