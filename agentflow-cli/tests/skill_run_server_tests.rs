//! Hermetic round-trip coverage for `agentflow skill run --server`
//! (P10.11.2).
//!
//! Spins up a tiny axum mock server that implements the two routes
//! the CLI talks to:
//! - `POST /v1/skills/{name}:run` → returns `{ run_id, status:
//!   "queued" }` after persisting the request body to a shared
//!   AtomicCell so the test can assert what was actually sent.
//! - `GET /v1/runs/{id}` → returns a terminal-status row on the
//!   second call (the first call returns `running` to exercise the
//!   poll loop at least once).
//!
//! No Postgres / no real skill registry required — this stays
//! hermetic so workspace `cargo test` runs it on every dev machine.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use assert_cmd::Command;
use axum::{
  Json, Router,
  extract::{Path, State},
  routing::{get, post},
};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const RUN_ID: &str = "00000000-0000-0000-0000-0000000000ab";

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

#[derive(Clone, Default)]
struct MockState {
  /// Captures the most recent request body sent to
  /// `POST /v1/skills/<name>:run` so tests can assert input + tenant
  /// were forwarded correctly.
  last_skill_run_body: Arc<Mutex<Option<Value>>>,
  /// Captures the skill name extracted from the URL path so tests
  /// can assert the positional arg flowed through verbatim.
  last_skill_name: Arc<Mutex<Option<String>>>,
  /// Tracks how many times `GET /v1/runs/<id>` was hit so the second
  /// call can return `succeeded` while the first returns `running` —
  /// exercises the poll loop without making the test slow.
  get_run_calls: Arc<AtomicUsize>,
}

async fn post_skill_run(
  State(state): State<MockState>,
  Path(name_run): Path<String>,
  Json(body): Json<Value>,
) -> Json<Value> {
  // Path matches the server's `/v1/skills/:name_run` convention;
  // strip the trailing `:run` to recover the bare name.
  let skill_name = name_run
    .strip_suffix(":run")
    .unwrap_or(name_run.as_str())
    .to_string();
  *state.last_skill_name.lock().await = Some(skill_name);
  *state.last_skill_run_body.lock().await = Some(body);
  Json(json!({
    "run_id": RUN_ID,
    "status": "queued",
  }))
}

async fn get_run(State(state): State<MockState>, Path(_id): Path<String>) -> Json<Value> {
  let n = state.get_run_calls.fetch_add(1, Ordering::SeqCst);
  let status = if n == 0 { "running" } else { "succeeded" };
  Json(json!({
    "id": RUN_ID,
    "status": status,
    "workflow": "@skill:hello_world",
    "tenant_id": "test-tenant",
    // The minimal extras a real terminal-status row would carry.
    // The CLI doesn't introspect these — it just pretty-prints the
    // whole row — but pinning them surfaces accidental schema
    // tightening.
    "created_at": "2026-05-20T10:00:00Z",
    "finished_at": "2026-05-20T10:00:01Z",
  }))
}

async fn spawn_mock_server() -> (String, MockState) {
  let state = MockState::default();
  let router = Router::new()
    .route("/v1/skills/:name_run", post(post_skill_run))
    .route("/v1/runs/:id", get(get_run))
    .with_state(state.clone());
  let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let addr = listener.local_addr().expect("local addr");
  tokio::spawn(async move {
    let _ = axum::serve(listener, router.into_make_service()).await;
  });
  tokio::time::sleep(Duration::from_millis(80)).await;
  (format!("http://{addr}"), state)
}

#[tokio::test]
async fn cli_skill_run_via_server_submits_and_polls_to_terminal() {
  let (server_url, state) = spawn_mock_server().await;

  let url = server_url.clone();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "skill",
        "run",
        "hello_world",
        "--message",
        "say hi",
        "--server",
        &url,
        "--tenant",
        "test-tenant",
      ])
      .assert()
      .success()
  })
  .await
  .expect("join");

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  // text mode prints the emoji-prefixed submission line + the final
  // run-row JSON pretty-printed. Both must be present.
  assert!(
    stdout.contains("Submitted skill 'hello_world' as run"),
    "missing submission line: {stdout}"
  );
  assert!(
    stdout.contains("\"status\": \"succeeded\""),
    "missing terminal status: {stdout}"
  );

  // The CLI must have forwarded the message and tenant verbatim.
  let body = state
    .last_skill_run_body
    .lock()
    .await
    .clone()
    .expect("server received a request");
  assert_eq!(body["input"], "say hi");
  assert_eq!(body["tenant_id"], "test-tenant");

  // The positional skill name must have flowed through to the URL
  // path; the test stripped `:run` already so it sees the bare name.
  let name = state.last_skill_name.lock().await.clone();
  assert_eq!(name.as_deref(), Some("hello_world"));

  // The poll loop must have run at least twice (status "running"
  // first, then "succeeded"). The mock returns "running" on call 0
  // and "succeeded" on call 1+, so 2 calls is the minimum that gets
  // us to terminal.
  assert!(
    state.get_run_calls.load(Ordering::SeqCst) >= 2,
    "expected at least 2 GET /v1/runs/<id> calls (running → succeeded)"
  );
}

#[tokio::test]
async fn cli_skill_run_via_server_envelope_mode_wraps_terminal_row() {
  let (server_url, _state) = spawn_mock_server().await;
  let url = server_url.clone();
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "skill",
        "run",
        "hello_world",
        "--message",
        "say hi",
        "--server",
        &url,
        "--output",
        "json-envelope",
      ])
      .assert()
      .success()
  })
  .await
  .expect("join");

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let parsed: Value = serde_json::from_str(&stdout).expect("envelope must be valid JSON");
  // Canonical envelope shape: { version, command, result, errors }.
  assert_eq!(parsed["version"], "agentflow.cli/1");
  assert_eq!(parsed["command"], "skill run");
  let errors = parsed["errors"].as_array().expect("errors is array");
  assert!(
    errors.is_empty(),
    "succeeded run must have no envelope errors: {errors:?}"
  );
  assert_eq!(parsed["result"]["status"], "succeeded");
}

#[tokio::test]
async fn cli_skill_run_via_server_rejects_model_flag() {
  // The validation runs before any HTTP call, so using a junk URL
  // is safe — if the validation didn't fire, the CLI would still
  // bail when it failed to connect, but the error message would be
  // about networking, not the local-only flag. Pinning the
  // specific message keeps the contract crisp.
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "skill",
        "run",
        "hello_world",
        "--message",
        "say hi",
        "--server",
        "http://127.0.0.1:1",
        "--model",
        "gpt-4o",
      ])
      .assert()
      .failure()
  })
  .await
  .expect("join");

  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--model is local-only"),
    "stderr must explain the rejection: {stderr}"
  );
}

#[tokio::test]
async fn cli_skill_run_via_server_rejects_output_json() {
  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "skill",
        "run",
        "hello_world",
        "--message",
        "say hi",
        "--server",
        "http://127.0.0.1:1",
        "--output",
        "json",
      ])
      .assert()
      .failure()
  })
  .await
  .expect("join");

  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--output json is local-only") && stderr.contains("json-envelope"),
    "stderr must point the user at the server-mode equivalent: {stderr}"
  );
}

#[tokio::test]
async fn cli_skill_run_via_server_surfaces_skill_not_installed_clearly() {
  // The mock server's only route returns 200; to exercise the 404
  // path we spawn a tiny ad-hoc server that always returns 404 on
  // skill submission. The CLI must propagate that as a non-zero
  // exit with the message body visible to the operator.
  let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let addr = listener.local_addr().expect("local addr");
  let router: Router = Router::new().route(
    "/v1/skills/:name_run",
    post(|| async {
      (
        axum::http::StatusCode::NOT_FOUND,
        Json(json!({
          "error": {
            "code": "not_found",
            "message": "skill 'ghost' not installed (configure AGENTFLOW_SKILLS_INDEX)"
          }
        })),
      )
    }),
  );
  tokio::spawn(async move {
    let _ = axum::serve(listener, router.into_make_service()).await;
  });
  tokio::time::sleep(Duration::from_millis(80)).await;
  let url = format!("http://{addr}");

  let assert = tokio::task::spawn_blocking(move || {
    cli_bin()
      .args([
        "skill",
        "run",
        "ghost",
        "--message",
        "anything",
        "--server",
        &url,
      ])
      .assert()
      .failure()
  })
  .await
  .expect("join");

  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("not installed") || stderr.contains("404"),
    "stderr should propagate the server's 'not installed' message: {stderr}"
  );
}
