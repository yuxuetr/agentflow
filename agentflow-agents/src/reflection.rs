use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Why a reflection strategy was invoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionTrigger {
  Step,
  Failure,
  Final,
}

/// Input available to reflection strategies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionContext {
  pub session_id: String,
  pub step_index: usize,
  pub trigger: ReflectionTrigger,
  pub thought: Option<String>,
  pub observation: Option<String>,
  pub answer: Option<String>,
  pub error: Option<String>,
  #[serde(default)]
  pub metadata: Value,
}

impl ReflectionContext {
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
  pub strategy: String,
  pub trigger: ReflectionTrigger,
  pub content: String,
  pub timestamp: DateTime<Utc>,
}

impl Reflection {
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

#[derive(Debug, thiserror::Error)]
pub enum ReflectionError {
  #[error("Reflection strategy failed: {message}")]
  Failed { message: String },
}

/// Pluggable reflection boundary for agent runtimes.
#[async_trait]
pub trait ReflectionStrategy: Send + Sync {
  fn name(&self) -> &'static str;

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
