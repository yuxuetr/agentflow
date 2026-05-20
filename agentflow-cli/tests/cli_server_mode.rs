//! End-to-end coverage for `agentflow workflow --server` (P2.5).
//!
//! Spins up an in-process `agentflow-server` HTTP gateway against the
//! test Postgres, points the CLI binary at it via `--server <url>`, and
//! exercises submit/list/cancel/graph roundtrips.
//!
//! Self-skips when `AGENTFLOW_DATABASE_TEST_URL` is unset so workspace
//! `cargo test` stays hermetic.

use std::time::Duration;

use agentflow_db::{Database, NewRun, RunRepo, RunStatus};
use agentflow_server::{AppState, create_router};
use assert_cmd::Command;
use serde_json::Value;
use tokio::net::TcpListener;
use uuid::Uuid;

const FIXED_DAG: &str = r#"
name: P2.5 Server-Mode Demo
nodes:
  - id: render
    type: template
    parameters:
      template: "hello server"
"#;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn spawn_server() -> Option<(String, AppState)> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  let state = AppState::new(db);
  let router = create_router(state.clone());

  let listener = TcpListener::bind("127.0.0.1:0").await.ok()?;
  let addr = listener.local_addr().ok()?;
  tokio::spawn(async move {
    let _ = axum::serve(listener, router.into_make_service()).await;
  });
  // Give the server a chance to start accepting connections.
  tokio::time::sleep(Duration::from_millis(80)).await;
  Some((format!("http://{addr}"), state))
}

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

#[tokio::test]
async fn cli_workflow_run_via_server_executes_and_returns_terminal_state() {
  let Some((server_url, _state)) = spawn_server().await else {
    eprintln!("skipping cli_workflow_run_via_server — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  // Write the workflow to a temp file so the CLI can read it.
  let dir = tempfile::tempdir().expect("tempdir");
  let workflow_file = dir.path().join("workflow.yml");
  std::fs::write(&workflow_file, FIXED_DAG).expect("write workflow");

  // CLI subprocess — `--server` triggers the new HTTP path.
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "workflow",
        "run",
        workflow_file.to_str().unwrap(),
        "--server",
        &server_url,
      ])
      .assert()
      .success()
  })
  .await
  .expect("join");

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  assert!(
    stdout.contains("Submitted run"),
    "stdout should announce the submission: {stdout}"
  );
  // The CLI polls until terminal and prints the final run row JSON.
  assert!(
    stdout.contains("\"succeeded\"") || stdout.contains("\"status\": \"succeeded\""),
    "expected the final JSON to carry a succeeded status, got: {stdout}"
  );
}

#[tokio::test]
async fn cli_workflow_list_via_server_returns_tenant_scoped_rows() {
  let Some((server_url, state)) = spawn_server().await else {
    eprintln!("skipping cli_workflow_list_via_server — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  // Seed a unique tenant so parallel tests don't collide.
  let tenant = format!("p25-list-{}", Uuid::new_v4());
  for i in 0..3 {
    state
      .repos
      .runs
      .create(NewRun {
        id: Uuid::new_v4(),
        workflow: format!("seed-{i}"),
        status: RunStatus::Queued,
        run_dir: None,
        tenant_id: tenant.clone(),
      })
      .await
      .unwrap();
  }

  let server_url_for_cli = server_url.clone();
  let tenant_for_cli = tenant.clone();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "workflow",
        "list",
        "--server",
        &server_url_for_cli,
        "--tenant",
        &tenant_for_cli,
        "--limit",
        "10",
      ])
      .assert()
      .success()
  })
  .await
  .unwrap();

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let parsed: Value = serde_json::from_str(&stdout).expect("CLI emits JSON");
  let rows = parsed["runs"].as_array().expect("runs array present");
  assert_eq!(rows.len(), 3, "exactly the 3 seeded rows for this tenant");
  for row in rows {
    assert_eq!(row["tenant_id"], tenant);
  }
}

#[tokio::test]
async fn cli_workflow_cancel_via_server_marks_run_cancelled() {
  let Some((server_url, state)) = spawn_server().await else {
    eprintln!("skipping cli_workflow_cancel_via_server — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let tenant = format!("p25-cancel-{}", Uuid::new_v4());
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "cancel-me".into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: tenant.clone(),
    })
    .await
    .unwrap();

  let url = server_url.clone();
  let tenant_for_cli = tenant.clone();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "workflow",
        "cancel",
        &id.to_string(),
        "--server",
        &url,
        "--tenant",
        &tenant_for_cli,
      ])
      .assert()
      .success()
  })
  .await
  .unwrap();

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let parsed: Value = serde_json::from_str(&stdout).expect("CLI emits JSON");
  assert_eq!(parsed["cancelled"], true);

  // The row must have transitioned in the DB.
  let row = state.repos.runs.get(id).await.unwrap().unwrap();
  assert_eq!(row.status, "cancelled");
}

// (P10.13.1: `cli_workflow_graph_via_server_returns_visualisation`
// removed alongside the `agentflow-viz` crate + the
// `workflow graph` subcommand.)

#[tokio::test]
async fn cli_workflow_list_without_server_errors_clearly() {
  // Negative path — no server URL configured.
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .env_remove("AGENTFLOW_SERVER_URL")
      .args(["workflow", "list"])
      .assert()
      .failure()
  })
  .await
  .unwrap();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--server") || stderr.contains("AGENTFLOW_SERVER_URL"),
    "error must point the operator at the flag / env var, got: {stderr}"
  );
}

#[tokio::test]
async fn cli_workflow_run_via_server_404s_against_unknown_tenant() {
  let Some((server_url, state)) = spawn_server().await else {
    eprintln!("skipping cli_workflow_run_via_server_404s — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  // Seed a row under tenant-owner; then ask for it as tenant-intruder.
  let id = Uuid::new_v4();
  let owner_tenant = format!("p25-tenant-owner-{}", Uuid::new_v4());
  let intruder_tenant = format!("p25-tenant-intruder-{}", Uuid::new_v4());
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "owner-run".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: owner_tenant.clone(),
    })
    .await
    .unwrap();

  // graph is the simplest read path; cancel would also work.
  let url = server_url.clone();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "workflow",
        "graph",
        &id.to_string(),
        "--server",
        &url,
        "--tenant",
        &intruder_tenant,
      ])
      .assert()
      .failure()
  })
  .await
  .unwrap();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let combined = format!("{stderr}\n{stdout}");
  assert!(
    combined.contains("not found") || combined.contains("404") || combined.contains("Not Found"),
    "cross-tenant access must surface as 404, got: {combined}"
  );
}
