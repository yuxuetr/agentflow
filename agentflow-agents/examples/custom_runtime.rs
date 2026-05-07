//! Smallest viable `AgentRuntime` implementation.
//!
//! This example shows how to implement the `AgentRuntime` trait from scratch
//! without using `ReActAgent`, `PlanExecuteAgent`, or any LLM. The runtime
//! is fully deterministic and dependency-free, so it doubles as a reference
//! shell for authors who want to plug a custom planning algorithm or DSL
//! interpreter into the AgentFlow agent ecosystem.
//!
//! What the runtime demonstrates:
//!
//! * Honouring `RuntimeLimits` (here: `max_steps`).
//! * Honouring the cancellation token from `AgentContext`.
//! * Emitting structured `AgentStep`s in chronological order so trace
//!   replay / event listeners / multi-agent supervisors keep working.
//! * Returning a structured `AgentRunResult` with a meaningful
//!   `AgentStopReason`.
//!
//! Run:
//!
//! ```sh
//! cargo run -p agentflow-agents --example custom_runtime
//! ```

use agentflow_agents::{
  AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentRuntimeError, AgentStep,
  AgentStepKind, AgentStopReason,
};
use async_trait::async_trait;
use chrono::Utc;

/// A trivial echo runtime: it observes the input, "plans" a one-line
/// response, then emits a `FinalAnswer`. No model and no tools.
struct EchoRuntime;

#[async_trait]
impl AgentRuntime for EchoRuntime {
  fn runtime_name(&self) -> &'static str {
    "echo"
  }

  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
    if context.input.trim().is_empty() {
      return Err(AgentRuntimeError::InvalidContext {
        message: "input must be non-empty".to_string(),
      });
    }

    let mut steps: Vec<AgentStep> = Vec::new();
    let mut events: Vec<AgentEvent> = Vec::new();

    events.push(AgentEvent::RunStarted {
      session_id: context.session_id.clone(),
      model: context.model.clone(),
      timestamp: Utc::now(),
    });

    // Step 0: observe the input verbatim.
    push_step(
      &mut steps,
      &mut events,
      &context,
      AgentStepKind::Observe {
        input: context.input.clone(),
      },
    );
    if let Some(reason) = should_stop(&context, &steps) {
      return finish(context, steps, events, reason);
    }

    // Step 1: produce a one-line plan.
    push_step(
      &mut steps,
      &mut events,
      &context,
      AgentStepKind::Plan {
        thought: format!("echo back the {} chars of input", context.input.len()),
      },
    );
    if let Some(reason) = should_stop(&context, &steps) {
      return finish(context, steps, events, reason);
    }

    // Step 2: terminal answer.
    let answer = format!("echo: {}", context.input);
    push_step(
      &mut steps,
      &mut events,
      &context,
      AgentStepKind::FinalAnswer {
        answer: answer.clone(),
      },
    );

    let reason = AgentStopReason::FinalAnswer;
    let mut result = finish(context, steps, events, reason)?;
    result.answer = Some(answer);
    Ok(result)
  }
}

fn push_step(
  steps: &mut Vec<AgentStep>,
  events: &mut Vec<AgentEvent>,
  context: &AgentContext,
  kind: AgentStepKind,
) {
  let step = AgentStep::new(steps.len(), kind);
  events.push(AgentEvent::StepCompleted {
    session_id: context.session_id.clone(),
    step: step.clone(),
  });
  steps.push(step);
}

fn should_stop(context: &AgentContext, steps: &[AgentStep]) -> Option<AgentStopReason> {
  if let Some(token) = &context.cancellation_token
    && token.is_cancelled()
  {
    return Some(AgentStopReason::Cancelled {
      message: "cancellation token tripped".to_string(),
    });
  }
  if let Some(max_steps) = context.limits.max_steps
    && steps.len() >= max_steps
  {
    return Some(AgentStopReason::MaxSteps { max_steps });
  }
  None
}

fn finish(
  context: AgentContext,
  steps: Vec<AgentStep>,
  mut events: Vec<AgentEvent>,
  stop_reason: AgentStopReason,
) -> Result<AgentRunResult, AgentRuntimeError> {
  events.push(AgentEvent::RunStopped {
    session_id: context.session_id.clone(),
    reason: stop_reason.clone(),
    timestamp: Utc::now(),
  });
  let answer = steps.iter().rev().find_map(|step| match &step.kind {
    AgentStepKind::FinalAnswer { answer } => Some(answer.clone()),
    _ => None,
  });
  Ok(AgentRunResult {
    session_id: context.session_id,
    answer,
    stop_reason,
    steps,
    events,
  })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let mut runtime = EchoRuntime;

  // 1. Happy path — runs to FinalAnswer.
  let happy = AgentRuntime::run(
    &mut runtime,
    AgentContext::new("custom-runtime-happy", "hello world", "no-llm"),
  )
  .await?;
  println!("--- Happy path ---");
  println!("Stop reason: {:?}", happy.stop_reason);
  println!("Answer: {}", happy.answer.as_deref().unwrap_or("<none>"));
  println!("Steps: {}", happy.steps.len());

  // 2. Limits — `max_steps = 1` stops after the observe step.
  let limited_ctx = AgentContext::new("custom-runtime-limited", "hello", "no-llm").with_limits(
    agentflow_agents::RuntimeLimits {
      max_steps: Some(1),
      ..Default::default()
    },
  );
  let limited = AgentRuntime::run(&mut runtime, limited_ctx).await?;
  println!();
  println!("--- max_steps=1 ---");
  println!("Stop reason: {:?}", limited.stop_reason);
  println!("Answer: {:?}", limited.answer);
  println!("Steps: {}", limited.steps.len());

  // 3. Validation — empty input is a structured `InvalidContext` error.
  let bad = AgentRuntime::run(
    &mut runtime,
    AgentContext::new("custom-runtime-bad", "   ", "no-llm"),
  )
  .await
  .err();
  println!();
  println!("--- Empty input ---");
  println!("Error: {:?}", bad);

  Ok(())
}
