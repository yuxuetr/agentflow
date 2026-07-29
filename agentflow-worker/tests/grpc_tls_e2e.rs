//! T1.2 end-to-end: a real, separately-compiled `agentflow-worker`
//! process connects to `agentflow-server`'s worker gRPC control plane
//! over mutual TLS and completes a claim/heartbeat/execute/report
//! cycle. This is the acceptance test for the "one server, N workers"
//! deployment shape described in `docs/DISTRIBUTED.md` actually working
//! end-to-end, not just the library-level wiring.
//!
//! Certificates are generated in-process (rcgen) — no fixture cert
//! files are checked into the repo.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agentflow_server::scheduler::WorkerTask;
use agentflow_server::worker_grpc::{
  WorkerGrpcServeConfig, WorkerGrpcTlsConfig, build_worker_control_plane, serve_worker_grpc,
};
use assert_cmd::Command;
use rcgen::{BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
use tempfile::TempDir;
use tokio::sync::oneshot;
use uuid::Uuid;

struct TlsMaterial {
  ca_cert_path: PathBuf,
  server_cert_path: PathBuf,
  server_key_path: PathBuf,
  client_cert_path: PathBuf,
  client_key_path: PathBuf,
}

fn write_pem(dir: &Path, name: &str, pem: &str) -> PathBuf {
  let path = dir.join(name);
  std::fs::write(&path, pem).unwrap();
  path
}

fn named_dn(common_name: &str) -> DistinguishedName {
  let mut dn = DistinguishedName::new();
  dn.push(DnType::CommonName, common_name);
  dn
}

/// Build a throwaway CA, a server leaf cert (SAN: 127.0.0.1, localhost),
/// and a client leaf cert, all signed by the same CA, and write every
/// PEM to `dir`.
fn generate_tls_material(dir: &Path) -> TlsMaterial {
  let mut ca_params = CertificateParams::new(Vec::new()).unwrap();
  ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
  ca_params.distinguished_name = named_dn("agentflow-test-ca");
  let ca_key = KeyPair::generate().unwrap();
  let ca_cert = ca_params.self_signed(&ca_key).unwrap();

  let mut server_params =
    CertificateParams::new(vec!["127.0.0.1".to_string(), "localhost".to_string()]).unwrap();
  server_params.distinguished_name = named_dn("agentflow-test-server");
  let server_key = KeyPair::generate().unwrap();
  let server_cert = server_params
    .signed_by(&server_key, &ca_cert, &ca_key)
    .unwrap();

  let mut client_params = CertificateParams::new(Vec::new()).unwrap();
  client_params.distinguished_name = named_dn("worker-e2e");
  let client_key = KeyPair::generate().unwrap();
  let client_cert = client_params
    .signed_by(&client_key, &ca_cert, &ca_key)
    .unwrap();

  TlsMaterial {
    ca_cert_path: write_pem(dir, "ca.pem", &ca_cert.pem()),
    server_cert_path: write_pem(dir, "server.pem", &server_cert.pem()),
    server_key_path: write_pem(dir, "server-key.pem", &server_key.serialize_pem()),
    client_cert_path: write_pem(dir, "client.pem", &client_cert.pem()),
    client_key_path: write_pem(dir, "client-key.pem", &client_key.serialize_pem()),
  }
}

/// Poll raw TCP reachability (not a TLS handshake — just "is anything
/// listening yet") before spawning the real worker process, which makes
/// exactly one connection attempt and gives up immediately on failure
/// (no built-in retry, unlike `WorkerRuntime::run_forever`).
async fn wait_until_listening(addr: std::net::SocketAddr) {
  for _ in 0..80 {
    if tokio::net::TcpStream::connect(addr).await.is_ok() {
      return;
    }
    tokio::time::sleep(Duration::from_millis(25)).await;
  }
  panic!("worker gRPC listener never became reachable at {addr}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_worker_process_completes_claim_report_cycle_over_mtls() {
  let tmp = TempDir::new().unwrap();
  let tls = generate_tls_material(tmp.path());

  let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = probe.local_addr().unwrap();
  drop(probe);

  let grpc_config = WorkerGrpcServeConfig {
    bind: addr,
    tls: Some(WorkerGrpcTlsConfig {
      cert_pem_path: tls.server_cert_path.clone(),
      key_pem_path: tls.server_key_path.clone(),
      // mTLS: only clients presenting a cert signed by this CA are
      // accepted at the transport layer, on top of the PSK admission
      // check at the application layer.
      client_ca_pem_path: Some(tls.ca_cert_path.clone()),
    }),
    allowed_worker_ids: vec!["worker-e2e".to_string()],
    shared_psk: Some("e2e-token".to_string()),
  };
  let plane =
    build_worker_control_plane(&grpc_config, agentflow_tools::SecurityProfile::Local).unwrap();

  let run_id = Uuid::new_v4();
  plane
    .inner()
    .schedule_task(WorkerTask::new(
      run_id,
      "node-a",
      serde_json::json!({"input": 1}),
    ))
    .await
    .unwrap();

  let (shutdown_tx, shutdown_rx) = oneshot::channel();
  let server_plane = plane.clone();
  let server_tls = grpc_config.tls.clone();
  let server = tokio::spawn(async move {
    serve_worker_grpc(addr, server_plane, server_tls, async {
      let _ = shutdown_rx.await;
    })
    .await
  });

  wait_until_listening(addr).await;

  // The real worker binary does blocking process I/O; run it on a
  // blocking-pool thread so it can't starve the tokio runtime the
  // spawned server task depends on (this test's `multi_thread` flavor
  // + 2 worker_threads is redundant-but-cheap belt-and-suspenders on
  // top of that).
  let ca_path = tls.ca_cert_path.clone();
  let client_cert_path = tls.client_cert_path.clone();
  let client_key_path = tls.client_key_path.clone();
  let control_plane_url = format!("grpc://{addr}");
  tokio::task::spawn_blocking(move || {
    Command::cargo_bin("agentflow-worker")
      .unwrap()
      .args([
        "--once",
        "--worker-id",
        "worker-e2e",
        "--control-plane",
        &control_plane_url,
        "--admission-token",
        "e2e-token",
        "--server-ca",
        ca_path.to_str().unwrap(),
        "--client-cert",
        client_cert_path.to_str().unwrap(),
        "--client-key",
        client_key_path.to_str().unwrap(),
      ])
      .timeout(Duration::from_secs(15))
      .assert()
      .success();
  })
  .await
  .unwrap();

  let snapshot = plane.inner().run_snapshot(run_id).await.unwrap();
  assert_eq!(
    snapshot.succeeded_tasks, 1,
    "the real worker process must have claimed, executed, and reported the task over mTLS"
  );

  let _ = shutdown_tx.send(());
  server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_worker_process_without_client_cert_is_rejected_by_mtls_handshake() {
  let tmp = TempDir::new().unwrap();
  let tls = generate_tls_material(tmp.path());

  let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = probe.local_addr().unwrap();
  drop(probe);

  let grpc_config = WorkerGrpcServeConfig {
    bind: addr,
    tls: Some(WorkerGrpcTlsConfig {
      cert_pem_path: tls.server_cert_path.clone(),
      key_pem_path: tls.server_key_path.clone(),
      client_ca_pem_path: Some(tls.ca_cert_path.clone()),
    }),
    allowed_worker_ids: vec!["worker-e2e".to_string()],
    shared_psk: Some("e2e-token".to_string()),
  };
  let plane =
    build_worker_control_plane(&grpc_config, agentflow_tools::SecurityProfile::Local).unwrap();

  let (shutdown_tx, shutdown_rx) = oneshot::channel();
  let server_plane = plane.clone();
  let server_tls = grpc_config.tls.clone();
  let server = tokio::spawn(async move {
    serve_worker_grpc(addr, server_plane, server_tls, async {
      let _ = shutdown_rx.await;
    })
    .await
  });

  wait_until_listening(addr).await;

  let ca_path = tls.ca_cert_path.clone();
  let control_plane_url = format!("grpc://{addr}");
  tokio::task::spawn_blocking(move || {
    // Only `--server-ca` is passed — no client cert/key, so the
    // server's mTLS client-cert requirement must reject the connection
    // before any admission/PSK check even runs.
    Command::cargo_bin("agentflow-worker")
      .unwrap()
      .args([
        "--once",
        "--worker-id",
        "worker-e2e",
        "--control-plane",
        &control_plane_url,
        "--admission-token",
        "e2e-token",
        "--server-ca",
        ca_path.to_str().unwrap(),
      ])
      .timeout(Duration::from_secs(15))
      .assert()
      .failure();
  })
  .await
  .unwrap();

  let _ = shutdown_tx.send(());
  server.await.unwrap().unwrap();
}
