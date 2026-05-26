//! Q3.1.2 + Q5.3 — shared Ctrl-C / SIGTERM helpers for CLI commands.
//!
//! Long-running CLI commands (`workflow run`, `harness run`, `skill chat`)
//! used to ignore SIGINT entirely: Ctrl-C aborted the tokio runtime
//! before in-flight trace events could be flushed, leaving the JSONL
//! file the CLI just told the operator to inspect missing its final
//! `WorkflowCancelled` event. This module gives every command the same
//! signal-aware pattern so cancellation is graceful and the exit code
//! is the POSIX-standard `128 + SIGINT = 130`.
//!
//! Q5.3: the actual `shutdown_signal()` future + `SIGINT_EXIT_CODE`
//! constant moved to `agentflow_core::shutdown` so the CLI, server,
//! and worker all share one implementation. The CLI keeps this
//! module as a thin re-export plus the CLI-specific
//! [`DEFAULT_TRACE_FLUSH_TIMEOUT`] constant — keeping the existing
//! `crate::shutdown::{shutdown_signal, SIGINT_EXIT_CODE}` import
//! sites compiling unchanged.

use std::time::Duration;

pub use agentflow_core::shutdown::{
  SIGINT_EXIT_CODE, SIGTERM_EXIT_CODE, ShutdownReason, shutdown_signal, shutdown_signal_with_reason,
};

/// How long the CLI will wait for the in-process trace drain task to
/// catch up before exiting after Ctrl-C. Bounded so a misbehaving
/// storage backend can't hang the shell indefinitely; the default is
/// generous enough for a healthy local filesystem write.
///
/// CLI-specific (not shared with server / worker), so it stays here
/// rather than moving to `agentflow-core::shutdown` along with
/// [`shutdown_signal`].
pub const DEFAULT_TRACE_FLUSH_TIMEOUT: Duration = Duration::from_secs(5);
