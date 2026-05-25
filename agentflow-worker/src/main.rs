use std::time::Duration;

use agentflow_server::{GrpcWorkerProtocol, InMemoryWorkerProtocol, WorkerId};
use agentflow_worker::{WorkerConfig, WorkerRuntime};

#[tokio::main]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("agentflow-worker: {e}");
    std::process::exit(1);
  }
}

/// Q3.1.3: future that resolves on SIGINT (all platforms) or SIGTERM
/// (unix). Mirrors the helper in `agentflow-cli::shutdown` and the
/// server's `shutdown_signal`. Kept private to the binary so the
/// library has zero direct dependency on `tokio::signal`.
async fn shutdown_signal() {
  let ctrl_c = async {
    if let Err(err) = tokio::signal::ctrl_c().await {
      eprintln!("agentflow-worker: failed to install ctrl_c handler: {err}");
      std::future::pending::<()>().await;
    }
  };

  #[cfg(unix)]
  let terminate = async {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
      Ok(mut sigterm) => {
        let _ = sigterm.recv().await;
      }
      Err(err) => {
        eprintln!("agentflow-worker: failed to install SIGTERM handler: {err}");
        std::future::pending::<()>().await;
      }
    }
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
    _ = ctrl_c => {}
    _ = terminate => {}
  }
}

async fn run() -> Result<(), String> {
  let args = Args::parse(std::env::args().skip(1))?;
  let worker_id = WorkerId::new(args.worker_id).map_err(|e| e.to_string())?;
  let mut config = WorkerConfig::new(worker_id, args.control_plane);
  config.poll_interval = args.poll_interval;
  config.heartbeat_interval = args.heartbeat_interval;

  if config.control_plane == "memory://local" {
    if args.admission_token.is_some() {
      eprintln!(
        "agentflow-worker: warning — --admission-token / AGENTFLOW_ADMISSION_TOKEN is set but \
         control-plane is memory://local, which is auth-exempt. The token will be ignored."
      );
    }
    let runtime = WorkerRuntime::new(InMemoryWorkerProtocol::new(), config);
    return run_runtime(runtime, args.once).await;
  }

  let endpoint = grpc_endpoint(&config.control_plane)?;
  let mut protocol = GrpcWorkerProtocol::connect(&endpoint)
    .await
    .map_err(|e| e.to_string())?;
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
  // TLS flags are accepted for CLI compatibility; current tonic
  // wiring uses the channel as-is. Operators provide their own
  // certificate material — no in-tree cert generation script.
  if args.server_ca.is_some() || args.client_cert.is_some() || args.client_key.is_some() {
    eprintln!(
      "agentflow-worker: warning — TLS flags are accepted but not yet wired through the \
       channel builder. Track Q3.x for the full mTLS uplift."
    );
  }
  let runtime = WorkerRuntime::new(protocol, config);
  run_runtime(runtime, args.once).await
}

async fn run_runtime<P>(runtime: WorkerRuntime<P>, once: bool) -> Result<(), String>
where
  P: agentflow_server::WorkerProtocol,
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

fn grpc_endpoint(control_plane: &str) -> Result<String, String> {
  if let Some(rest) = control_plane.strip_prefix("grpc://") {
    return Ok(format!("http://{rest}"));
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
  /// PEM-encoded CA certificate used to validate the server cert.
  /// Accepted today as a CLI surface but not yet wired into the
  /// tonic channel — full mTLS lands in a follow-up.
  server_ca: Option<String>,
  client_cert: Option<String>,
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
   [--server-ca PATH]        # PEM CA cert (TLS, not yet wired) \
   [--client-cert PATH]      # PEM client cert (mTLS, not yet wired) \
   [--client-key PATH]       # PEM client key (mTLS, not yet wired) \
   [--once] [--poll-ms N] [--heartbeat-ms N]"
    .into()
}
