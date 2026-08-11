//! W4.3b-d — end-to-end distributed run.
//!
//! The first test anywhere to combine a real `serve_worker_grpc`-bound
//! listener on loopback TCP (not `InMemoryWorkerProtocol`-only, unlike
//! every `DistributedDagScheduler` test that came before it) with a real
//! `agentflow_worker::WorkerRuntime` connected over `GrpcWorkerProtocol`,
//! submitted through `POST /v1/runs` with `execution_mode: "distributed"`.
//! Proves the full wire path: HTTP submit -> `DistributedFlowRunExecutor`
//! -> gRPC control plane -> real worker process claims + executes a real
//! `template` node payload -> reports back -> run reaches `succeeded` and
//! the DB/SSE event log looks like any other run's.
//!
//! Requires Postgres pointed to by `AGENTFLOW_DATABASE_TEST_URL`. Without
//! it the tests self-skip, matching every other server e2e test file.

use std::net::SocketAddr;
use std::time::Duration;

use agentflow_db::{Database, EventRepo};
use agentflow_server::scheduler::{GrpcWorkerProtocol, WorkerId};
use agentflow_server::worker_grpc::{
  WorkerGrpcServeConfig, build_worker_control_plane, serve_worker_grpc,
};
use agentflow_server::{AppState, create_router};
use agentflow_worker::{WorkerConfig, WorkerRuntime};
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tower::ServiceExt;
use uuid::Uuid;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

const TEMPLATE_WORKFLOW: &str = r#"
name: W4.3b-d end-to-end distributed run
nodes:
  - id: n1
    type: template
    parameters:
      template: "hello distributed"
"#;

/// Bind a real, fully-open (dev/local default: no PSK, no allowlist)
/// worker gRPC control plane on loopback, matching
/// `worker_grpc.rs::serves_plaintext_and_completes_claim_heartbeat_report_cycle`'s
/// bind-probe-then-serve idiom. Returns the bound address, the control
/// plane handle (attach to `AppState` for the HTTP side), and a shutdown
/// sender for the listener task.
async fn bind_worker_grpc() -> (
  SocketAddr,
  std::sync::Arc<
    agentflow_server::scheduler::AuthenticatedControlPlane<
      agentflow_server::scheduler::InMemoryWorkerProtocol,
    >,
  >,
  oneshot::Sender<()>,
) {
  let probe: SocketAddr = "127.0.0.1:0".parse().unwrap();
  let listener = tokio::net::TcpListener::bind(probe).await.unwrap();
  let addr = listener.local_addr().unwrap();
  drop(listener);

  let config = WorkerGrpcServeConfig {
    bind: addr,
    tls: None,
    allowed_worker_ids: Vec::new(),
    shared_psk: None,
  };
  let plane = build_worker_control_plane(&config, agentflow_tools::SecurityProfile::Local).unwrap();

  let (shutdown_tx, shutdown_rx) = oneshot::channel();
  let serve_plane = plane.clone();
  tokio::spawn(async move {
    let _ = serve_worker_grpc(addr, serve_plane, None, async {
      let _ = shutdown_rx.await;
    })
    .await;
  });

  (addr, plane, shutdown_tx)
}

/// Connect a real `GrpcWorkerProtocol` client, retrying until the
/// listener spawned by `bind_worker_grpc` is actually ready to accept
/// connections (mirrors the existing `worker_grpc.rs` test's retry
/// loop — the listener task above hasn't necessarily bound yet by the
/// time this runs).
async fn connect_worker_protocol(addr: SocketAddr) -> GrpcWorkerProtocol {
  let endpoint = format!("http://{addr}");
  for _ in 0..40 {
    if let Ok(client) = GrpcWorkerProtocol::connect(&endpoint).await {
      return client;
    }
    tokio::time::sleep(Duration::from_millis(25)).await;
  }
  panic!("worker gRPC listener never became ready");
}

/// Drive a real `WorkerRuntime` (genuine node-payload execution, not a
/// hand-rolled fake) until `max_iterations` empty-or-successful claim
/// attempts have passed — bounded so a broken scheduler/listener can't
/// hang the test forever.
async fn run_real_worker_until_drained(
  runtime: WorkerRuntime<GrpcWorkerProtocol>,
  max_iterations: usize,
) {
  for _ in 0..max_iterations {
    match runtime.run_once().await {
      Ok(Some(_)) => {}
      Ok(None) => tokio::time::sleep(Duration::from_millis(25)).await,
      Err(_) => break,
    }
  }
}

async fn submit(app_state: AppState, body: Value) -> (StatusCode, Value) {
  let response = create_router(app_state)
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  let status = response.status();
  let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
  let parsed: Value = serde_json::from_slice(&bytes).unwrap();
  (status, parsed)
}

async fn get_run(app_state: AppState, run_id: &str) -> Value {
  let response = create_router(app_state)
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{run_id}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
  serde_json::from_slice(&bytes).unwrap()
}

async fn poll_until_terminal(state: &AppState, run_id: &str, timeout: Duration) -> Value {
  let deadline = tokio::time::Instant::now() + timeout;
  loop {
    let body = get_run(state.clone(), run_id).await;
    let status = body["status"].as_str().unwrap_or("");
    if matches!(status, "succeeded" | "failed" | "cancelled") {
      return body;
    }
    if tokio::time::Instant::now() >= deadline {
      panic!("run {run_id} never reached a terminal status, last body: {body}");
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
}

#[tokio::test]
async fn distributed_submission_reaches_succeeded_via_a_real_worker() {
  let Some(url) = live_url() else {
    eprintln!("skipping distributed_submission_reaches_succeeded_via_a_real_worker");
    return;
  };
  let db = Database::connect_and_migrate(&url, 4)
    .await
    .expect("connect");
  let repos = agentflow_db::Repositories::from_pool(db.pool.clone());
  let state = AppState::new(db);

  let (addr, plane, _shutdown_tx) = bind_worker_grpc().await;
  let state = state.with_worker_control_plane(Some(plane));

  let (status, body) = submit(
    state.clone(),
    json!({"workflow": TEMPLATE_WORKFLOW, "execution_mode": "distributed"}),
  )
  .await;
  assert_eq!(status, StatusCode::OK, "submit failed: {body}");
  let run_id = body["run_id"].as_str().expect("run_id").to_string();

  let client = connect_worker_protocol(addr).await;
  let worker_id = WorkerId::new("w4-3b-d-worker").unwrap();
  let runtime = WorkerRuntime::new(client, WorkerConfig::new(worker_id, addr.to_string()));
  let worker_handle = tokio::spawn(run_real_worker_until_drained(runtime, 200));

  let terminal = tokio::time::timeout(
    Duration::from_secs(15),
    poll_until_terminal(&state, &run_id, Duration::from_secs(15)),
  )
  .await
  .expect("run must reach a terminal status within 15s");
  assert_eq!(terminal["status"], "succeeded", "got: {terminal}");
  worker_handle.abort();

  let run_uuid = Uuid::parse_str(&run_id).unwrap();
  let events = repos
    .events
    .list_after("default", run_uuid, -1, 100)
    .await
    .expect("list events");
  let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
  assert_eq!(kinds.first(), Some(&"workflow.started"), "got: {kinds:?}");
  assert_eq!(kinds.last(), Some(&"workflow.completed"), "got: {kinds:?}");
  assert!(
    kinds.contains(&"node.started") && kinds.contains(&"node.completed"),
    "expected node lifecycle events, got: {kinds:?}"
  );
}

#[tokio::test]
async fn cancelling_a_distributed_run_reaches_cancelled_promptly() {
  let Some(url) = live_url() else {
    eprintln!("skipping cancelling_a_distributed_run_reaches_cancelled_promptly");
    return;
  };
  let db = Database::connect_and_migrate(&url, 4)
    .await
    .expect("connect");
  let state = AppState::new(db);

  let (_addr, plane, _shutdown_tx) = bind_worker_grpc().await;
  let state = state.with_worker_control_plane(Some(plane));

  // Deliberately no worker connects — this proves the gateway-side
  // "stop and mark cancelled" path (decision 4 of the W4.3b plan) works
  // independent of whatever the worker side would have done; it makes
  // no assertion about in-flight worker execution, which is the
  // documented, accepted limitation.
  let (status, body) = submit(
    state.clone(),
    json!({"workflow": TEMPLATE_WORKFLOW, "execution_mode": "distributed"}),
  )
  .await;
  assert_eq!(status, StatusCode::OK, "submit failed: {body}");
  let run_id = body["run_id"].as_str().expect("run_id").to_string();

  let cancel_response = create_router(state.clone())
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/runs/{run_id}:cancel"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(cancel_response.status(), StatusCode::OK);

  let terminal = tokio::time::timeout(
    Duration::from_secs(10),
    poll_until_terminal(&state, &run_id, Duration::from_secs(10)),
  )
  .await
  .expect("run must reach a terminal status within 10s");
  assert_eq!(terminal["status"], "cancelled", "got: {terminal}");
}
