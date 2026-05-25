//! Q3.1.2 — shared Ctrl-C / SIGTERM helpers for CLI commands.
//!
//! Long-running CLI commands (`workflow run`, `harness run`, `skill chat`)
//! used to ignore SIGINT entirely: Ctrl-C aborted the tokio runtime
//! before in-flight trace events could be flushed, leaving the JSONL
//! file the CLI just told the operator to inspect missing its final
//! `WorkflowCancelled` event. This module gives every command the same
//! signal-aware pattern so cancellation is graceful and the exit code
//! is the POSIX-standard `128 + SIGINT = 130`.

use std::time::Duration;

/// POSIX exit code for "terminated by SIGINT" (= `128 + 2`). Use this
/// from CLI handlers that exit because the user pressed Ctrl-C.
pub const SIGINT_EXIT_CODE: i32 = 130;

/// How long the CLI will wait for the in-process trace drain task to
/// catch up before exiting after Ctrl-C. Bounded so a misbehaving
/// storage backend can't hang the shell indefinitely; the default is
/// generous enough for a healthy local filesystem write.
pub const DEFAULT_TRACE_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

/// Future that resolves on the first SIGINT (Ctrl-C) on any platform,
/// or SIGTERM on unix (so k8s `terminationGracePeriodSeconds` works
/// when the CLI is invoked from a container entrypoint).
///
/// Mirrors `agentflow_server::serve::shutdown_signal` but lives here
/// so the CLI doesn't pull the whole server crate in.
pub async fn shutdown_signal() {
  let ctrl_c = async {
    if let Err(err) = tokio::signal::ctrl_c().await {
      // We can't install the handler at all (very unusual — typically
      // means there's no signal-capable runtime). Block forever so the
      // outer `tokio::select!` falls through to the other branch.
      tracing::error!(error = %err, "failed to install ctrl_c handler");
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
        tracing::error!(error = %err, "failed to install SIGTERM handler");
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
