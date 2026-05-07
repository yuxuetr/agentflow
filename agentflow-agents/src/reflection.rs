use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Why a reflection strategy was invoked.
///
/// Implementations of [`ReflectionStrategy`] should usually filter on
/// `trigger` first and return `Ok(None)` for triggers they do not handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionTrigger {
  /// Per-step trigger: emitted after every action / observation pair when
  /// the runtime opts in.
  Step,
  /// A tool call or model invocation failed.
  Failure,
  /// The agent produced its final answer and is about to terminate.
  Final,
}

/// Input available to reflection strategies.
///
/// Strategies receive enough context to produce a short, free-form
/// reflection without needing access to the full memory store. Use
/// `metadata` for runtime-specific extensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionContext {
  /// Session id for the run that produced this reflection point.
  pub session_id: String,
  /// Step index at which the reflection was requested.
  pub step_index: usize,
  /// The reason the reflection was invoked.
  pub trigger: ReflectionTrigger,
  /// Most recent agent thought, when available.
  pub thought: Option<String>,
  /// Most recent observation / tool result content, when available.
  pub observation: Option<String>,
  /// Final answer text when `trigger == Final`.
  pub answer: Option<String>,
  /// Error description when `trigger == Failure`.
  pub error: Option<String>,
  /// Free-form structured metadata supplied by the runtime.
  #[serde(default)]
  pub metadata: Value,
}

impl ReflectionContext {
  /// Build a context for a [`ReflectionTrigger::Failure`] reflection.
  pub fn failure(
    session_id: impl Into<String>,
    step_index: usize,
    error: impl Into<String>,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      step_index,
      trigger: ReflectionTrigger::Failure,
      thought: None,
      observation: None,
      answer: None,
      error: Some(error.into()),
      metadata: Value::Object(Default::default()),
    }
  }

  /// Build a context for a [`ReflectionTrigger::Final`] reflection.
  pub fn final_answer(
    session_id: impl Into<String>,
    step_index: usize,
    answer: impl Into<String>,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      step_index,
      trigger: ReflectionTrigger::Final,
      thought: None,
      observation: None,
      answer: Some(answer.into()),
      error: None,
      metadata: Value::Object(Default::default()),
    }
  }
}

/// Reflection text produced by a strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reflection {
  /// Stable strategy name (matches [`ReflectionStrategy::name`]).
  pub strategy: String,
  /// Trigger this reflection was produced for.
  pub trigger: ReflectionTrigger,
  /// Reflection text emitted by the strategy.
  pub content: String,
  /// Wall-clock time the reflection was produced.
  pub timestamp: DateTime<Utc>,
}

impl Reflection {
  /// Build a reflection record with a fresh `timestamp`.
  pub fn new(
    strategy: impl Into<String>,
    trigger: ReflectionTrigger,
    content: impl Into<String>,
  ) -> Self {
    Self {
      strategy: strategy.into(),
      trigger,
      content: content.into(),
      timestamp: Utc::now(),
    }
  }
}

/// Errors a reflection strategy can return.
#[derive(Debug, thiserror::Error)]
pub enum ReflectionError {
  /// The strategy could not produce a reflection (e.g. an LLM-backed
  /// summariser failed). The runtime treats this as non-fatal: the run
  /// continues without a reflection step.
  #[error("Reflection strategy failed: {message}")]
  Failed {
    /// Human-readable failure description.
    message: String,
  },
}

/// Pluggable reflection boundary for agent runtimes.
///
/// A reflection strategy can be plugged into a runtime (e.g. the ReAct
/// agent via `ReActAgent::with_reflection_strategy`) to inject short
/// post-hoc summaries into the step trace. Strategies should:
///
/// - Filter on [`ReflectionContext::trigger`] and return `Ok(None)` for
///   triggers they do not handle.
/// - Avoid expensive blocking work; reflections run inline with the loop.
/// - Treat their own failures as non-fatal (return `Err(ReflectionError)`
///   only when the situation is genuinely actionable; the runtime will
///   continue without inserting a reflection step).
#[async_trait]
pub trait ReflectionStrategy: Send + Sync {
  /// Stable, machine-readable strategy name (e.g. `"failure"`, `"final"`).
  fn name(&self) -> &'static str;

  /// Produce an optional [`Reflection`] for `context`. Return `Ok(None)`
  /// to skip emitting a reflection for this trigger.
  async fn reflect(
    &self,
    context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError>;
}

/// Reflection strategy that intentionally emits nothing.
#[derive(Debug, Default, Clone)]
pub struct NoOpReflection;

#[async_trait]
impl ReflectionStrategy for NoOpReflection {
  fn name(&self) -> &'static str {
    "noop"
  }

  async fn reflect(
    &self,
    _context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError> {
    Ok(None)
  }
}

/// Records concise notes when a step or run fails.
#[derive(Debug, Default, Clone)]
pub struct FailureReflection;

#[async_trait]
impl ReflectionStrategy for FailureReflection {
  fn name(&self) -> &'static str {
    "failure"
  }

  async fn reflect(
    &self,
    context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError> {
    if context.trigger != ReflectionTrigger::Failure {
      return Ok(None);
    }

    let content = context
      .error
      .as_deref()
      .map(|error| format!("Failure at step {}: {}", context.step_index, error))
      .unwrap_or_else(|| format!("Failure at step {}.", context.step_index));

    Ok(Some(Reflection::new(
      self.name(),
      ReflectionTrigger::Failure,
      content,
    )))
  }
}

/// Records a concise final answer summary.
#[derive(Debug, Default, Clone)]
pub struct FinalReflection;

#[async_trait]
impl ReflectionStrategy for FinalReflection {
  fn name(&self) -> &'static str {
    "final"
  }

  async fn reflect(
    &self,
    context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError> {
    if context.trigger != ReflectionTrigger::Final {
      return Ok(None);
    }

    let content = context
      .answer
      .as_deref()
      .map(|answer| {
        format!(
          "Final answer produced at step {}: {}",
          context.step_index, answer
        )
      })
      .unwrap_or_else(|| format!("Final answer produced at step {}.", context.step_index));

    Ok(Some(Reflection::new(
      self.name(),
      ReflectionTrigger::Final,
      content,
    )))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn noop_reflection_emits_nothing() {
    let strategy = NoOpReflection;
    let context = ReflectionContext::failure("session-1", 2, "tool failed");

    let reflection = strategy.reflect(&context).await.unwrap();

    assert!(reflection.is_none());
  }

  #[tokio::test]
  async fn failure_reflection_only_handles_failures() {
    let strategy = FailureReflection;
    let failure = ReflectionContext::failure("session-1", 2, "tool failed");
    let final_answer = ReflectionContext::final_answer("session-1", 3, "done");

    let reflection = strategy.reflect(&failure).await.unwrap().unwrap();
    assert_eq!(reflection.strategy, "failure");
    assert!(reflection.content.contains("tool failed"));
    assert!(strategy.reflect(&final_answer).await.unwrap().is_none());
  }

  #[tokio::test]
  async fn final_reflection_only_handles_final_answers() {
    let strategy = FinalReflection;
    let failure = ReflectionContext::failure("session-1", 2, "tool failed");
    let final_answer = ReflectionContext::final_answer("session-1", 3, "done");

    assert!(strategy.reflect(&failure).await.unwrap().is_none());
    let reflection = strategy.reflect(&final_answer).await.unwrap().unwrap();
    assert_eq!(reflection.strategy, "final");
    assert!(reflection.content.contains("done"));
  }
}
