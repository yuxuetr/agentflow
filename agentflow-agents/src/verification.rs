use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

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

#[derive(Deserialize)]
struct VerificationJudgeResponse {
  approved: bool,
  #[serde(default)]
  feedback: String,
}

/// LLM-as-judge [`VerificationStrategy`] (W3.4): one structured-output call
/// per [`verify`](VerificationStrategy::verify) invocation, judging whether
/// the candidate answer genuinely and completely addresses the user's
/// request. Mirrors [`crate::citation::LlmCitationChecker`] — the reference
/// pattern this module's own doc pointed future LLM-backed strategies at.
///
/// [`AlwaysApprove`] remains the default (deterministic, replay-safe, no
/// network call) — this is an opt-in strategy for callers who want a real
/// quality gate instead of a rubber stamp.
pub struct LlmVerification {
  model: String,
}

impl LlmVerification {
  pub fn new(model: impl Into<String>) -> Self {
    Self {
      model: model.into(),
    }
  }
}

#[async_trait]
impl VerificationStrategy for LlmVerification {
  fn name(&self) -> &'static str {
    "llm_judge"
  }

  async fn verify(
    &self,
    context: &VerificationContext,
  ) -> Result<VerificationOutcome, VerificationError> {
    let thought_block = context
      .thought
      .as_deref()
      .map(|thought| format!("\n\nReasoning that led to this answer:\n{thought}"))
      .unwrap_or_default();
    let prompt = format!(
      "User request:\n{}\n\nCandidate final answer (attempt {}):\n{}{}\n\n\
       Judge whether this answer genuinely and completely addresses the user's request. \
       Be strict: reject vague, incomplete, off-topic, or unsupported answers rather than \
       giving the benefit of the doubt.",
      context.user_input, context.attempt, context.answer, thought_block
    );
    let schema = json!({
      "type": "object",
      "properties": {
        "approved": { "type": "boolean" },
        "feedback": {
          "type": "string",
          "description": "Required when approved=false: a concrete critique the agent can act on to fix the answer. May be empty when approved=true."
        }
      },
      "required": ["approved", "feedback"]
    });

    let raw = AgentFlow::model(&self.model)
      .prompt(&prompt)
      .json_schema("verification_verdict", schema)
      .execute()
      .await
      .map_err(|e| VerificationError::Failed {
        message: e.to_string(),
      })?;
    let parsed: VerificationJudgeResponse =
      serde_json::from_str(&raw).map_err(|e| VerificationError::Failed {
        message: format!("failed to parse verification judge response: {e}"),
      })?;

    if parsed.approved {
      Ok(VerificationOutcome::Approved)
    } else {
      let feedback = if parsed.feedback.trim().is_empty() {
        "the judge rejected this answer without a specific reason".to_string()
      } else {
        parsed.feedback
      };
      Ok(VerificationOutcome::Rejected { feedback })
    }
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

  async fn init_mock_model(model: &str, response: &str) {
    // SAFETY: callers hold LLM_TEST_LOCK while mutating these process-wide vars.
    unsafe {
      std::env::set_var("AGENTFLOW_MOCK_RESPONSE", response);
      // AGENTFLOW_MOCK_RESPONSES (plural, a FIFO queue) takes priority over
      // AGENTFLOW_MOCK_RESPONSE in the mock provider — a prior test in this
      // process that set the queue and didn't clear it would otherwise
      // silently hijack this response. Clear it defensively.
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
    let config_path = std::env::temp_dir().join(format!(
      "agentflow-verification-mock-{}.yml",
      uuid::Uuid::new_v4()
    ));
    std::fs::write(
      &config_path,
      format!(
        "models:\n  {model}:\n    vendor: mock\n    type: text\n    model_id: {model}\n\
         providers:\n  mock:\n    api_key_env: MOCK_API_KEY\n"
      ),
    )
    .expect("write mock config");
    agentflow_llm::AgentFlow::init_with_config(config_path.to_str().expect("utf8 path"))
      .await
      .expect("init mock model");
  }

  /// W3.4 regression: `LlmVerification` must actually issue a structured-
  /// output call and map `approved: true` to `Approved`.
  #[tokio::test]
  async fn llm_verification_approves_when_judge_says_approved() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-verify-approve-{}", uuid::Uuid::new_v4());
    init_mock_model(&model, r#"{"approved":true,"feedback":""}"#).await;

    let strategy = LlmVerification::new(&model);
    let context = VerificationContext::for_answer("session-1", 3, "what is 2+2?", "4", 1);
    let outcome = strategy.verify(&context).await.unwrap();

    assert_eq!(outcome, VerificationOutcome::Approved);
  }

  /// W3.4 regression: `approved: false` must map to `Rejected` and carry
  /// the judge's feedback through unchanged.
  #[tokio::test]
  async fn llm_verification_rejects_with_feedback_when_judge_says_rejected() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-verify-reject-{}", uuid::Uuid::new_v4());
    init_mock_model(
      &model,
      r#"{"approved":false,"feedback":"the answer never addresses the request"}"#,
    )
    .await;

    let strategy = LlmVerification::new(&model);
    let context = VerificationContext::for_answer("session-1", 3, "what is 2+2?", "banana", 1);
    let outcome = strategy.verify(&context).await.unwrap();

    assert_eq!(
      outcome,
      VerificationOutcome::Rejected {
        feedback: "the answer never addresses the request".to_string(),
      }
    );
  }

  /// A rejection with no feedback text must still carry an actionable
  /// message rather than an empty string the next turn can't act on.
  #[tokio::test]
  async fn llm_verification_rejected_with_empty_feedback_gets_a_fallback_message() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-verify-reject-empty-{}", uuid::Uuid::new_v4());
    init_mock_model(&model, r#"{"approved":false,"feedback":""}"#).await;

    let strategy = LlmVerification::new(&model);
    let context = VerificationContext::for_answer("session-1", 3, "what is 2+2?", "banana", 1);
    let outcome = strategy.verify(&context).await.unwrap();

    match outcome {
      VerificationOutcome::Rejected { feedback } => assert!(!feedback.trim().is_empty()),
      other => panic!("expected Rejected, got {other:?}"),
    }
  }
}
