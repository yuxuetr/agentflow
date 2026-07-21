use async_trait::async_trait;
use serde_json::Value;

/// Input available to verification strategies.
///
/// A verification runs against a *candidate* final answer — one the loop
/// is about to terminate with — and decides whether the loop should
/// actually stop there or take another attempt. Contrast with
/// [`crate::reflection::ReflectionContext`], which only observes a
/// decision that has already been made.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationContext {
  /// Session id for the run being verified.
  pub session_id: String,
  /// Step index at which the candidate answer was produced.
  pub step_index: usize,
  /// The original user input that started the run.
  pub user_input: String,
  /// The model's reasoning text leading to the candidate answer, when
  /// available.
  pub thought: Option<String>,
  /// The candidate final answer under review.
  pub answer: String,
  /// 1-based attempt number: `1` for the first candidate answer, `2` for
  /// the answer produced after one rejection, and so on.
  pub attempt: usize,
  /// Free-form structured metadata supplied by the runtime.
  pub metadata: Value,
}

impl VerificationContext {
  /// Build a context for a candidate final answer.
  pub fn for_answer(
    session_id: impl Into<String>,
    step_index: usize,
    user_input: impl Into<String>,
    answer: impl Into<String>,
    attempt: usize,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      step_index,
      user_input: user_input.into(),
      thought: None,
      answer: answer.into(),
      attempt,
      metadata: Value::Object(Default::default()),
    }
  }

  /// Attach the reasoning thought that led to the candidate answer.
  pub fn with_thought(mut self, thought: impl Into<String>) -> Self {
    self.thought = Some(thought.into());
    self
  }
}

/// Verdict produced by a [`VerificationStrategy`].
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationOutcome {
  /// The candidate answer is accepted; the loop should stop.
  Approved,
  /// The candidate answer is rejected. `feedback` is fed back into the
  /// loop as the next observation so the model can address it.
  Rejected {
    /// Critique explaining why the answer was rejected, phrased so it
    /// reads naturally as the next turn's observation.
    feedback: String,
  },
}

/// Errors a verification strategy can return.
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
  /// The strategy could not produce a verdict (e.g. an LLM-backed judge
  /// call failed). The runtime treats this as non-fatal: the candidate
  /// answer is accepted rather than getting the run stuck.
  #[error("Verification strategy failed: {message}")]
  Failed {
    /// Human-readable failure description.
    message: String,
  },
}

/// Pluggable verification gate for agent runtimes.
///
/// A verification strategy can be plugged into a runtime (e.g. the ReAct
/// agent via `ReActAgent::with_verification_strategy`) to gate whether a
/// candidate final answer actually ends the run. Unlike
/// [`crate::reflection::ReflectionStrategy`], a verification's verdict
/// changes control flow: [`VerificationOutcome::Rejected`] sends the loop
/// back around for another attempt instead of terminating.
///
/// Strategies should:
///
/// - Avoid expensive blocking work; verification runs inline with the
///   loop and gates every candidate answer.
/// - Treat their own failures as non-fatal (return
///   `Err(VerificationError)` only when genuinely actionable; the runtime
///   accepts the candidate answer rather than deadlocking the run).
#[async_trait]
pub trait VerificationStrategy: Send + Sync {
  /// Stable, machine-readable strategy name (e.g. `"always-approve"`).
  fn name(&self) -> &'static str;

  /// Produce a verdict for `context`.
  async fn verify(
    &self,
    context: &VerificationContext,
  ) -> Result<VerificationOutcome, VerificationError>;
}

/// Verification strategy that always approves.
///
/// Useful in tests and examples, and as a base to wrap with real
/// domain-specific logic (an LLM-judge call, a test-runner invocation,
/// a schema check, ...).
#[derive(Debug, Default, Clone)]
pub struct AlwaysApprove;

#[async_trait]
impl VerificationStrategy for AlwaysApprove {
  fn name(&self) -> &'static str {
    "always-approve"
  }

  async fn verify(
    &self,
    _context: &VerificationContext,
  ) -> Result<VerificationOutcome, VerificationError> {
    Ok(VerificationOutcome::Approved)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn always_approve_approves_every_attempt() {
    let strategy = AlwaysApprove;
    let context = VerificationContext::for_answer("session-1", 3, "do the thing", "done", 1);

    let outcome = strategy.verify(&context).await.unwrap();

    assert_eq!(outcome, VerificationOutcome::Approved);
  }

  #[tokio::test]
  async fn rejecting_strategy_carries_feedback_into_outcome() {
    struct RejectOnce;

    #[async_trait]
    impl VerificationStrategy for RejectOnce {
      fn name(&self) -> &'static str {
        "reject-once"
      }

      async fn verify(
        &self,
        context: &VerificationContext,
      ) -> Result<VerificationOutcome, VerificationError> {
        if context.attempt == 1 {
          Ok(VerificationOutcome::Rejected {
            feedback: "missing citations".to_string(),
          })
        } else {
          Ok(VerificationOutcome::Approved)
        }
      }
    }

    let strategy = RejectOnce;
    let first = VerificationContext::for_answer("session-1", 3, "research X", "X is Y", 1);
    let second = VerificationContext::for_answer("session-1", 5, "research X", "X is Y [1]", 2);

    assert_eq!(
      strategy.verify(&first).await.unwrap(),
      VerificationOutcome::Rejected {
        feedback: "missing citations".to_string(),
      }
    );
    assert_eq!(
      strategy.verify(&second).await.unwrap(),
      VerificationOutcome::Approved
    );
  }

  #[tokio::test]
  async fn failing_strategy_returns_non_fatal_error() {
    struct AlwaysFails;

    #[async_trait]
    impl VerificationStrategy for AlwaysFails {
      fn name(&self) -> &'static str {
        "always-fails"
      }

      async fn verify(
        &self,
        _context: &VerificationContext,
      ) -> Result<VerificationOutcome, VerificationError> {
        Err(VerificationError::Failed {
          message: "judge unavailable".to_string(),
        })
      }
    }

    let strategy = AlwaysFails;
    let context = VerificationContext::for_answer("session-1", 3, "do the thing", "done", 1);

    let err = strategy.verify(&context).await.unwrap_err();

    assert!(matches!(err, VerificationError::Failed { .. }));
  }
}
