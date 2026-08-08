use clap::Args;

use super::execute;

#[derive(Args)]
pub struct ServeArgs {
  /// `host:port` to bind to (default: 127.0.0.1:8080, env: AGENTFLOW_SERVE_BIND)
  #[arg(long)]
  bind: Option<String>,
  /// Postgres URL (default env: DATABASE_URL)
  #[arg(long)]
  database_url: Option<String>,
  /// Postgres read-replica URL — routes get_*/list_* repo calls
  /// to a replica while writes go to `--database-url`. Defaults
  /// to env `AGENTFLOW_DATABASE_READ_URL`. Unset = reads use the
  /// primary (single-node default). P10.15.2.
  #[arg(long)]
  database_read_url: Option<String>,
  /// Workflow run-artifact root (env: AGENTFLOW_RUN_DIR)
  #[arg(long)]
  run_dir: Option<String>,
  /// Trace directory (env: AGENTFLOW_TRACE_DIR)
  #[arg(long)]
  trace_dir: Option<String>,
  /// Active security profile
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  security_profile: String,
  /// Name of the env var that carries the bearer auth token
  #[arg(long, default_value = "AGENTFLOW_API_TOKEN")]
  auth_token_env: String,
  /// Explicit CORS allow-list (comma-separated)
  #[arg(long, value_delimiter = ',')]
  cors_origins: Vec<String>,
  /// Maximum request body size in megabytes
  #[arg(long)]
  max_body_mb: Option<u64>,
  /// `host:port` to bind the worker gRPC control-plane listener on
  /// (e.g. `0.0.0.0:50051`). Unset (default) does not start it — the
  /// "one server, N workers" deployment shape from docs/DISTRIBUTED.md
  /// is opt-in. T1.2.
  #[arg(long)]
  worker_grpc: Option<String>,
  /// PEM server certificate for the worker gRPC listener. Must be
  /// paired with `--worker-grpc-tls-key`; omit both for plaintext gRPC.
  #[arg(long)]
  worker_grpc_tls_cert: Option<String>,
  /// PEM private key for `--worker-grpc-tls-cert`.
  #[arg(long)]
  worker_grpc_tls_key: Option<String>,
  /// PEM CA used to verify worker client certificates (enables mTLS on
  /// top of `--worker-grpc-tls-cert`/`--worker-grpc-tls-key`).
  #[arg(long)]
  worker_grpc_client_ca: Option<String>,
  /// Worker IDs allowed to join the gRPC control plane (comma-separated).
  /// Empty (default) allows any worker id — combined with no
  /// `--worker-psk`, that is fully open and only permitted outside
  /// `production` (T0.2's fail-closed admission check).
  #[arg(long, value_delimiter = ',')]
  worker_ids: Vec<String>,
  /// Shared pre-shared-key token every id in `--worker-ids` must present.
  #[arg(long)]
  worker_psk: Option<String>,
  /// Run readiness diagnostics without binding any sockets and exit
  #[arg(long)]
  check: bool,
}

pub async fn dispatch(args: ServeArgs) -> anyhow::Result<()> {
  execute(
    args.bind,
    args.database_url,
    args.database_read_url,
    args.run_dir,
    args.trace_dir,
    args.security_profile,
    args.auth_token_env,
    args.cors_origins,
    args.max_body_mb,
    args.worker_grpc,
    args.worker_grpc_tls_cert,
    args.worker_grpc_tls_key,
    args.worker_grpc_client_ca,
    args.worker_ids,
    args.worker_psk,
    args.check,
  )
  .await
}
