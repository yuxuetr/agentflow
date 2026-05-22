//! Harness runtime — Phase H1 MVP.
//!
//! [`HarnessRuntime`] is a thin wrapper that composes existing
//! `agentflow_agents::AgentRuntime` implementations with Harness-level
//! plumbing: context provider collection, persona assembly, event
//! envelope translation, and JSONL/in-memory persistence. The crate
//! does not own LLM, tool, or memory primitives; the caller hands the
//! runtime a built `AgentRuntime` (typically a `ReActAgent`) and the
//! runtime keeps the wrapper boundary intentionally narrow.

use std::path::PathBuf;
use std::sync::Arc;

use agentflow_agents::runtime::{
  AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentStep, AgentStepKind,
  AgentStopReason, RuntimeLimits,
};
use chrono::Utc;

use crate::context::{
  ContextItem, ContextPriority, ContextProvider, HarnessContext, HarnessProfile, HarnessRuntimeKind,
};
use crate::error::HarnessError;
use crate::event::{
  HarnessEvent, HarnessEventBody, SessionStartedPayload, StepStartedPayload, StopReason,
  StoppedPayload, ToolCallCompletedPayload, ToolCallRequestedPayload,
};
use crate::persistence::{HarnessEventSink, SinkChain};

/// Inputs handed to a single [`HarnessRuntime::run`] invocation.
///
/// Built via [`HarnessRunOptions::new`] plus the `with_*` setters. The
/// `session_id` defaults to a fresh UUID; provide one to resume an
/// existing session (Phase H1 wires the value through but does not yet
/// re-attach prior memory — see `P-H.1` follow-ups).
#[derive(Debug, Clone)]
pub struct HarnessRunOptions {
  pub user_input: String,
  pub workspace_root: PathBuf,
  pub model: String,
  pub runtime: HarnessRuntimeKind,
  pub profile: HarnessProfile,
  pub session_id: Option<String>,
  pub limits: RuntimeLimits,
  pub skill_name: Option<String>,
  pub persona_prefix: Option<String>,
  /// Soft cap on the total number of tokens contributed by context
  /// providers. `None` keeps every collected item. Higher-priority
  /// items are admitted first when the budget is tight.
  pub context_token_budget: Option<usize>,
  /// Free-form structured metadata attached to the underlying
  /// `AgentContext` and emitted on `session_started`.
  pub metadata: serde_json::Value,
}

impl HarnessRunOptions {
  pub fn new(
    user_input: impl Into<String>,
    workspace_root: impl Into<PathBuf>,
    model: impl Into<String>,
  ) -> Self {
    Self {
      user_input: user_input.into(),
      workspace_root: workspace_root.into(),
      model: model.into(),
      runtime: HarnessRuntimeKind::React,
      profile: HarnessProfile::Local,
      session_id: None,
      limits: RuntimeLimits::default(),
      skill_name: None,
      persona_prefix: None,
      context_token_budget: None,
      metadata: serde_json::Value::Null,
    }
  }

  pub fn with_runtime_kind(mut self, runtime: HarnessRuntimeKind) -> Self {
    self.runtime = runtime;
    self
  }

  pub fn with_profile(mut self, profile: HarnessProfile) -> Self {
    self.profile = profile;
    self
  }

  pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
    self.session_id = Some(session_id.into());
    self
  }

  pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = limits;
    self
  }

  pub fn with_skill_name(mut self, skill: impl Into<String>) -> Self {
    self.skill_name = Some(skill.into());
    self
  }

  pub fn with_persona_prefix(mut self, persona: impl Into<String>) -> Self {
    self.persona_prefix = Some(persona.into());
    self
  }

  pub fn with_context_token_budget(mut self, budget: usize) -> Self {
    self.context_token_budget = Some(budget);
    self
  }

  pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
    self.metadata = metadata;
    self
  }
}

/// Outcome returned by [`HarnessRuntime::run`].
#[derive(Debug, Clone)]
pub struct HarnessRunResult {
  /// Resolved session id (matches the inner agent's session id).
  pub session_id: String,
  /// Final answer when the agent produced one.
  pub answer: Option<String>,
  /// Inner agent stop reason.
  pub stop_reason: AgentStopReason,
  /// Final monotonic event sequence number written through the sink
  /// chain. Equals the last emitted event's `seq`.
  pub final_event_seq: u64,
  /// Number of context items admitted into the assembled persona.
  pub context_items_admitted: usize,
  /// Number of context items dropped under the token budget.
  pub context_items_dropped: usize,
  /// Original inner agent run result. Kept for trace replay / debug.
  pub inner: AgentRunResult,
}

impl HarnessRunResult {
  /// Convenience for callers that need to render `"<answer>\n\nSession: <id>"`.
  pub fn answer_with_session(&self) -> String {
    let answer = self.answer.as_deref().unwrap_or("(no answer)");
    format!("{answer}\n\nSession: {}", self.session_id)
  }
}

/// Harness runtime façade composed at session-construction time.
///
/// Phase H1 keeps the surface narrow: one inner [`AgentRuntime`], a
/// list of context providers, and a sink chain. Hooks, approval
/// providers, and parallel tool calls arrive in Phase H2+ without
/// breaking the existing `run` signature.
pub struct HarnessRuntime {
  inner: Box<dyn AgentRuntime>,
  context_providers: Vec<Arc<dyn ContextProvider>>,
  sinks: SinkChain,
  initial_seq: u64,
}

impl HarnessRuntime {
  /// Build a runtime from a pre-constructed inner [`AgentRuntime`].
  /// Most callers pass `Box::new(ReActAgent::new(...))` or use
  /// `agentflow_skills::SkillBuilder` to assemble one.
  pub fn new(inner: Box<dyn AgentRuntime>) -> Self {
    Self {
      inner,
      context_providers: Vec::new(),
      sinks: SinkChain::new(),
      initial_seq: 0,
    }
  }

  /// Append a single context provider. Ordering is preserved; the
  /// runtime sorts by [`ContextPriority`] before admitting items.
  pub fn with_context_provider(mut self, provider: Arc<dyn ContextProvider>) -> Self {
    self.context_providers.push(provider);
    self
  }

  /// Append every provider returned by an iterator.
  pub fn with_context_providers<I>(mut self, providers: I) -> Self
  where
    I: IntoIterator<Item = Arc<dyn ContextProvider>>,
  {
    self.context_providers.extend(providers);
    self
  }

  /// Register an event sink. Sinks are dispatched in registration order.
  pub fn with_event_sink(mut self, sink: Arc<dyn HarnessEventSink>) -> Self {
    self.sinks = std::mem::take(&mut self.sinks).push(sink);
    self
  }

  /// Set the seq number used for the first event emitted by the next
  /// `run()`. Defaults to `0`, matching the original behaviour.
  ///
  /// **Why:** the Harness `:resume` route on the server has two flavours.
  /// The default *rerun* semantic clears prior persisted events and
  /// starts a fresh `seq=0` series, which is fine when the operator
  /// wants a clean retry. *Append-mode* resume keeps the prior log and
  /// continues the seq series — for that to work without colliding
  /// with persisted `(session_id, seq)` rows, the runtime needs to
  /// start emitting at `MAX(seq) + 1`, not `0`. This builder is the
  /// single seam that lets the server thread that offset in.
  ///
  /// **How to apply:** pass the next-unused seq for the session
  /// (`MAX(existing seq) + 1`). The first emitted `session_started`
  /// event will use this value; subsequent events increment from
  /// there, so `final_event_seq` in the result still equals the last
  /// emitted event's `seq`.
  pub fn with_initial_seq(mut self, initial_seq: u64) -> Self {
    self.initial_seq = initial_seq;
    self
  }

  /// Read-only view of the configured context providers (helpful for
  /// diagnostics and tests).
  pub fn context_provider_names(&self) -> Vec<&str> {
    self.context_providers.iter().map(|p| p.name()).collect()
  }

  /// Run one Harness session and return the structured outcome.
  ///
  /// Steps:
  /// 1. Resolve or generate a `session_id`.
  /// 2. Build [`HarnessContext`] for the providers.
  /// 3. Collect, sort, and trim context items under the configured
  ///    token budget.
  /// 4. Emit `session_started` and persist it.
  /// 5. Build an [`AgentContext`] (persona = context block + caller
  ///    prefix) and run the inner agent.
  /// 6. Translate the inner agent's [`AgentEvent`]s and post-hoc
  ///    inferred per-step events to [`HarnessEvent`]s with a
  ///    monotonic `seq`.
  /// 7. Emit `stopped` with the terminal reason.
  pub async fn run(
    &mut self,
    options: HarnessRunOptions,
  ) -> Result<HarnessRunResult, HarnessError> {
    let session_id = options
      .session_id
      .clone()
      .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let ctx = HarnessContext {
      session_id: session_id.clone(),
      workspace_root: options.workspace_root.clone(),
      user_input: options.user_input.clone(),
      model: options.model.clone(),
      runtime: options.runtime,
      profile: options.profile,
      metadata: options.metadata.clone(),
    };

    let (items, dropped) = self
      .collect_context(&ctx, options.context_token_budget)
      .await?;
    let persona = assemble_persona(options.persona_prefix.as_deref(), &items);

    let mut seq = self.initial_seq;
    let context_token_estimate: usize = items.iter().map(|item| item.token_estimate).sum();
    let started_payload = SessionStartedPayload {
      workspace_root: ctx.workspace_root.to_string_lossy().into_owned(),
      runtime: ctx.runtime,
      profile: ctx.profile,
      model: ctx.model.clone(),
      skills: options
        .skill_name
        .as_ref()
        .map(|name| vec![name.clone()])
        .unwrap_or_default(),
      context_item_count: items.len(),
      context_token_estimate,
    };
    let started_event = HarnessEvent {
      seq,
      session_id: session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::SessionStarted(started_payload),
    };
    self.sinks.dispatch(&started_event).await?;

    let mut agent_context = AgentContext::new(&session_id, &options.user_input, &options.model)
      .with_limits(options.limits.clone());
    if let Some(persona) = persona.clone() {
      agent_context = agent_context.with_persona(persona);
    }
    if let Some(skill) = options.skill_name.as_deref() {
      agent_context = agent_context.with_skill_name(skill);
    }
    if let serde_json::Value::Null = options.metadata {
      // Leave the default empty object set by AgentContext::new.
    } else {
      agent_context.metadata = options.metadata.clone();
    }

    let inner_result = self
      .inner
      .run(agent_context)
      .await
      .map_err(|err| HarnessError::Other(format!("inner agent failed: {err}")))?;

    let translated = translate_inner_events(&inner_result, &session_id, &mut seq);
    for event in &translated {
      self.sinks.dispatch(event).await?;
    }

    let stop_reason_clone = inner_result.stop_reason.clone();
    let answer_clone = inner_result.answer.clone();
    let stopped_payload = stopped_payload_from(&stop_reason_clone, answer_clone.as_deref());
    seq += 1;
    let stopped_event = HarnessEvent {
      seq,
      session_id: session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::Stopped(stopped_payload),
    };
    self.sinks.dispatch(&stopped_event).await?;
    self.sinks.flush_all().await?;

    Ok(HarnessRunResult {
      session_id,
      answer: inner_result.answer.clone(),
      stop_reason: stop_reason_clone,
      final_event_seq: seq,
      context_items_admitted: items.len(),
      context_items_dropped: dropped,
      inner: inner_result,
    })
  }

  async fn collect_context(
    &self,
    ctx: &HarnessContext,
    budget: Option<usize>,
  ) -> Result<(Vec<ContextItem>, usize), HarnessError> {
    let mut items = Vec::new();
    for provider in &self.context_providers {
      let name = provider.name().to_string();
      let part = provider.collect(ctx).await.map_err(|err| match err {
        HarnessError::ContextProviderFailed { .. } => err,
        other => HarnessError::context(name, other.to_string()),
      })?;
      items.extend(part);
    }
    // Stable sort by ascending priority — Critical (0) wins over Low (3).
    items.sort_by_key(|item| item.priority as u8);
    let dropped = match budget {
      None => 0,
      Some(budget) => trim_to_budget(&mut items, budget),
    };
    Ok((items, dropped))
  }
}

fn trim_to_budget(items: &mut Vec<ContextItem>, budget: usize) -> usize {
  let mut used = 0usize;
  let mut admitted = Vec::with_capacity(items.len());
  let mut dropped = 0usize;
  for item in items.drain(..) {
    let estimate = item.token_estimate;
    if used.saturating_add(estimate) <= budget {
      used += estimate;
      admitted.push(item);
    } else {
      dropped += 1;
    }
  }
  *items = admitted;
  dropped
}

fn assemble_persona(prefix: Option<&str>, items: &[ContextItem]) -> Option<String> {
  if prefix.is_none() && items.is_empty() {
    return None;
  }
  let mut buf = String::new();
  if let Some(prefix) = prefix {
    buf.push_str(prefix.trim_end());
    buf.push_str("\n\n");
  }
  if !items.is_empty() {
    buf.push_str("## Project Context\n\n");
    buf.push_str(
      "The following workspace context was assembled by Harness context providers. \
        Use it to ground your answers; cite the source name when you rely on it.\n\n",
    );
    for item in items {
      buf.push_str("### ");
      buf.push_str(&item.source);
      buf.push_str(" (priority=");
      buf.push_str(priority_str(item.priority));
      buf.push_str(")\n\n");
      buf.push_str(item.content.trim_end());
      buf.push_str("\n\n");
    }
  }
  Some(buf.trim_end().to_string())
}

fn priority_str(priority: ContextPriority) -> &'static str {
  match priority {
    ContextPriority::Critical => "critical",
    ContextPriority::High => "high",
    ContextPriority::Normal => "normal",
    ContextPriority::Low => "low",
  }
}

fn translate_inner_events(
  result: &AgentRunResult,
  session_id: &str,
  seq: &mut u64,
) -> Vec<HarnessEvent> {
  let mut out = Vec::new();
  // Step boundaries from the recorded steps give us deterministic
  // ordering for `step_started` events that doesn't depend on the
  // inner runtime's choice of when to fire `AgentEvent::StepStarted`.
  for step in &result.steps {
    *seq += 1;
    out.push(HarnessEvent {
      seq: *seq,
      session_id: session_id.to_owned(),
      ts: step.timestamp,
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: step.index,
        step_type: step_kind_name(&step.kind).to_owned(),
      }),
    });
    if let Some(payload) = tool_call_requested_from_step(step) {
      *seq += 1;
      out.push(HarnessEvent {
        seq: *seq,
        session_id: session_id.to_owned(),
        ts: step.timestamp,
        body: HarnessEventBody::ToolCallRequested(payload),
      });
    }
  }
  for event in &result.events {
    if let AgentEvent::ToolCallCompleted {
      step_index,
      tool,
      is_error,
      duration_ms,
      source,
      timestamp,
      ..
    } = event
    {
      *seq += 1;
      out.push(HarnessEvent {
        seq: *seq,
        session_id: session_id.to_owned(),
        ts: *timestamp,
        body: HarnessEventBody::ToolCallCompleted(ToolCallCompletedPayload {
          step_index: *step_index,
          tool: tool.clone(),
          is_error: *is_error,
          duration_ms: *duration_ms,
          source: source.clone(),
          output_summary: None,
        }),
      });
    }
  }
  out
}

fn tool_call_requested_from_step(step: &AgentStep) -> Option<ToolCallRequestedPayload> {
  match &step.kind {
    AgentStepKind::ToolCall { tool, params } => Some(ToolCallRequestedPayload {
      step_index: step.index,
      tool: tool.clone(),
      source: None,
      permissions: Vec::new(),
      idempotency: None,
      params_summary: params.clone(),
    }),
    _ => None,
  }
}

fn step_kind_name(kind: &AgentStepKind) -> &'static str {
  match kind {
    AgentStepKind::Observe { .. } => "observe",
    AgentStepKind::Plan { .. } => "plan",
    AgentStepKind::ToolCall { .. } => "tool_call",
    AgentStepKind::ToolResult { .. } => "tool_result",
    AgentStepKind::Reflect { .. } => "reflect",
    AgentStepKind::FinalAnswer { .. } => "final_answer",
    AgentStepKind::Handoff { .. } => "handoff",
    AgentStepKind::BlackboardOp { .. } => "blackboard_op",
    AgentStepKind::DebateProposal { .. } => "debate_proposal",
    AgentStepKind::DebateVerdict { .. } => "debate_verdict",
  }
}

fn stopped_payload_from(reason: &AgentStopReason, answer: Option<&str>) -> StoppedPayload {
  match reason {
    AgentStopReason::FinalAnswer => StoppedPayload {
      reason: StopReason::Completed,
      final_answer: answer.map(ToOwned::to_owned),
      error: None,
    },
    AgentStopReason::StopCondition { condition } => StoppedPayload {
      reason: StopReason::Completed,
      final_answer: answer.map(ToOwned::to_owned),
      error: Some(format!("stop_condition: {condition}")),
    },
    AgentStopReason::MaxSteps { max_steps } => StoppedPayload {
      reason: StopReason::LimitReached,
      final_answer: None,
      error: Some(format!("max_steps={max_steps}")),
    },
    AgentStopReason::MaxToolCalls { max_tool_calls } => StoppedPayload {
      reason: StopReason::LimitReached,
      final_answer: None,
      error: Some(format!("max_tool_calls={max_tool_calls}")),
    },
    AgentStopReason::Timeout { timeout_ms } => StoppedPayload {
      reason: StopReason::LimitReached,
      final_answer: None,
      error: Some(format!("timeout_ms={timeout_ms}")),
    },
    AgentStopReason::Cancelled { message } => StoppedPayload {
      reason: StopReason::Cancelled,
      final_answer: None,
      error: Some(message.clone()),
    },
    AgentStopReason::TokenBudgetExceeded { used, budget } => StoppedPayload {
      reason: StopReason::LimitReached,
      final_answer: None,
      error: Some(format!("token_budget exceeded: {used}/{budget}")),
    },
    AgentStopReason::CostLimitExceeded {
      used_usd,
      budget_usd,
    } => StoppedPayload {
      reason: StopReason::LimitReached,
      final_answer: None,
      error: Some(format!(
        "cost_limit_usd exceeded: ${used_usd:.4}/${budget_usd:.4}"
      )),
    },
    AgentStopReason::Error { message } => StoppedPayload {
      reason: StopReason::Failed,
      final_answer: None,
      error: Some(message.clone()),
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::context::HarnessRuntimeKind;
  use agentflow_agents::runtime::{
    AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentRuntimeError, AgentStep,
    AgentStepKind, AgentStopReason,
  };
  use async_trait::async_trait;
  use chrono::Utc;
  use serde_json::json;

  struct ScriptedRuntime {
    answer: String,
    stop_reason: AgentStopReason,
    extra_steps: Vec<AgentStep>,
    extra_events: Vec<AgentEvent>,
    captured_persona: Arc<tokio::sync::Mutex<Option<String>>>,
  }

  #[async_trait]
  impl AgentRuntime for ScriptedRuntime {
    async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
      *self.captured_persona.lock().await = context.persona.clone();
      let mut steps = vec![AgentStep::new(
        0,
        AgentStepKind::Observe {
          input: context.input.clone(),
        },
      )];
      steps.extend(self.extra_steps.iter().cloned());
      steps.push(AgentStep::new(
        steps.len(),
        AgentStepKind::FinalAnswer {
          answer: self.answer.clone(),
        },
      ));
      Ok(AgentRunResult {
        session_id: context.session_id,
        answer: Some(self.answer.clone()),
        stop_reason: self.stop_reason.clone(),
        steps,
        events: self.extra_events.clone(),
      })
    }

    fn runtime_name(&self) -> &'static str {
      "scripted"
    }
  }

  fn make_runtime(
    answer: &str,
    captured: Arc<tokio::sync::Mutex<Option<String>>>,
  ) -> ScriptedRuntime {
    ScriptedRuntime {
      answer: answer.to_owned(),
      stop_reason: AgentStopReason::FinalAnswer,
      extra_steps: Vec::new(),
      extra_events: Vec::new(),
      captured_persona: captured,
    }
  }

  #[tokio::test]
  async fn runtime_emits_session_started_and_stopped_bookends() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("hi there", captured.clone()));
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime =
      HarnessRuntime::new(inner).with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();

    let result = runtime
      .run(
        HarnessRunOptions::new("hello", dir.path(), "mock")
          .with_runtime_kind(HarnessRuntimeKind::React),
      )
      .await
      .unwrap();
    assert_eq!(result.answer.as_deref(), Some("hi there"));
    assert!(result.session_id.len() >= 32);
    let events = sink.snapshot().await;
    assert!(matches!(
      events.first().unwrap().body,
      HarnessEventBody::SessionStarted(_)
    ));
    assert!(matches!(
      events.last().unwrap().body,
      HarnessEventBody::Stopped(_)
    ));
    let seqs: Vec<u64> = events.iter().map(|event| event.seq).collect();
    let mut sorted = seqs.clone();
    sorted.sort();
    assert_eq!(seqs, sorted, "seqs must be monotonic increasing");
  }

  #[tokio::test]
  async fn runtime_pipes_context_into_inner_persona() {
    use crate::providers::AgentsMdProvider;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::write(dir.path().join("AGENTS.md"), "rule: keep commits small\n")
      .await
      .unwrap();
    let mut runtime =
      HarnessRuntime::new(inner).with_context_provider(Arc::new(AgentsMdProvider::new()));

    let result = runtime
      .run(HarnessRunOptions::new(
        "scope my change",
        dir.path(),
        "mock",
      ))
      .await
      .unwrap();
    assert_eq!(result.context_items_admitted, 1);
    let persona = captured.lock().await.clone().unwrap();
    assert!(persona.contains("Project Context"));
    assert!(persona.contains("rule: keep commits small"));
  }

  #[tokio::test]
  async fn runtime_emits_step_started_and_tool_call_events_from_inner_steps() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let mut inner = make_runtime("done", captured.clone());
    inner.extra_steps.push(AgentStep::new(
      1,
      AgentStepKind::ToolCall {
        tool: "echo".into(),
        params: json!({"text": "hi"}),
      },
    ));
    inner.extra_events.push(AgentEvent::ToolCallCompleted {
      session_id: "ignored".into(),
      step_index: 1,
      tool: "echo".into(),
      is_error: false,
      duration_ms: 7,
      source: Some("builtin".into()),
      permissions: Vec::new(),
      timestamp: Utc::now(),
    });

    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    let result = runtime
      .run(HarnessRunOptions::new("call tools", dir.path(), "mock"))
      .await
      .unwrap();
    let events = sink.snapshot().await;
    let kinds: Vec<&str> = events
      .iter()
      .map(|event| match &event.body {
        HarnessEventBody::SessionStarted(_) => "session_started",
        HarnessEventBody::StepStarted(_) => "step_started",
        HarnessEventBody::ToolCallRequested(_) => "tool_call_requested",
        HarnessEventBody::ToolCallCompleted(_) => "tool_call_completed",
        HarnessEventBody::Stopped(_) => "stopped",
        _ => "other",
      })
      .collect();
    assert_eq!(kinds.first(), Some(&"session_started"));
    assert!(kinds.contains(&"step_started"));
    assert!(kinds.contains(&"tool_call_requested"));
    assert!(kinds.contains(&"tool_call_completed"));
    assert_eq!(kinds.last(), Some(&"stopped"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
  }

  #[tokio::test]
  async fn runtime_trims_context_under_budget() {
    use crate::context::ContextItem;
    use async_trait::async_trait;

    struct FixedProvider {
      name: &'static str,
      priority: ContextPriority,
      tokens: usize,
    }

    #[async_trait]
    impl ContextProvider for FixedProvider {
      fn name(&self) -> &str {
        self.name
      }

      async fn collect(&self, _ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
        Ok(vec![ContextItem {
          source: self.name.to_owned(),
          priority: self.priority,
          token_estimate: self.tokens,
          content: format!("{}-body", self.name),
          metadata: serde_json::Value::Null,
        }])
      }
    }

    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = HarnessRuntime::new(inner)
      .with_context_provider(Arc::new(FixedProvider {
        name: "high",
        priority: ContextPriority::Critical,
        tokens: 100,
      }))
      .with_context_provider(Arc::new(FixedProvider {
        name: "low",
        priority: ContextPriority::Low,
        tokens: 100,
      }));
    let result = runtime
      .run(HarnessRunOptions::new("test", dir.path(), "mock").with_context_token_budget(120))
      .await
      .unwrap();
    assert_eq!(result.context_items_admitted, 1);
    assert_eq!(result.context_items_dropped, 1);
    let persona = captured.lock().await.clone().unwrap();
    assert!(persona.contains("high-body"));
    assert!(!persona.contains("low-body"));
  }

  #[tokio::test]
  async fn runtime_maps_stop_reasons_to_envelope_reasons() {
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let mut inner = make_runtime("ignored", captured.clone());
    inner.stop_reason = AgentStopReason::MaxSteps { max_steps: 4 };
    use crate::persistence::InMemoryEventSink;
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    let _ = runtime
      .run(HarnessRunOptions::new("test", dir.path(), "mock"))
      .await
      .unwrap();
    let events = sink.snapshot().await;
    let stopped = events
      .iter()
      .find_map(|event| match &event.body {
        HarnessEventBody::Stopped(payload) => Some(payload),
        _ => None,
      })
      .expect("stopped event present");
    assert_eq!(stopped.reason, StopReason::LimitReached);
    assert!(stopped.error.as_deref().unwrap().contains("max_steps=4"));
  }

  #[tokio::test]
  async fn runtime_with_initial_seq_offsets_first_event() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(inner)
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>)
      .with_initial_seq(42);
    let dir = tempfile::tempdir().unwrap();

    let result = runtime
      .run(HarnessRunOptions::new("hi", dir.path(), "mock"))
      .await
      .unwrap();
    let events = sink.snapshot().await;
    assert_eq!(
      events.first().unwrap().seq,
      42,
      "first emitted event must use initial_seq"
    );
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    let mut sorted = seqs.clone();
    sorted.sort();
    assert_eq!(seqs, sorted, "seqs remain monotonic when offset");
    let final_seq = events.iter().map(|e| e.seq).max().unwrap();
    assert_eq!(final_seq, result.final_event_seq);
    assert!(
      final_seq >= 42,
      "final seq must be at or above the initial offset"
    );
  }

  #[tokio::test]
  async fn runtime_default_initial_seq_starts_at_zero() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime =
      HarnessRuntime::new(inner).with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    runtime
      .run(HarnessRunOptions::new("hi", dir.path(), "mock"))
      .await
      .unwrap();
    let events = sink.snapshot().await;
    assert_eq!(events.first().unwrap().seq, 0);
  }

  #[tokio::test]
  async fn runtime_persists_session_via_jsonl_sink() {
    use crate::persistence::JsonlEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("answered", captured.clone()));
    let tmp = tempfile::tempdir().unwrap();
    let sink = Arc::new(JsonlEventSink::new(tmp.path().join("sessions")));
    let mut runtime =
      HarnessRuntime::new(inner).with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    let result = runtime
      .run(HarnessRunOptions::new("hi", dir.path(), "mock").with_session_id("sess-fixed"))
      .await
      .unwrap();
    assert_eq!(result.session_id, "sess-fixed");
    let events = sink.read_session("sess-fixed").await.unwrap();
    assert!(!events.is_empty());
    assert_eq!(events.first().unwrap().seq, 0);
    let final_seq = events.iter().map(|e| e.seq).max().unwrap();
    assert_eq!(final_seq, result.final_event_seq);
  }
}
