use std::time::Duration;

use agentflow_worker::{WorkerConfig, WorkerRuntime};
use agentflow_worker_proto::{
  Certificate, ClientTlsConfig, GrpcWorkerProtocol, Identity, InMemoryWorkerProtocol, WorkerId,
};

#[tokio::main]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("agentflow-worker: {e}");
    std::process::exit(1);
  }
}

/// Q3.1.3 + Q5.3: thin re-export of the shared
/// `agentflow_core::shutdown::shutdown_signal` future. Keeps the
/// local binary callsite stable while moving the actual SIGINT /
/// SIGTERM handling into the workspace-shared helper introduced in
/// Q5.3.
use agentflow_core::shutdown::shutdown_signal;

async fn run() -> Result<(), String> {
  let args = Args::parse(std::env::args().skip(1))?;
  // T1.2: build the TLS config (if any) up front, before any field of
  // `args` moves out — `build_tls_config` only borrows.
  let tls_config = build_tls_config(&args)?;
  let worker_id = WorkerId::new(args.worker_id).map_err(|e| e.to_string())?;
  let mut config = WorkerConfig::new(worker_id, args.control_plane.clone());
  config.poll_interval = args.poll_interval;
  config.heartbeat_interval = args.heartbeat_interval;

  if config.control_plane == "memory://local" {
    if args.admission_token.is_some() {
      eprintln!(
        "agentflow-worker: warning — --admission-token / AGENTFLOW_ADMISSION_TOKEN is set but \
         control-plane is memory://local, which is auth-exempt. The token will be ignored."
      );
    }
    if tls_config.is_some() {
      eprintln!(
        "agentflow-worker: warning — TLS flags are set but control-plane is memory://local, \
         which is in-process and never uses TLS. They will be ignored."
      );
    }
    let runtime = WorkerRuntime::new(InMemoryWorkerProtocol::new(), config);
    return run_runtime(runtime, args.once).await;
  }

  let endpoint = grpc_endpoint(&config.control_plane, tls_config.is_some())?;
  let mut protocol = match tls_config {
    Some(tls) => GrpcWorkerProtocol::connect_tls(&endpoint, tls)
      .await
      .map_err(|e| e.to_string())?,
    None => GrpcWorkerProtocol::connect(&endpoint)
      .await
      .map_err(|e| e.to_string())?,
  };
  // Q1.6.1: attach the admission credential (PSK) sent as
  // `authorization: Bearer <token>` gRPC metadata. Production
  // deployments MUST set this; the server side rejects with
  // permission_denied otherwise. CLI takes precedence over env so
  // ops can rotate via systemd unit files without restarting.
  if let Some(token) = args.admission_token.as_deref() {
    protocol = protocol.with_admission_token(token);
  } else {
    eprintln!(
      "agentflow-worker: warning — no --admission-token / AGENTFLOW_ADMISSION_TOKEN configured. \
       The server will reject every RPC with permission_denied if it runs \
       AuthenticatedGrpcWorkerService."
    );
  }
  let runtime = WorkerRuntime::new(protocol, config);
  run_runtime(runtime, args.once).await
}

/// T1.2: build a `ClientTlsConfig` from `--server-ca`/`--client-cert`/
/// `--client-key` PEM file paths. Returns `Ok(None)` when none of the
/// three flags are set (plaintext gRPC, the historical default).
/// `--client-cert`/`--client-key` must both be present or both absent —
/// a lone one of the pair is a config error, not silently-ignored mTLS.
fn build_tls_config(args: &Args) -> Result<Option<ClientTlsConfig>, String> {
  if args.server_ca.is_none() && args.client_cert.is_none() && args.client_key.is_none() {
    return Ok(None);
  }

  let mut tls = ClientTlsConfig::new();
  if let Some(path) = &args.server_ca {
    let pem = std::fs::read(path)
      .map_err(|e| format!("agentflow-worker: failed to read --server-ca '{path}': {e}"))?;
    tls = tls.ca_certificate(Certificate::from_pem(pem));
  }
  match (&args.client_cert, &args.client_key) {
    (Some(cert_path), Some(key_path)) => {
      let cert_pem = std::fs::read(cert_path).map_err(|e| {
        format!("agentflow-worker: failed to read --client-cert '{cert_path}': {e}")
      })?;
      let key_pem = std::fs::read(key_path)
        .map_err(|e| format!("agentflow-worker: failed to read --client-key '{key_path}': {e}"))?;
      tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
    }
    (None, None) => {}
    _ => {
      return Err(
        "agentflow-worker: --client-cert and --client-key must both be set for mTLS, or both omitted"
          .to_string(),
      );
    }
  }
  Ok(Some(tls))
}

async fn run_runtime<P>(runtime: WorkerRuntime<P>, once: bool) -> Result<(), String>
where
  // Q3.3.2: `run_forever` spawns concurrent dispatches and needs
  // `P: Clone + Send + 'static` to share the protocol handle across
  // spawned tasks. Both `InMemoryWorkerProtocol` and
  // `GrpcWorkerProtocol` already derive `Clone`.
  P: agentflow_worker_proto::WorkerProtocol + Clone + Send + 'static,
{
  if once {
    let _ = runtime.run_once().await.map_err(|e| e.to_string())?;
    return Ok(());
  }

  // Q3.1.3: install a SIGINT/SIGTERM hook so k8s rolling deploys (and
  // local Ctrl-C) can drain the runtime instead of `SIGKILL`-ing an
  // in-flight node execution. `run_forever` exits naturally once the
  // cancellation flag flips; we then award the loop a bounded
  // drain window for the current dispatch to settle.
  let cancel = runtime.cancellation_token();
  let run_future = runtime.run_forever();
  tokio::pin!(run_future);
  let signal_future = shutdown_signal();
  tokio::pin!(signal_future);

  tokio::select! {
    biased;
    res = &mut run_future => res.map_err(|e| e.to_string()),
    _ = &mut signal_future => {
      eprintln!("agentflow-worker: received SIGINT/SIGTERM; beginning graceful drain");
      cancel.cancel();
      // 30s is generous — even an LLM node usually finishes within
      // its own timeout (default 300s in `WorkerResourceLimits` but
      // ops typically set lower). If the worker is still busy when
      // the deadline expires, the supervisor will follow up with
      // SIGKILL; the in-flight task is reported as `cancelled` by
      // `execute_stub` so the scheduler requeues immediately
      // instead of waiting the stale-heartbeat timeout.
      const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
      match tokio::time::timeout(DRAIN_TIMEOUT, &mut run_future).await {
        Ok(res) => res.map_err(|e| e.to_string()),
        Err(_) => {
          eprintln!(
            "agentflow-worker: drain timed out after {DRAIN_TIMEOUT:?}; exiting anyway"
          );
          Ok(())
        }
      }
    }
  }
}

/// T1.2: `grpc://host:port` maps to `https://` when TLS is configured
/// (tonic rejects a `tls_config` paired with an `http://` endpoint), and
/// to `http://` otherwise — unchanged from the pre-T1.2 behavior. An
/// explicit `http://`/`https://` control-plane URL always passes through
/// as-is, so an operator who wants TLS without the `grpc://` shorthand
/// can just write `https://host:port` directly.
fn grpc_endpoint(control_plane: &str, tls_enabled: bool) -> Result<String, String> {
  if let Some(rest) = control_plane.strip_prefix("grpc://") {
    let scheme = if tls_enabled { "https" } else { "http" };
    return Ok(format!("{scheme}://{rest}"));
  }
  if control_plane.starts_with("http://") || control_plane.starts_with("https://") {
    return Ok(control_plane.to_string());
  }
  Err(format!(
    "control plane '{control_plane}' is unsupported; use memory://local, grpc://host:port, or http(s)://host:port"
  ))
}

#[derive(Debug)]
struct Args {
  worker_id: String,
  control_plane: String,
  once: bool,
  poll_interval: Duration,
  heartbeat_interval: Duration,
  /// Q1.6.1: pre-shared admission token sent as gRPC `authorization`
  /// metadata. Falls back to `AGENTFLOW_ADMISSION_TOKEN`.
  admission_token: Option<String>,
  /// Path to a PEM-encoded CA certificate used to validate the
  /// server's cert. Set alone, this enables server-only TLS; paired
  /// with `client_cert`/`client_key`, mutual TLS.
  server_ca: Option<String>,
  /// Path to a PEM-encoded client certificate (mTLS). Must be set
  /// together with `client_key`.
  client_cert: Option<String>,
  /// Path to the PEM-encoded private key for `client_cert`.
  client_key: Option<String>,
}

impl Args {
  fn parse<I>(mut args: I) -> Result<Self, String>
  where
    I: Iterator<Item = String>,
  {
    let mut parsed = Self {
      worker_id: std::env::var("AGENTFLOW_WORKER_ID").unwrap_or_else(|_| "worker-local".into()),
      control_plane: std::env::var("AGENTFLOW_CONTROL_PLANE")
        .unwrap_or_else(|_| "memory://local".into()),
      once: false,
      poll_interval: Duration::from_millis(250),
      heartbeat_interval: Duration::from_secs(5),
      admission_token: std::env::var("AGENTFLOW_ADMISSION_TOKEN").ok(),
      server_ca: std::env::var("AGENTFLOW_WORKER_SERVER_CA").ok(),
      client_cert: std::env::var("AGENTFLOW_WORKER_CLIENT_CERT").ok(),
      client_key: std::env::var("AGENTFLOW_WORKER_CLIENT_KEY").ok(),
    };

    while let Some(arg) = args.next() {
      match arg.as_str() {
        "--worker-id" => {
          parsed.worker_id = args
            .next()
            .ok_or_else(|| "--worker-id requires a value".to_string())?;
        }
        "--control-plane" => {
          parsed.control_plane = args
            .next()
            .ok_or_else(|| "--control-plane requires a value".to_string())?;
        }
        "--admission-token" => {
          parsed.admission_token = Some(
            args
              .next()
              .ok_or_else(|| "--admission-token requires a value".to_string())?,
          );
        }
        "--server-ca" => {
          parsed.server_ca = Some(
            args
              .next()
              .ok_or_else(|| "--server-ca requires a path to a PEM file".to_string())?,
          );
        }
        "--client-cert" => {
          parsed.client_cert = Some(
            args
              .next()
              .ok_or_else(|| "--client-cert requires a path to a PEM file".to_string())?,
          );
        }
        "--client-key" => {
          parsed.client_key = Some(
            args
              .next()
              .ok_or_else(|| "--client-key requires a path to a PEM file".to_string())?,
          );
        }
        "--once" => parsed.once = true,
        "--poll-ms" => {
          let value = args
            .next()
            .ok_or_else(|| "--poll-ms requires a value".to_string())?;
          let millis = value
            .parse::<u64>()
            .map_err(|_| "--poll-ms must be an integer".to_string())?;
          parsed.poll_interval = Duration::from_millis(millis);
        }
        "--heartbeat-ms" => {
          let value = args
            .next()
            .ok_or_else(|| "--heartbeat-ms requires a value".to_string())?;
          let millis = value
            .parse::<u64>()
            .map_err(|_| "--heartbeat-ms must be an integer".to_string())?;
          parsed.heartbeat_interval = Duration::from_millis(millis);
        }
        "--help" | "-h" => return Err(help()),
        other => return Err(format!("unknown argument: {other}\n{}", help())),
      }
    }
    Ok(parsed)
  }
}

fn help() -> String {
  "usage: agentflow-worker \
   [--worker-id ID] \
   [--control-plane memory://local|grpc://host:port|http://host:port] \
   [--admission-token PSK]   # required for gRPC under AuthenticatedGrpcWorkerService \
   [--server-ca PATH]        # PEM CA cert; enables TLS (grpc:// -> https://) \
   [--client-cert PATH]      # PEM client cert; pair with --client-key for mTLS \
   [--client-key PATH]       # PEM client key; pair with --client-cert for mTLS \
   [--once] [--poll-ms N] [--heartbeat-ms N]"
    .into()
}
