//! Race a future against an optional deadline and an optional cancellation
//! signal.
//!
//! Agent runtimes repeatedly need the same shape: run an operation (an LLM
//! round-trip, a tool call) but stop early if a wall-clock budget elapses or a
//! cancellation token fires. Hand-written, this is a four-arm match over
//! `(Option<Duration>, Option<CancelSignal>)` with two nested `tokio::select!`
//! blocks — and every call site duplicated the timeout- and cancel-handling
//! branches. [`race_with_limits`] captures the control flow once and returns a
//! [`RaceOutcome`] so each call site only writes its own per-outcome handling.

use std::future::Future;
use std::time::Duration;

/// The result of racing a future against its limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceOutcome<T> {
  /// The future finished before any limit fired; carries its output.
  Completed(T),
  /// The wall-clock deadline elapsed first.
  TimedOut,
  /// The cancellation signal fired first.
  Cancelled,
}

/// Run `fut`, stopping early if `remaining` elapses or `cancel` resolves.
///
/// - `remaining` is the time budget left for this operation; `None` means no
///   wall-clock limit.
/// - `cancel` is a future that resolves when the operation should be cancelled
///   (e.g. `token.cancelled()`); `None` means the operation cannot be cancelled.
///
/// Returns [`RaceOutcome::Completed`] with the future's output, or
/// [`RaceOutcome::TimedOut`] / [`RaceOutcome::Cancelled`] when a limit won the
/// race. When both a deadline and a cancellation are armed, the two are polled
/// without bias (matching the hand-written `tokio::select!` blocks this
/// replaces): whichever is ready first wins, ties broken arbitrarily.
pub async fn race_with_limits<F, C>(
  fut: F,
  remaining: Option<Duration>,
  cancel: Option<C>,
) -> RaceOutcome<F::Output>
where
  F: Future,
  C: Future<Output = ()>,
{
  match (remaining, cancel) {
    (Some(deadline), Some(cancel)) => {
      tokio::select! {
        result = tokio::time::timeout(deadline, fut) => match result {
          Ok(value) => RaceOutcome::Completed(value),
          Err(_) => RaceOutcome::TimedOut,
        },
        _ = cancel => RaceOutcome::Cancelled,
      }
    }
    (Some(deadline), None) => match tokio::time::timeout(deadline, fut).await {
      Ok(value) => RaceOutcome::Completed(value),
      Err(_) => RaceOutcome::TimedOut,
    },
    (None, Some(cancel)) => {
      tokio::select! {
        value = fut => RaceOutcome::Completed(value),
        _ = cancel => RaceOutcome::Cancelled,
      }
    }
    (None, None) => RaceOutcome::Completed(fut.await),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  /// A future that never resolves — stands in for a long-running op so a limit
  /// deterministically wins the race.
  async fn never() -> &'static str {
    std::future::pending::<()>().await;
    "unreachable"
  }

  /// A cancellation signal that fires after a short delay. `tokio::time::sleep`
  /// already resolves to `()`, so it is a ready-made `Future<Output = ()>`.
  fn cancel_after(ms: u64) -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_millis(ms))
  }

  /// A cancellation signal that never fires.
  fn never_cancel() -> tokio::time::Sleep {
    tokio::time::sleep(Duration::from_secs(30))
  }

  #[tokio::test]
  async fn completes_when_no_limits_armed() {
    let outcome = race_with_limits(async { 7 }, None, None::<tokio::time::Sleep>).await;
    assert_eq!(outcome, RaceOutcome::Completed(7));
  }

  #[tokio::test]
  async fn completes_within_deadline() {
    let outcome = race_with_limits(
      async { "ok" },
      Some(Duration::from_secs(30)),
      None::<tokio::time::Sleep>,
    )
    .await;
    assert_eq!(outcome, RaceOutcome::Completed("ok"));
  }

  #[tokio::test]
  async fn times_out_when_deadline_elapses() {
    let outcome = race_with_limits(
      never(),
      Some(Duration::from_millis(20)),
      None::<tokio::time::Sleep>,
    )
    .await;
    assert_eq!(outcome, RaceOutcome::TimedOut);
  }

  #[tokio::test]
  async fn cancels_when_signal_fires_without_deadline() {
    let outcome = race_with_limits(never(), None, Some(cancel_after(10))).await;
    assert_eq!(outcome, RaceOutcome::Cancelled);
  }

  #[tokio::test]
  async fn cancels_when_signal_fires_before_deadline() {
    // Long deadline so cancellation wins the race deterministically.
    let outcome = race_with_limits(
      never(),
      Some(Duration::from_secs(30)),
      Some(cancel_after(10)),
    )
    .await;
    assert_eq!(outcome, RaceOutcome::Cancelled);
  }

  #[tokio::test]
  async fn completes_before_either_limit_fires() {
    // Signal effectively never fires; deadline is generous; the future wins.
    let outcome = race_with_limits(
      async { 42 },
      Some(Duration::from_secs(30)),
      Some(never_cancel()),
    )
    .await;
    assert_eq!(outcome, RaceOutcome::Completed(42));
  }
}
