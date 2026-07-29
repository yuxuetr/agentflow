//! T1.2 — the worker gRPC control-plane listener.
//!
//! `agentflow-server` has always shipped the pieces needed to serve
//! [`WorkerControlServer`] (see `scheduler::grpc`), but nothing in the
//! binary ever bound a socket and started one: the "one control plane,
//! N workers" deployment shape described in `docs/DISTRIBUTED.md`
//! could not actually run. This module is the missing listener.
//!
//! Kept independent of [`crate::serve::run`]'s Postgres/`AppState`
//! dependency on purpose — it only needs an [`AuthenticatedControlPlane`]
//! and a bind address, so it's testable (and usable from a bare `#[tokio::test]`
//! or a minimal binary) without standing up the rest of the gateway.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

use crate::scheduler::{
  AuthenticatedControlPlane, AuthenticatedGrpcWorkerService, InMemoryWorkerProtocol,
  WorkerControlServer,
};

/// TLS material for the worker gRPC listener. `client_ca_pem_path` is
/// optional: set it to require and verify a client certificate (mTLS);
/// omit it to accept any TLS client (server-authentication only).
#[derive(Debug, Clone)]
pub struct WorkerGrpcTlsConfig {
  pub cert_pem_path: PathBuf,
  pub key_pem_path: PathBuf,
  pub client_ca_pem_path: Option<PathBuf>,
}

/// Configuration for [`serve_worker_grpc`]. Constructing this and
/// wiring it into [`crate::serve::ServeConfig::worker_grpc`] is what
/// turns the listener on — it stays off by default.
#[derive(Debug, Clone)]
pub struct WorkerGrpcServeConfig {
  /// `host:port` to bind the gRPC control plane on.
  pub bind: SocketAddr,
  /// `None` serves plaintext gRPC (same-host / trusted-network only).
  pub tls: Option<WorkerGrpcTlsConfig>,
  /// Worker IDs allowed to join. Empty means "any worker id" — combined
  /// with `shared_psk: None` that is the fully-open dev/local default;
  /// [`WorkerAdmissionPolicy::for_profile`](crate::scheduler::WorkerAdmissionPolicy::for_profile)
  /// still fails startup under `SecurityProfile::Production` if both are
  /// left empty (T0.2).
  pub allowed_worker_ids: Vec<String>,
  /// Shared pre-shared-key token every id in `allowed_worker_ids` must
  /// present (sent by the worker as `authorization: Bearer <token>`).
  /// A single shared secret rather than per-worker tokens — simple
  /// rotation-free operation for the common "trusted fleet" case;
  /// operators needing per-worker PSKs or JWT identity should construct
  /// a [`WorkerAdmissionPolicy`](crate::scheduler::WorkerAdmissionPolicy)
  /// directly instead of going through this config struct.
  pub shared_psk: Option<String>,
}

/// Errors from building or running the worker gRPC listener.
#[derive(Debug, thiserror::Error)]
pub enum WorkerGrpcError {
  #[error("failed to bind worker gRPC listener on {bind}: {source}")]
  Bind {
    bind: SocketAddr,
    #[source]
    source: std::io::Error,
  },
  #[error("failed to read TLS file '{path}': {source}")]
  TlsFile {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },
  #[error("invalid worker gRPC TLS configuration: {0}")]
  TlsConfig(String),
  #[error("worker admission configuration error: {0}")]
  Admission(#[from] crate::scheduler::AdmissionConfigError),
  #[error("worker gRPC server error: {0}")]
  Runtime(String),
}

/// Build the [`AuthenticatedControlPlane`] described by `config`,
/// validated against `profile`'s fail-closed requirement (T0.2's
/// `WorkerAdmissionPolicy::for_profile`).
pub fn build_worker_control_plane(
  config: &WorkerGrpcServeConfig,
  profile: agentflow_tools::SecurityProfile,
) -> Result<Arc<AuthenticatedControlPlane<InMemoryWorkerProtocol>>, WorkerGrpcError> {
  use crate::scheduler::{WorkerAdmissionPolicy, WorkerControlPlane, WorkerId};

  let mut policy = WorkerAdmissionPolicy::default();
  if !config.allowed_worker_ids.is_empty() {
    let ids: std::collections::HashSet<WorkerId> = config
      .allowed_worker_ids
      .iter()
      .filter_map(|id| WorkerId::new(id.clone()).ok())
      .collect();
    if let Some(token) = &config.shared_psk {
      for id in &ids {
        policy
          .pre_shared_keys
          .entry(id.clone())
          .or_default()
          .insert(token.clone());
      }
    }
    policy.allowed_workers = Some(ids);
  }
  let policy = policy.for_profile(profile)?;
  let plane = WorkerControlPlane::new(InMemoryWorkerProtocol::new());
  Ok(Arc::new(AuthenticatedControlPlane::new(plane, policy)))
}

/// Bind and serve the worker gRPC control plane until `shutdown`
/// resolves. Runs forever (or until an error / shutdown) — callers
/// typically `tokio::spawn` this as a background task alongside the
/// primary HTTP listener, the way [`crate::serve::run`] does.
pub async fn serve_worker_grpc(
  bind: SocketAddr,
  plane: Arc<AuthenticatedControlPlane<InMemoryWorkerProtocol>>,
  tls: Option<WorkerGrpcTlsConfig>,
  shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), WorkerGrpcError> {
  let mut builder = Server::builder();
  if let Some(tls) = tls {
    let cert_pem =
      std::fs::read(&tls.cert_pem_path).map_err(|source| WorkerGrpcError::TlsFile {
        path: tls.cert_pem_path.clone(),
        source,
      })?;
    let key_pem = std::fs::read(&tls.key_pem_path).map_err(|source| WorkerGrpcError::TlsFile {
      path: tls.key_pem_path.clone(),
      source,
    })?;
    let mut server_tls = ServerTlsConfig::new().identity(Identity::from_pem(cert_pem, key_pem));
    if let Some(ca_path) = &tls.client_ca_pem_path {
      let ca_pem = std::fs::read(ca_path).map_err(|source| WorkerGrpcError::TlsFile {
        path: ca_path.clone(),
        source,
      })?;
      server_tls = server_tls.client_ca_root(Certificate::from_pem(ca_pem));
    }
    builder = builder
      .tls_config(server_tls)
      .map_err(|err| WorkerGrpcError::TlsConfig(err.to_string()))?;
  }

  // Fail-fast bind probe before we hand the address to tonic (which
  // binds internally on first poll) — an operator who explicitly
  // configured this listener should see a bind error immediately, not
  // have it silently swallowed by whatever spawns this future.
  let probe = tokio::net::TcpListener::bind(bind)
    .await
    .map_err(|source| WorkerGrpcError::Bind { bind, source })?;
  drop(probe);

  builder
    .add_service(WorkerControlServer::new(
      AuthenticatedGrpcWorkerService::new(plane),
    ))
    .serve_with_shutdown(bind, shutdown)
    .await
    .map_err(|err| WorkerGrpcError::Runtime(err.to_string()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::scheduler::{GrpcWorkerProtocol, WorkerId, WorkerTask};
  use tokio::sync::oneshot;
  use uuid::Uuid;

  #[tokio::test]
  async fn serves_plaintext_and_completes_claim_heartbeat_report_cycle() {
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    // Port 0 means "any free port" for the bind, but tonic needs the
    // concrete port to build the client endpoint — reuse the same
    // bind-probe-then-serve idiom `serve_worker_grpc` itself uses.
    let probe = tokio::net::TcpListener::bind(bind).await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let config = WorkerGrpcServeConfig {
      bind: addr,
      tls: None,
      allowed_worker_ids: vec!["worker-a".to_string()],
      shared_psk: Some("test-token".to_string()),
    };
    let plane =
      build_worker_control_plane(&config, agentflow_tools::SecurityProfile::Local).unwrap();

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
    let server = tokio::spawn(serve_worker_grpc(addr, plane.clone(), None, async {
      let _ = shutdown_rx.await;
    }));

    let endpoint = format!("http://{addr}");
    let mut client = None;
    for _ in 0..20 {
      if let Ok(c) = GrpcWorkerProtocol::connect(&endpoint).await {
        client = Some(c);
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let client = client
      .expect("worker gRPC listener never became ready")
      .with_admission_token("test-token");

    let worker_id = WorkerId::new("worker-a").unwrap();
    use agentflow_worker_proto::WorkerProtocol;
    client
      .heartbeat(crate::scheduler::WorkerHeartbeat::now(
        worker_id.clone(),
        None,
        1,
      ))
      .await
      .unwrap();

    let claimed = client
      .claim_task(worker_id.clone())
      .await
      .unwrap()
      .expect("task must be claimable");
    assert_eq!(claimed.node_id, "node-a");

    client
      .report_result(
        worker_id,
        claimed.task_id,
        agentflow_worker_proto::WorkerTaskResult::Succeeded {
          output: serde_json::json!({"ok": true}),
          events: Vec::new(),
        },
      )
      .await
      .unwrap();

    let snapshot = plane.inner().run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.succeeded_tasks, 1);

    let _ = shutdown_tx.send(());
    server.await.unwrap().unwrap();
  }

  #[tokio::test]
  async fn production_profile_rejects_worker_grpc_with_no_credentials_configured() {
    let config = WorkerGrpcServeConfig {
      bind: "127.0.0.1:0".parse().unwrap(),
      tls: None,
      allowed_worker_ids: Vec::new(),
      shared_psk: None,
    };
    let err = build_worker_control_plane(&config, agentflow_tools::SecurityProfile::Production)
      .unwrap_err();
    assert!(matches!(err, WorkerGrpcError::Admission(_)));
  }
}
