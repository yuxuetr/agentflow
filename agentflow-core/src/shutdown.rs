//! Q5.3: shared `SIGINT` / `SIGTERM` shutdown handling.
//!
//! Pre-Q5.3 the CLI, server, and worker each kept their own near-
//! identical copy of `async fn shutdown_signal()`, including small but
//! load-bearing differences:
//!
//! - `agentflow-server::serve::shutdown_signal` panicked via
//!   `.expect("install … signal handler")` if signal installation
//!   failed — a Q5.1 unwrap site in the production gateway path.
//! - `agentflow-cli::shutdown::shutdown_signal` logged via
//!   `tracing::error!` and fell through to `pending::<()>()`.
//! - `agentflow-worker::main::shutdown_signal` used `eprintln!` and
//!   the same fall-through.
//!
//! Centralising the helper here:
//!
//! 1. eliminates the panic in the server path (handler-install
//!    failure now logs and degrades to "no signal handler" rather
//!    than crashing the process),
//! 2. gives every binary the same SIGINT-vs-SIGTERM semantics (and
//!    the same exit-code convention via [`SIGINT_EXIT_CODE`] /
//!    [`SIGTERM_EXIT_CODE`]),
//! 3. and gives callers that need to distinguish the two a
//!    structured [`ShutdownReason`] return without forcing every
//!    callsite to upgrade.
//!
//! The helper depends only on `tokio::signal`, which `agentflow-core`
//! already pulls in via `tokio = { features = ["full"] }`.

/// POSIX exit code for "terminated by SIGINT" (= `128 + 2`).
///
/// Use this from binaries that exit because the user pressed Ctrl-C
/// so shells and CI harnesses report the standard "interrupted"
/// signal. Pre-Q5.3 only `agentflow-cli` exposed this; the worker
/// + server bypassed exit codes entirely.
pub const SIGINT_EXIT_CODE: i32 = 130;

/// POSIX exit code for "terminated by SIGTERM" (= `128 + 15`).
///
/// Kubernetes / systemd / supervisord deliver `SIGTERM` for graceful
/// shutdown. Long-running daemons should exit with this code rather
/// than `0` so orchestrators can distinguish "completed cleanly" from
/// "drained on shutdown signal."
pub const SIGTERM_EXIT_CODE: i32 = 143;

/// Why [`shutdown_signal_with_reason`] resolved. Callers map this to
/// log lines / exit codes / cleanup behavior; the basic
/// [`shutdown_signal`] entry point discards the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownReason {
  /// Received `SIGINT` (Ctrl-C). Maps to exit code
  /// [`SIGINT_EXIT_CODE`].
  Interrupt,
  /// Received `SIGTERM`. Maps to exit code [`SIGTERM_EXIT_CODE`].
  /// On non-unix targets this variant is unreachable.
  Terminate,
}

impl ShutdownReason {
  /// Convenience: return the canonical POSIX exit code for this
  /// shutdown reason. Callers that distinguish reasons (e.g. the CLI
  /// reporting Ctrl-C to a shell pipeline) use this instead of
  /// hardcoding 130 / 143 at every callsite.
  pub fn exit_code(self) -> i32 {
    match self {
      Self::Interrupt => SIGINT_EXIT_CODE,
      Self::Terminate => SIGTERM_EXIT_CODE,
    }
  }
}

/// Future that resolves on the first `SIGINT` (any platform) or
/// `SIGTERM` (unix only). The returned [`ShutdownReason`] tells
/// callers which signal arrived first.
///
/// Failure modes are non-fatal: if installing a handler fails
/// (extremely rare — typically means there is no signal-capable
/// runtime, e.g. inside an unusual `tokio::test` harness), the
/// failing branch falls through to `pending::<()>()` rather than
/// panicking. The other branch can still resolve the future. If
/// **both** branches fail, the future never resolves — which is the
/// correct "no shutdown signal will ever arrive" behavior, even if
/// it means the host binary loses its graceful-drain hook.
pub async fn shutdown_signal_with_reason() -> ShutdownReason {
  let ctrl_c = async {
    if let Err(err) = tokio::signal::ctrl_c().await {
      // Use eprintln! (not tracing!) because `agentflow-core` keeps
      // `tracing` behind an optional feature flag — the helper has to
      // work in any consumer regardless of feature mix. Install
      // failure here is extremely rare anyway: it means the runtime
      // refused to install a SIGINT handler at all.
      eprintln!("agentflow: failed to install ctrl_c handler: {err}");
      std::future::pending::<()>().await;
    }
    ShutdownReason::Interrupt
  };

  #[cfg(unix)]
  let terminate = async {
    use tokio::signal::unix::{SignalKind, signal};
    match signal(SignalKind::terminate()) {
      Ok(mut sigterm) => {
        let _ = sigterm.recv().await;
      }
      Err(err) => {
        eprintln!("agentflow: failed to install SIGTERM handler: {err}");
        std::future::pending::<()>().await;
      }
    }
    ShutdownReason::Terminate
  };

  #[cfg(not(unix))]
  let terminate = async {
    std::future::pending::<()>().await;
    ShutdownReason::Terminate
  };

  tokio::select! {
    reason = ctrl_c => reason,
    reason = terminate => reason,
  }
}

/// Convenience wrapper for callers that don't need to distinguish
/// `SIGINT` from `SIGTERM`. Equivalent to
/// `shutdown_signal_with_reason().await` with the result discarded.
///
/// Use this in `tokio::select!` branches that just need to break
/// out of a long-running async loop on any termination signal.
///
/// ## Usage template for graceful drain on signal
///
/// The canonical integration pattern, as used by `agentflow-cli`,
/// `agentflow-server`, and `agentflow-worker` after Q5.3:
///
/// ```ignore
/// use agentflow_core::shutdown::{shutdown_signal_with_reason, ShutdownReason};
///
/// tokio::select! {
///     _ = run_long_running_work() => { /* completed naturally */ }
///     reason = shutdown_signal_with_reason() => {
///         match reason {
///             ShutdownReason::Interrupt => {
///                 // Ctrl-C — user is at the terminal. Print a
///                 // human-readable line to stderr, then drain.
///                 eprintln!("interrupted; draining…");
///             }
///             ShutdownReason::Terminate => {
///                 // SIGTERM — typically k8s `terminationGracePeriodSeconds`
///                 // or systemd. Log at info level so the orchestrator's
///                 // log scraper picks it up.
///                 tracing::info!("SIGTERM; draining…");
///             }
///         }
///         drain_in_flight_work().await;
///         std::process::exit(reason.exit_code());
///     }
/// }
/// # async fn run_long_running_work() {}
/// # async fn drain_in_flight_work() {}
/// ```
///
/// For SIGTERM-specific integration tests against actual binaries
/// (subprocess + `kill`), see the worker integration suite —
/// pure-future tests should mock the `select!` arm rather than
/// trying to deliver real signals to `cargo test`'s host process,
/// which would also signal every other in-flight test.
pub async fn shutdown_signal() {
  let _ = shutdown_signal_with_reason().await;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn posix_exit_codes_match_128_plus_signal_number() {
    assert_eq!(SIGINT_EXIT_CODE, 128 + 2, "SIGINT is signal number 2");
    assert_eq!(SIGTERM_EXIT_CODE, 128 + 15, "SIGTERM is signal number 15");
  }

  #[test]
  fn shutdown_reason_exit_code_round_trips_to_constants() {
    assert_eq!(ShutdownReason::Interrupt.exit_code(), SIGINT_EXIT_CODE);
    assert_eq!(ShutdownReason::Terminate.exit_code(), SIGTERM_EXIT_CODE);
  }

  #[test]
  fn shutdown_reason_supports_basic_equality() {
    // Pin the derive(PartialEq, Eq) since downstream callers (the
    // CLI reporter) match on the variant to render an info-level
    // log distinguishing the two signal types.
    assert_eq!(ShutdownReason::Interrupt, ShutdownReason::Interrupt);
    assert_ne!(ShutdownReason::Interrupt, ShutdownReason::Terminate);
  }

  /// `shutdown_signal_with_reason()` resolves to the reason that
  /// fired first. We cannot actually deliver SIGTERM to the test
  /// process without affecting other tests (cargo test runs lots of
  /// threads in one process), so the runtime behavior is exercised
  /// at the integration level by the binaries that consume this
  /// helper. Here we just prove the API surface compiles and the
  /// `await` returns a `ShutdownReason`.
  #[test]
  fn signature_returns_shutdown_reason() {
    fn _check<F>(_f: F)
    where
      F: std::future::Future<Output = ShutdownReason>,
    {
    }
    _check(shutdown_signal_with_reason());
  }

  /// `shutdown_signal()` discards the reason; pin the bare-`()`
  /// signature so callers using it inside `tokio::select!` don't
  /// silently break when the helper evolves.
  #[test]
  fn convenience_signature_returns_unit() {
    fn _check<F>(_f: F)
    where
      F: std::future::Future<Output = ()>,
    {
    }
    _check(shutdown_signal());
  }
}
