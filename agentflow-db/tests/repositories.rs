//! Integration tests for the Postgres repository implementations.
//!
//! Gated by `AGENTFLOW_DATABASE_TEST_URL` for the same reason as the
//! migrations test — keeps `cargo test --workspace` hermetic. To run:
//!
//! ```bash
//! AGENTFLOW_DATABASE_TEST_URL=postgres://postgres:postgres@localhost:5432/agentflow_test \
//!   cargo test -p agentflow-db --test repositories
//! ```

use agentflow_db::{
  Database, EventRepo, NewEvent, NewRun, NewStep, Repositories, RunRepo, RunStatus, StepRepo,
};
use serde_json::json;
use uuid::Uuid;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_db() -> Option<Database> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4)
    .await
    .expect("connect + migrate");
  // Each test gets a clean slate. Cascade delete propagates to dependent rows.
  sqlx::query("TRUNCATE runs RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .expect("truncate");
  sqlx::query("TRUNCATE skill_installs, mcp_sessions RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .expect("truncate");
  Some(db)
}

#[tokio::test]
async fn run_repo_create_get_list_update() {
  let Some(db) = fresh_db().await else {
    eprintln!("skipping run_repo_create_get_list_update — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let repos = Repositories::from_pool(db.pool.clone());

  let id = Uuid::new_v4();
  let new_run = NewRun {
    id,
    workflow: "demo".into(),
    status: RunStatus::Queued,
    run_dir: Some("/tmp/x".into()),
    tenant_id: "default".into(),
  };
  let created = repos.runs.create(new_run).await.expect("create run");
  assert_eq!(created.id, id);
  assert_eq!(created.status, "queued");

  let fetched = repos.runs.get(id).await.expect("get run").expect("present");
  assert_eq!(fetched.workflow, "demo");

  repos
    .runs
    .update_status(id, RunStatus::Failed, Some("oops"))
    .await
    .expect("update");

  let after = repos.runs.get(id).await.expect("get run").expect("present");
  assert_eq!(after.status, "failed");
  assert_eq!(after.error.as_deref(), Some("oops"));
  assert!(after.finished_at.is_some());

  let listed = repos.runs.list("default", 10).await.expect("list");
  assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn step_and_event_repos_round_trip() {
  let Some(db) = fresh_db().await else {
    eprintln!("skipping step_and_event_repos_round_trip — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let repos = Repositories::from_pool(db.pool.clone());

  let run_id = Uuid::new_v4();
  repos
    .runs
    .create(NewRun {
      id: run_id,
      workflow: "demo".into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: "default".into(),
    })
    .await
    .expect("create run");

  let step = repos
    .steps
    .append(NewStep {
      run_id,
      seq: 0,
      node_id: "n0".into(),
      kind: "node".into(),
      status: "started".into(),
      duration_ms: None,
      payload: Some(json!({"hello": "world"})),
    })
    .await
    .expect("append step");
  assert_eq!(step.seq, 0);

  for seq in 0..3 {
    repos
      .events
      .append(NewEvent {
        run_id,
        seq,
        kind: "node_started".into(),
        payload: json!({"seq": seq}),
      })
      .await
      .expect("append event");
  }

  let events_after_zero = repos
    .events
    .list_after(run_id, 0, 100)
    .await
    .expect("list events");
  // seq > 0 means we get seq 1 and 2 only.
  assert_eq!(events_after_zero.len(), 2);
  assert_eq!(events_after_zero[0].seq, 1);
}
