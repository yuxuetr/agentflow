//! Harness runtime — Phase H1 MVP.
//!
//! [`HarnessRuntime`] is a thin wrapper that composes existing
//! `agentflow_agent_spi::AgentRuntime` implementations with Harness-level
//! plumbing: context provider collection, persona assembly, event
//! envelope translation, and JSONL/in-memory persistence. The crate
//! does not own LLM, tool, or memory primitives; the caller hands the
//! runtime a built `AgentRuntime` (typically a `ReActAgent`) and the
//! runtime keeps the wrapper boundary intentionally narrow.

use std::path::PathBuf;
use std::sync::Arc;

use std::sync::atomic::{AtomicU64, Ordering};

use agentflow_agent_spi::runtime::{
  AgentContext, AgentEvent, AgentEventSink, AgentRunResult, AgentRuntime, AgentStep, AgentStepKind,
  AgentStopReason, RuntimeLimits,
};
use agentflow_agent_spi::{TurnDrivenRuntime, TurnProgress};
use async_trait::async_trait;
use chrono::Utc;

use crate::compaction::ContextSummarizer;
use crate::context::{
  ContextItem, ContextPriority, ContextProvider, HarnessContext, HarnessProfile, HarnessRuntimeKind,
};
use crate::error::HarnessError;
use crate::event::{
  HarnessEvent, HarnessEventBody, MemorySummaryAddedPayload, SessionStartedPayload,
  StepStartedPayload, StopReason, StoppedPayload, ToolCallCompletedPayload,
  ToolCallRequestedPayload,
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
  /// Q3.1.2: optional cancellation token forwarded to the inner
  /// `AgentContext` so the CLI Ctrl-C path (or any other supervisor)
  /// can stop the agent loop after the current step instead of
  /// dropping the future mid-`await`. Default `None` keeps the
  /// pre-Q3.1.2 behaviour for callers that build the agent context
  /// themselves.
  pub cancellation_token: Option<agentflow_agent_spi::runtime::AgentCancellationToken>,
  /// Phase 2b: optional between-turn hook forwarded to the inner agent's
  /// context. The inner agent (ReActAgent) invokes it before each turn's
  /// LLM call, so the caller can perform between-turn context engineering
  /// (e.g. memory compaction) mid-run. Stored as the `Debug`-able handle
  /// so `HarnessRunOptions` keeps deriving `Debug` / `Clone`.
  pub between_turn_hook: Option<agentflow_agent_spi::runtime::BetweenTurnHookHandle>,
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
      cancellation_token: None,
      between_turn_hook: None,
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

  /// Q3.1.2: attach a cancellation token that the inner agent loop
  /// honors. The CLI Ctrl-C handler creates a fresh token, hands one
  /// clone here, and keeps the other clone to call `cancel()` from
  /// the signal future.
  pub fn with_cancellation_token(
    mut self,
    token: agentflow_agent_spi::runtime::AgentCancellationToken,
  ) -> Self {
    self.cancellation_token = Some(token);
    self
  }

  /// Phase 2b: attach a between-turn hook forwarded to the inner agent so
  /// the caller performs context engineering (e.g. compaction) before
  /// each turn's LLM call.
  pub fn with_between_turn_hook(
    mut self,
    hook: Arc<dyn agentflow_agent_spi::runtime::BetweenTurnHook>,
  ) -> Self {
    self.between_turn_hook = Some(agentflow_agent_spi::runtime::BetweenTurnHookHandle(hook));
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
  /// Phase 0: number of context items that were truncated (rather than
  /// dropped) to fit the remaining token budget. An item is truncated
  /// when it overflows the budget but enough headroom remains to keep a
  /// useful prefix; it is dropped only when the remaining budget is
  /// below the per-item floor. See RFC_HARNESS_LOOP_OWNERSHIP §4.
  pub context_items_truncated: usize,
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
/// How the harness executes the inner agent.
///
/// `Opaque` runs the whole agent loop in one `AgentRuntime::run` call (the
/// agent owns iteration; the harness observes via the event bridge).
/// `TurnDriven` (RFC_HARNESS_LOOP_OWNERSHIP §6) lets the **harness own the
/// loop** — it pumps `next_turn` itself, the seam at which caller-owned
/// context engineering between turns will plug in.
enum InnerRuntime {
  Opaque(Box<dyn AgentRuntime>),
  TurnDriven(Box<dyn TurnDrivenRuntime>),
  /// No inner agent — the harness governs a deterministic `Flow` run instead
  /// (P-A2.2). `run()` rejects this kind; use `run_flow()` (see `flow_run`).
  None,
}

pub struct HarnessRuntime {
  inner: InnerRuntime,
  context_providers: Vec<Arc<dyn ContextProvider>>,
  /// Phase 2 (RFC_HARNESS_LOOP_OWNERSHIP): optional context compactor.
  /// When set and the assembled context overflows the token budget, the
  /// items that would be dropped are summarized into a single synthetic
  /// `context_compaction` item (so continuity is preserved) and a
  /// `MemorySummaryAdded` event is emitted. `None` keeps the Phase 0
  /// drop/truncate behaviour.
  compactor: Option<Arc<dyn ContextSummarizer>>,
  // pub(crate) so the P-A2.2 `flow_run` module can dispatch the
  // session_started / stopped envelope through the same sink chain.
  pub(crate) sinks: SinkChain,
  /// Q1.7.1: the runtime and the hook layer (via `HookConfig`) used to
  /// have independent `seq` counters. Mixed runtime + hook events
  /// could collide on the same `(session_id, seq)` PK, breaking the
  /// "monotonic, never gap" promise of the Beta-frozen `HarnessEvent`
  /// envelope. Now both paths share this single `Arc<AtomicU64>`.
  /// Surface it to callers via [`Self::seq_counter`]. `pub(crate)` so the
  /// P-A2.2 `flow_run` module shares the same monotonic series.
  pub(crate) seq_counter: Arc<std::sync::atomic::AtomicU64>,
  /// §6: when `true` and the runtime is turn-driven, re-run the context
  /// providers before each turn and inject refreshed workspace context
  /// into the session when it changed. Off by default (re-running
  /// providers has IO cost); opt in with [`Self::with_context_refresh`].
  refresh_context: bool,
}

impl HarnessRuntime {
  /// Build a runtime from a pre-constructed inner [`AgentRuntime`].
  /// Most callers pass `Box::new(ReActAgent::new(...))` or use
  /// `agentflow_skills::SkillBuilder` to assemble one.
  pub fn new(inner: Box<dyn AgentRuntime>) -> Self {
    Self::with_inner(InnerRuntime::Opaque(inner))
  }

  /// Build a runtime that **drives the inner agent turn-by-turn** (RFC §6).
  /// The harness pumps [`TurnDrivenRuntime`] one turn at a time, owning the
  /// loop. Live events still stream through the event bridge exactly as in
  /// the opaque path; the difference is the harness controls iteration and
  /// gets a between-turn seam for its own context engineering.
  pub fn new_turn_driven(inner: Box<dyn TurnDrivenRuntime>) -> Self {
    Self::with_inner(InnerRuntime::TurnDriven(inner))
  }

  /// Build a runtime with **no inner agent**, to govern a deterministic
  /// `Flow` run via [`Self::run_flow`] (P-A2.2). Calling [`Self::run`] on a
  /// runtime built this way returns an error — there is no agent loop to drive.
  pub fn for_flow() -> Self {
    Self::with_inner(InnerRuntime::None)
  }

  fn with_inner(inner: InnerRuntime) -> Self {
    Self {
      inner,
      context_providers: Vec::new(),
      compactor: None,
      sinks: SinkChain::new(),
      seq_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
      refresh_context: false,
    }
  }

  /// §6: in turn-driven mode, re-run the context providers before each
  /// turn and inject refreshed workspace context (as a clearly-prefixed
  /// user message, since the agent skips `Role::System` history) when it
  /// changed — so a long-running agent perceives workspace edits mid-run.
  /// Emits a `memory_summary_added` event (`layer = "context_refresh"`).
  /// No-op in opaque mode (the harness can't get between turns there).
  pub fn with_context_refresh(mut self) -> Self {
    self.refresh_context = true;
    self
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

  /// Phase 2: attach a [`ContextSummarizer`] so over-budget context is
  /// compacted into a single summary item (and a `MemorySummaryAdded`
  /// event) instead of being dropped. Pass
  /// [`crate::DeterministicContextSummarizer`] for an LLM-free default.
  pub fn with_context_summarizer(mut self, compactor: Arc<dyn ContextSummarizer>) -> Self {
    self.compactor = Some(compactor);
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
  pub fn with_initial_seq(self, initial_seq: u64) -> Self {
    self
      .seq_counter
      .store(initial_seq, std::sync::atomic::Ordering::SeqCst);
    self
  }

  /// Q1.7.1: shared seq counter handle. Pass this `Arc<AtomicU64>` to
  /// [`HookConfig::with_seq_counter`](crate::HookConfig::with_seq_counter)
  /// so the hook-layer `tool_call_requested` / `approval_*` events use
  /// the same monotonic series as the runtime's `session_started` /
  /// `step_started` / `stopped` events. Clones are cheap (Arc bump).
  pub fn seq_counter(&self) -> Arc<std::sync::atomic::AtomicU64> {
    self.seq_counter.clone()
  }

  /// Q1.7.1: inject a pre-existing seq counter so the runtime and the
  /// hook layer share the same atomic. Use this from `harness_live`
  /// where the registry (and therefore the `HookConfig`) is built
  /// before the agent + runtime that owns it — both ends accept the
  /// same `Arc` to keep the series monotonic.
  pub fn with_seq_counter(mut self, counter: Arc<std::sync::atomic::AtomicU64>) -> Self {
    self.seq_counter = counter;
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

    let CollectedContext {
      items,
      dropped,
      truncated,
      compaction,
    } = self
      .collect_context(&ctx, options.context_token_budget)
      .await?;
    let persona = assemble_persona(options.persona_prefix.as_deref(), &items);

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
    // Q1.7.1: take seq from the shared counter so any HookConfig wired
    // with `with_seq_counter(runtime.seq_counter())` shares the same
    // monotonic series.
    let started_seq = self
      .seq_counter
      .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let started_event = HarnessEvent {
      seq: started_seq,
      session_id: session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::SessionStarted(started_payload),
    };
    self.sinks.dispatch(&started_event).await?;

    // Phase 2: if context assembly compacted over-budget items, surface
    // it as a MemorySummaryAdded event right after session_started so the
    // operator sees the compaction the agent's persona was built on.
    if let Some(compaction) = &compaction {
      let seq = self
        .seq_counter
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      let event = HarnessEvent {
        seq,
        session_id: session_id.clone(),
        ts: Utc::now(),
        body: HarnessEventBody::MemorySummaryAdded(MemorySummaryAddedPayload {
          layer: compaction.layer.clone(),
          summary: compaction.summary.clone(),
          token_estimate: compaction.token_estimate,
        }),
      };
      self.sinks.dispatch(&event).await?;
    }

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
    // Q3.1.2: forward the optional cancellation token from options into
    // the inner agent's context so the CLI Ctrl-C handler can stop the
    // ReAct / plan-execute loop gracefully.
    if let Some(token) = options.cancellation_token.clone() {
      agent_context = agent_context.with_cancellation_token(token);
    }
    // Phase 2b: forward the optional between-turn hook so the inner agent
    // calls it before each turn for caller-owned context engineering.
    if let Some(hook) = options.between_turn_hook.clone() {
      agent_context.between_turn_hook = Some(hook);
    }

    // Phase 1 (RFC_HARNESS_LOOP_OWNERSHIP): attach the live event bridge
    // so a live-aware inner agent (ReActAgent) streams tool + memory
    // events through the shared sink chain + seq counter as they happen,
    // interleaved correctly with the hook layer's approval events.
    let bridge = Arc::new(HarnessAgentEventBridge::new(
      session_id.clone(),
      self.sinks.clone(),
      self.seq_counter.clone(),
    ));
    agent_context = agent_context.with_event_sink(bridge.clone());

    // §6: between-turn context refresh setup (turn-driven mode). Cloned
    // here because the driving loop borrows `self.inner` and so cannot
    // touch other `self` fields. The initial block seeds `last_context`
    // so the first refresh only fires if the workspace actually changed.
    let refresh_enabled = self.refresh_context;
    let refresh_providers = self.context_providers.clone();
    let refresh_sinks = self.sinks.clone();
    let refresh_seq = self.seq_counter.clone();
    let refresh_ctx = ctx.clone();
    let mut last_context = if refresh_enabled {
      assemble_persona(None, &items)
    } else {
      None
    };

    let inner_result = match &mut self.inner {
      // P-A2.2: a Flow-governance runtime has no agent loop to drive here.
      InnerRuntime::None => {
        return Err(HarnessError::Other(
          "HarnessRuntime::run called on a Flow-governance runtime; use run_flow()".to_string(),
        ));
      }
      // Agent owns iteration; the harness observes via the event bridge.
      InnerRuntime::Opaque(agent) => agent
        .run(agent_context)
        .await
        .map_err(|err| HarnessError::Other(format!("inner agent failed: {err}")))?,
      // RFC §6: the harness owns the loop — pump one turn at a time, and
      // (when enabled) refresh workspace context between turns.
      InnerRuntime::TurnDriven(td) => {
        let mut session = td
          .begin(agent_context)
          .await
          .map_err(|err| HarnessError::Other(format!("inner agent failed: {err}")))?;
        loop {
          if refresh_enabled {
            refresh_context_between_turns(
              &refresh_providers,
              &refresh_ctx,
              &mut last_context,
              session.memory(),
              &refresh_sinks,
              &refresh_seq,
            )
            .await?;
          }
          match session
            .next_turn()
            .await
            .map_err(|err| HarnessError::Other(format!("inner agent failed: {err}")))?
          {
            TurnProgress::Continued => {}
            TurnProgress::Finished(result) => break result,
          }
        }
      }
    };

    // If the bridge emitted anything, the inner runtime is live-aware and
    // already streamed the tool events; the post-hoc pass then emits only
    // the `step_started` markers. Otherwise (a runtime that ignores
    // `event_sink`) reconstruct the full set for backward compatibility.
    let live_emitted = bridge.emitted() > 0;
    let translated =
      translate_inner_events(&inner_result, &session_id, &self.seq_counter, live_emitted);
    for event in &translated {
      self.sinks.dispatch(event).await?;
    }

    let stop_reason_clone = inner_result.stop_reason.clone();
    let answer_clone = inner_result.answer.clone();
    let stopped_payload = stopped_payload_from(&stop_reason_clone, answer_clone.as_deref());
    let stopped_seq = self
      .seq_counter
      .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let stopped_event = HarnessEvent {
      seq: stopped_seq,
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
      final_event_seq: stopped_seq,
      context_items_admitted: items.len(),
      context_items_dropped: dropped,
      context_items_truncated: truncated,
      inner: inner_result,
    })
  }

  /// Collect, token-account, sort, and budget-trim context items.
  ///
  /// Returns `(admitted_items, dropped_count, truncated_count)`.
  ///
  /// Phase 0 (RFC_HARNESS_LOOP_OWNERSHIP §4): the provider-declared
  /// `token_estimate` is treated as a hint; budgeting re-counts each
  /// item's content with the model's real tokenizer
  /// (`agentflow_llm::tokenizer::counter_for_model`) so the emitted
  /// `context_token_estimate` and the trim decisions reflect actual
  /// token cost rather than a `chars / 4` approximation.
  async fn collect_context(
    &self,
    ctx: &HarnessContext,
    budget: Option<usize>,
  ) -> Result<CollectedContext, HarnessError> {
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

    // Authoritative token accounting overrides the provider hint.
    let counter = agentflow_llm::tokenizer::counter_for_model(&ctx.model);
    for item in &mut items {
      item.token_estimate = counter.count_tokens(&item.content) as usize;
    }

    let Some(budget) = budget else {
      return Ok(CollectedContext {
        items,
        dropped: 0,
        truncated: 0,
        compaction: None,
      });
    };

    // Phase 2: when a compactor is configured, reserve a slice of the
    // budget for a compaction summary so the summary itself does not
    // re-overflow the window.
    let reserve = if self.compactor.is_some() {
      (budget / 8).clamp(MIN_ITEM_TOKENS, 256)
    } else {
      0
    };
    let effective = budget.saturating_sub(reserve);
    let (dropped_items, truncated) = trim_to_budget(&mut items, effective, counter.as_ref());

    let mut compaction = None;
    if let Some(compactor) = &self.compactor
      && !dropped_items.is_empty()
      && let Some(summary) = compactor.summarize(&dropped_items, reserve).await
    {
      let token_estimate = counter.count_tokens(&summary) as usize;
      // Inject the summary as a Critical synthetic item so it leads the
      // assembled persona and survives any future trim.
      items.insert(
        0,
        ContextItem {
          source: "context_compaction".to_owned(),
          priority: ContextPriority::Critical,
          token_estimate,
          content: summary.clone(),
          metadata: serde_json::json!({ "compacted_items": dropped_items.len() }),
        },
      );
      compaction = Some(CompactionOutcome {
        layer: compactor.name().to_owned(),
        summary,
        token_estimate,
      });
    }

    Ok(CollectedContext {
      items,
      dropped: dropped_items.len(),
      truncated,
      compaction,
    })
  }
}

/// Result of [`HarnessRuntime::collect_context`].
struct CollectedContext {
  items: Vec<ContextItem>,
  dropped: usize,
  truncated: usize,
  compaction: Option<CompactionOutcome>,
}

/// A compaction performed during context assembly, surfaced to the run
/// as a `MemorySummaryAdded` event.
struct CompactionOutcome {
  layer: String,
  summary: String,
  token_estimate: usize,
}

/// Per-item floor (in tokens). When the remaining budget is below this,
/// an overflowing item is dropped rather than truncated, because a
/// shorter-than-floor prefix carries too little signal to be worth the
/// header overhead it adds to the prompt.
const MIN_ITEM_TOKENS: usize = 32;

/// Marker appended to a context item whose body was cut to fit the
/// budget. Kept short so it barely dents the remaining headroom.
const TRUNCATION_MARKER: &str = "\n\n[...truncated to fit context budget]";

/// Admit items in priority order under `budget`. An item that fits is
/// admitted whole; one that overflows is **truncated to fit** (down to
/// [`MIN_ITEM_TOKENS`]) rather than dropped outright — this fixes the
/// priority inversion where a large high-priority item was silently
/// dropped while a small low-priority item slipped in
/// (RFC_HARNESS_LOOP_OWNERSHIP §1.2). Returns `(dropped_items,
/// truncated_count)` — the dropped items are handed back so a configured
/// [`ContextSummarizer`] can compact them (Phase 2) rather than losing
/// them entirely.
fn trim_to_budget(
  items: &mut Vec<ContextItem>,
  budget: usize,
  counter: &dyn agentflow_llm::tokenizer::TokenCounter,
) -> (Vec<ContextItem>, usize) {
  let mut used = 0usize;
  let mut admitted = Vec::with_capacity(items.len());
  let mut dropped = Vec::new();
  let mut truncated = 0usize;
  for mut item in items.drain(..) {
    let remaining = budget.saturating_sub(used);
    if item.token_estimate <= remaining {
      used += item.token_estimate;
      admitted.push(item);
    } else if remaining >= MIN_ITEM_TOKENS {
      let (content, tokens) = truncate_to_token_budget(&item.content, counter, remaining);
      item.content = content;
      item.token_estimate = tokens;
      used += tokens;
      truncated += 1;
      admitted.push(item);
    } else {
      dropped.push(item);
    }
  }
  *items = admitted;
  (dropped, truncated)
}

/// Cut `content` so that the truncated body plus [`TRUNCATION_MARKER`]
/// counts at most `budget` tokens. Truncation is char-based (so UTF-8
/// boundaries stay valid) but calibrated against the real tokenizer:
/// it seeds a guess from this content's own chars-per-token ratio, then
/// shrinks until the candidate fits. The loop is bounded — `take_chars`
/// strictly decreases each iteration and terminates at zero.
fn truncate_to_token_budget(
  content: &str,
  counter: &dyn agentflow_llm::tokenizer::TokenCounter,
  budget: usize,
) -> (String, usize) {
  let total_chars = content.chars().count();
  let total_tokens = counter.count_tokens(content) as usize;
  // Seed: assume tokens scale ~linearly with chars for this content.
  let chars_per_token = (total_chars as f64 / total_tokens.max(1) as f64).max(1.0);
  let mut take_chars = ((budget as f64) * chars_per_token) as usize;
  take_chars = take_chars.min(total_chars);
  loop {
    let body: String = content.chars().take(take_chars).collect();
    let candidate = format!("{body}{TRUNCATION_MARKER}");
    let tokens = counter.count_tokens(&candidate) as usize;
    if tokens <= budget || take_chars == 0 {
      return (candidate, tokens);
    }
    // Shrink by ~10% (at least one char) and retry.
    take_chars = take_chars.saturating_sub((take_chars / 10).max(1));
  }
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

/// §6: re-run the context providers between turns and, when the
/// assembled workspace context changed since the previous turn, inject it
/// into the session memory as a clearly-prefixed **user** message (the
/// agent skips `Role::System` history, so a user message is how refreshed
/// context reaches the next prompt) and emit a `memory_summary_added`
/// event (`layer = "context_refresh"`). A provider error aborts the turn
/// (fail-loud); a sink error is swallowed (observability must not break
/// execution).
async fn refresh_context_between_turns(
  providers: &[Arc<dyn ContextProvider>],
  ctx: &HarnessContext,
  last_context: &mut Option<String>,
  memory: &dyn agentflow_memory::MemoryStore,
  sinks: &SinkChain,
  seq_counter: &AtomicU64,
) -> Result<(), HarnessError> {
  let mut items = Vec::new();
  for provider in providers {
    let name = provider.name().to_string();
    let part = provider.collect(ctx).await.map_err(|err| match err {
      HarnessError::ContextProviderFailed { .. } => err,
      other => HarnessError::context(name, other.to_string()),
    })?;
    items.extend(part);
  }
  items.sort_by_key(|item| item.priority as u8);
  let block = assemble_persona(None, &items);

  if *last_context == block {
    return Ok(());
  }
  if let Some(block_str) = block.as_deref() {
    let message = agentflow_memory::Message::user(
      &ctx.session_id,
      format!("[workspace context refresh]\n{block_str}"),
    );
    memory
      .add_message(message)
      .await
      .map_err(|err| HarnessError::Other(format!("context refresh memory write failed: {err}")))?;
    let token_estimate =
      agentflow_llm::tokenizer::count_tokens_for_model(&ctx.model, block_str) as usize;
    let seq = seq_counter.fetch_add(1, Ordering::SeqCst);
    let event = HarnessEvent {
      seq,
      session_id: ctx.session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::MemorySummaryAdded(MemorySummaryAddedPayload {
        layer: "context_refresh".to_owned(),
        summary: format!("workspace context refreshed ({} items)", items.len()),
        token_estimate,
      }),
    };
    let _ = sinks.dispatch(&event).await;
  }
  *last_context = block;
  Ok(())
}

/// Phase 1 (RFC_HARNESS_LOOP_OWNERSHIP §5): live bridge from an inner
/// agent's [`AgentEvent`] stream to the Harness [`HarnessEvent`]
/// envelope. Attached to the inner [`AgentContext`] via
/// [`AgentContext::with_event_sink`], it maps tool + memory-summary
/// events the instant the agent produces them and dispatches them
/// through the shared [`SinkChain`] on the shared `seq_counter` — so a
/// tool call's `tool_call_requested` / `tool_call_completed` interleave
/// correctly with the `approval_requested` / `approval_decided` events
/// the hook layer fires during that same tool execution, instead of
/// being reconstructed in a post-hoc batch (the pre-Phase-1 "split-brain
/// seq epoch").
///
/// Runtimes that ignore `event_sink` (anything other than a live-aware
/// `ReActAgent`) simply never call [`emit`](AgentEventSink::emit); the
/// bridge's [`emitted`](Self::emitted) stays `0` and
/// [`HarnessRuntime::run`] falls back to full post-hoc translation, so
/// behavior is unchanged for those runtimes.
struct HarnessAgentEventBridge {
  session_id: String,
  sinks: SinkChain,
  seq_counter: Arc<AtomicU64>,
  emitted: AtomicU64,
}

impl HarnessAgentEventBridge {
  fn new(session_id: String, sinks: SinkChain, seq_counter: Arc<AtomicU64>) -> Self {
    Self {
      session_id,
      sinks,
      seq_counter,
      emitted: AtomicU64::new(0),
    }
  }

  /// Number of events this bridge dispatched live. `> 0` means the inner
  /// runtime is live-aware and the post-hoc translation must skip the
  /// tool events to avoid duplicating them.
  fn emitted(&self) -> u64 {
    self.emitted.load(Ordering::SeqCst)
  }

  async fn dispatch_body(&self, body: HarnessEventBody, ts: chrono::DateTime<Utc>) {
    let seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
    let event = HarnessEvent {
      seq,
      session_id: self.session_id.clone(),
      ts,
      body,
    };
    // Observability must never break execution: a sink error is logged
    // by the sink itself; here we swallow it so the agent loop proceeds.
    let _ = self.sinks.dispatch(&event).await;
    self.emitted.fetch_add(1, Ordering::SeqCst);
  }
}

#[async_trait]
impl AgentEventSink for HarnessAgentEventBridge {
  async fn emit(&self, event: &AgentEvent) {
    match event {
      AgentEvent::ToolCallStarted {
        step_index,
        tool,
        params,
        source,
        permissions,
        timestamp,
        ..
      } => {
        let params_summary = crate::params_summary::redact_and_cap(params.clone());
        self
          .dispatch_body(
            HarnessEventBody::ToolCallRequested(ToolCallRequestedPayload {
              step_index: *step_index,
              tool: tool.clone(),
              source: source.clone(),
              permissions: permissions.clone(),
              idempotency: None,
              params_summary,
            }),
            *timestamp,
          )
          .await;
      }
      AgentEvent::ToolCallCompleted {
        step_index,
        tool,
        is_error,
        duration_ms,
        source,
        timestamp,
        ..
      } => {
        self
          .dispatch_body(
            HarnessEventBody::ToolCallCompleted(ToolCallCompletedPayload {
              step_index: *step_index,
              tool: tool.clone(),
              is_error: *is_error,
              duration_ms: *duration_ms,
              source: source.clone(),
              output_summary: None,
            }),
            *timestamp,
          )
          .await;
      }
      AgentEvent::MemorySummaryAdded {
        layer,
        summary,
        token_estimate,
        timestamp,
        ..
      } => {
        self
          .dispatch_body(
            HarnessEventBody::MemorySummaryAdded(MemorySummaryAddedPayload {
              layer: layer.clone(),
              summary: summary.clone(),
              token_estimate: *token_estimate,
            }),
            *timestamp,
          )
          .await;
      }
      // Other events (RunStarted/Stopped, StepStarted, policy/capability
      // decisions, LLM-call accounting, multi-agent ops) are not part of
      // the Harness envelope's live tool/approval narrative. `step_started`
      // is emitted post-hoc from `result.steps` by `translate_inner_events`.
      _ => {}
    }
  }
}

fn translate_inner_events(
  result: &AgentRunResult,
  session_id: &str,
  seq_counter: &std::sync::atomic::AtomicU64,
  skip_tool_events: bool,
) -> Vec<HarnessEvent> {
  let mut out = Vec::new();
  // Q1.7.1: every event takes its seq from the shared
  // `Arc<AtomicU64>`. `fetch_add` is monotonic across the runtime +
  // hook-layer writers — no more `&mut u64` racing with the hook's
  // own counter.
  let next_seq = || seq_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
  // Step boundaries from the recorded steps give us deterministic
  // ordering for `step_started` events that doesn't depend on the
  // inner runtime's choice of when to fire `AgentEvent::StepStarted`.
  for step in &result.steps {
    out.push(HarnessEvent {
      seq: next_seq(),
      session_id: session_id.to_owned(),
      ts: step.timestamp,
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: step.index,
        step_type: step_kind_name(&step.kind).to_owned(),
      }),
    });
    // Phase 1: in live mode the bridge already emitted
    // `tool_call_requested` from `AgentEvent::ToolCallStarted`, so skip
    // the post-hoc reconstruction to avoid duplicates. `step_started`
    // always comes from here (the agent does not emit it live).
    if !skip_tool_events && let Some(payload) = tool_call_requested_from_step(step) {
      out.push(HarnessEvent {
        seq: next_seq(),
        session_id: session_id.to_owned(),
        ts: step.timestamp,
        body: HarnessEventBody::ToolCallRequested(payload),
      });
    }
  }
  if skip_tool_events {
    // `tool_call_completed` was emitted live by the bridge.
    return out;
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
      out.push(HarnessEvent {
        seq: next_seq(),
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
    AgentStepKind::ToolCall { tool, params } => {
      // Q1.7.2 + Phase 0: strip secrets AND cap the serialized size of
      // `params_summary` before the event flows into the JSONL / SSE /
      // stdout sinks (the wire type documents it as redacted/truncated).
      // The agent inner step still holds the raw params (it owns them);
      // the event envelope only sees the redacted, size-bounded form.
      let params_summary = crate::params_summary::redact_and_cap(params.clone());
      Some(ToolCallRequestedPayload {
        step_index: step.index,
        tool: tool.clone(),
        source: None,
        permissions: Vec::new(),
        idempotency: None,
        params_summary,
      })
    }
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
  use agentflow_agent_spi::runtime::{
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
    /// When non-empty, simulate a live-aware runtime (ReActAgent): emit
    /// each of these to `context.event_sink` as the run executes, in
    /// addition to returning them in `extra_events`.
    live_events: Vec<AgentEvent>,
  }

  #[async_trait]
  impl AgentRuntime for ScriptedRuntime {
    async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
      *self.captured_persona.lock().await = context.persona.clone();
      // Simulate live emission like a real ReActAgent would.
      if let Some(handle) = &context.event_sink {
        for ev in &self.live_events {
          handle.0.emit(ev).await;
        }
      }
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
      live_events: Vec::new(),
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

  /// Phase 1: a live-aware inner runtime (simulating ReActAgent) streams
  /// tool events through the bridge during the run; the post-hoc
  /// translation then skips them, so each tool call appears exactly once
  /// and on the shared monotonic seq clock — the split-brain epoch is
  /// gone. `step_started` is still emitted post-hoc from the recorded
  /// steps.
  #[tokio::test]
  async fn runtime_live_runtime_streams_tool_events_without_duplication() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let mut inner = make_runtime("done", captured.clone());
    inner.extra_steps.push(AgentStep::new(
      1,
      AgentStepKind::ToolCall {
        tool: "echo".into(),
        params: json!({ "text": "hi" }),
      },
    ));
    let ts = Utc::now();
    inner.live_events = vec![
      AgentEvent::ToolCallStarted {
        session_id: "ignored".into(),
        step_index: 1,
        tool: "echo".into(),
        params: json!({ "text": "hi" }),
        source: Some("builtin".into()),
        permissions: Vec::new(),
        timestamp: ts,
      },
      AgentEvent::ToolCallCompleted {
        session_id: "ignored".into(),
        step_index: 1,
        tool: "echo".into(),
        is_error: false,
        duration_ms: 3,
        source: Some("builtin".into()),
        permissions: Vec::new(),
        timestamp: ts,
      },
    ];

    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    runtime
      .run(HarnessRunOptions::new("go", dir.path(), "mock"))
      .await
      .unwrap();

    let events = sink.snapshot().await;
    let requested = events
      .iter()
      .filter(|e| matches!(e.body, HarnessEventBody::ToolCallRequested(_)))
      .count();
    let completed = events
      .iter()
      .filter(|e| matches!(e.body, HarnessEventBody::ToolCallCompleted(_)))
      .count();
    assert_eq!(
      requested, 1,
      "exactly one tool_call_requested (live; not duplicated by post-hoc translate)"
    );
    assert_eq!(completed, 1, "exactly one tool_call_completed");
    assert!(
      events
        .iter()
        .any(|e| matches!(e.body, HarnessEventBody::StepStarted(_))),
      "step_started still emitted post-hoc from recorded steps"
    );
    // The live tool events precede the post-hoc step_started markers, and
    // every seq is unique + monotonic across both emission paths.
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    let mut sorted = seqs.clone();
    sorted.sort();
    assert_eq!(seqs, sorted, "seqs monotonic across live + post-hoc paths");
    let mut deduped = seqs.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), seqs.len(), "no duplicate seqs");
  }

  use crate::context::ContextItem;

  /// Test provider that surfaces one item with caller-supplied content.
  /// `token_estimate` is left at 0 on purpose — the runtime's Phase 0
  /// recount with the real tokenizer is what budgeting must rely on.
  struct FixedProvider {
    name: &'static str,
    priority: ContextPriority,
    content: String,
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
        token_estimate: 0,
        content: self.content.clone(),
        metadata: serde_json::Value::Null,
      }])
    }
  }

  #[tokio::test]
  async fn runtime_trims_context_under_budget() {
    // Size each item so the real tokenizer ("mock" → heuristic) reports
    // a known count, then pick a budget that admits exactly one full
    // item and leaves less than the per-item floor for the second → the
    // lower-priority item is dropped.
    let counter = agentflow_llm::tokenizer::counter_for_model("mock");
    let body = "alpha beta gamma delta ".repeat(40);
    let item_tokens = counter.count_tokens(&body) as usize;
    assert!(
      item_tokens > MIN_ITEM_TOKENS * 2,
      "fixture must comfortably exceed the per-item floor"
    );
    let budget = item_tokens + MIN_ITEM_TOKENS / 2;

    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = HarnessRuntime::new(inner)
      .with_context_provider(Arc::new(FixedProvider {
        name: "high",
        priority: ContextPriority::Critical,
        content: body.clone(),
      }))
      .with_context_provider(Arc::new(FixedProvider {
        name: "low",
        priority: ContextPriority::Low,
        content: body.clone(),
      }));
    let result = runtime
      .run(HarnessRunOptions::new("test", dir.path(), "mock").with_context_token_budget(budget))
      .await
      .unwrap();
    assert_eq!(result.context_items_admitted, 1);
    assert_eq!(result.context_items_dropped, 1);
    assert_eq!(result.context_items_truncated, 0);
    let persona = captured.lock().await.clone().unwrap();
    assert!(persona.contains("### high"));
    assert!(!persona.contains("### low"));
  }

  /// Phase 0: an item that overflows the budget but leaves at least the
  /// per-item floor of headroom is truncated to fit, not dropped — so a
  /// large high-priority item still contributes its prefix instead of
  /// vanishing (RFC §1.2 priority-inversion fix).
  #[tokio::test]
  async fn runtime_truncates_oversized_item_instead_of_dropping() {
    let counter = agentflow_llm::tokenizer::counter_for_model("mock");
    let body = "alpha beta gamma delta ".repeat(60);
    let item_tokens = counter.count_tokens(&body) as usize;
    // Budget well below the item but well above the floor.
    let budget = (item_tokens / 2).max(MIN_ITEM_TOKENS * 2);
    assert!(budget < item_tokens && budget >= MIN_ITEM_TOKENS);

    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = HarnessRuntime::new(inner).with_context_provider(Arc::new(FixedProvider {
      name: "big",
      priority: ContextPriority::Critical,
      content: body,
    }));
    let result = runtime
      .run(HarnessRunOptions::new("test", dir.path(), "mock").with_context_token_budget(budget))
      .await
      .unwrap();
    assert_eq!(result.context_items_admitted, 1);
    assert_eq!(result.context_items_dropped, 0);
    assert_eq!(result.context_items_truncated, 1);
    let persona = captured.lock().await.clone().unwrap();
    assert!(
      persona.contains("truncated to fit context budget"),
      "truncated item must carry the marker"
    );
  }

  /// Phase 0: the provider-declared `token_estimate` is only a hint; the
  /// runtime recounts with the model's real tokenizer so a wildly wrong
  /// hint cannot distort budgeting.
  #[tokio::test]
  async fn runtime_recounts_tokens_ignoring_provider_hint() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let dir = tempfile::tempdir().unwrap();
    let sink = Arc::new(InMemoryEventSink::new());
    // Provider declares 0 tokens (FixedProvider hint) for a non-empty
    // body. The runtime must recount with the real tokenizer, so the
    // `context_token_estimate` reported on `session_started` is non-zero.
    let mut runtime = HarnessRuntime::new(inner)
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>)
      .with_context_provider(Arc::new(FixedProvider {
        name: "doc",
        priority: ContextPriority::Normal,
        content: "the quick brown fox jumps over the lazy dog".to_owned(),
      }));
    let result = runtime
      .run(HarnessRunOptions::new("test", dir.path(), "mock").with_context_token_budget(10_000))
      .await
      .unwrap();
    assert_eq!(result.context_items_admitted, 1);
    let events = sink.snapshot().await;
    let estimate = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::SessionStarted(p) => Some(p.context_token_estimate),
        _ => None,
      })
      .expect("session_started present");
    assert!(
      estimate > 0,
      "recount must replace the 0-token provider hint"
    );
  }

  /// Phase 2: with a compactor configured, an item that would be dropped
  /// under budget is instead summarized into a synthetic
  /// `context_compaction` item that leads the persona, and a
  /// `MemorySummaryAdded` event is emitted — the envelope the harness has
  /// advertised since H0 but never produced.
  #[tokio::test]
  async fn runtime_compacts_overbudget_context_and_emits_memory_summary() {
    use crate::compaction::DeterministicContextSummarizer;
    use crate::persistence::InMemoryEventSink;

    let counter = agentflow_llm::tokenizer::counter_for_model("mock");
    let body = "alpha beta gamma delta ".repeat(40);
    let item_tokens = counter.count_tokens(&body) as usize;
    // Budget admits the first (Critical) item whole and leaves less than
    // the floor for the second (Low) item → it would be dropped, but the
    // compactor summarizes it instead.
    let budget = item_tokens + 40;

    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let inner = Box::new(make_runtime("ok", captured.clone()));
    let sink = Arc::new(InMemoryEventSink::new());
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = HarnessRuntime::new(inner)
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>)
      .with_context_summarizer(Arc::new(DeterministicContextSummarizer))
      .with_context_provider(Arc::new(FixedProvider {
        name: "keep",
        priority: ContextPriority::Critical,
        content: body.clone(),
      }))
      .with_context_provider(Arc::new(FixedProvider {
        name: "spill",
        priority: ContextPriority::Low,
        content: body.clone(),
      }));
    let result = runtime
      .run(HarnessRunOptions::new("t", dir.path(), "mock").with_context_token_budget(budget))
      .await
      .unwrap();

    assert_eq!(
      result.context_items_dropped, 1,
      "the spilled item is compacted out"
    );
    let persona = captured.lock().await.clone().unwrap();
    assert!(
      persona.contains("context_compaction"),
      "compaction summary leads the persona"
    );
    assert!(persona.contains("Compacted 1 lower-priority context item"));
    let events = sink.snapshot().await;
    assert!(
      events
        .iter()
        .any(|e| matches!(e.body, HarnessEventBody::MemorySummaryAdded(_))),
      "compaction emits the long-advertised MemorySummaryAdded event"
    );
  }

  /// Phase 2b: a live `AgentEvent::MemorySummaryAdded` (the agent's
  /// mid-run, between-turn compaction) is mapped by the bridge to a
  /// harness `MemorySummaryAdded` envelope on the shared seq clock.
  #[tokio::test]
  async fn runtime_bridges_live_memory_summary_added() {
    use crate::persistence::InMemoryEventSink;
    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let mut inner = make_runtime("done", captured.clone());
    inner.live_events = vec![AgentEvent::MemorySummaryAdded {
      session_id: "ignored".into(),
      layer: "session".into(),
      summary: "compacted 3 older turns".into(),
      token_estimate: 12,
      timestamp: Utc::now(),
    }];
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    runtime
      .run(HarnessRunOptions::new("go", dir.path(), "mock"))
      .await
      .unwrap();
    let events = sink.snapshot().await;
    let summary = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::MemorySummaryAdded(p) => Some(p),
        _ => None,
      })
      .expect("bridge maps live MemorySummaryAdded to the harness envelope");
    assert_eq!(summary.layer, "session");
    assert_eq!(summary.token_estimate, 12);
    assert!(summary.summary.contains("compacted 3 older turns"));
  }

  /// RFC §6: `HarnessRuntime::new_turn_driven` makes the harness *own the
  /// loop* — it pumps `next_turn` (Continued × N then Finished) itself,
  /// producing the result and the session_started/stopped bookends.
  #[tokio::test]
  async fn harness_drives_turn_driven_runtime() {
    use crate::persistence::InMemoryEventSink;
    use agentflow_agent_spi::runtime::AgentRuntimeError;
    use agentflow_agent_spi::{LoopSession, TurnDrivenRuntime, TurnProgress};
    use agentflow_memory::{MemoryStore, SessionMemory};

    struct ScriptedSession {
      remaining: usize,
      answer: String,
      session_id: String,
      memory: SessionMemory,
    }
    #[async_trait]
    impl LoopSession for ScriptedSession {
      async fn next_turn(&mut self) -> Result<TurnProgress, AgentRuntimeError> {
        if self.remaining > 0 {
          self.remaining -= 1;
          Ok(TurnProgress::Continued)
        } else {
          Ok(TurnProgress::Finished(AgentRunResult {
            session_id: self.session_id.clone(),
            answer: Some(self.answer.clone()),
            stop_reason: AgentStopReason::FinalAnswer,
            steps: Vec::new(),
            events: Vec::new(),
          }))
        }
      }
      fn memory(&self) -> &dyn MemoryStore {
        &self.memory
      }
      fn turn_index(&self) -> usize {
        0
      }
    }

    struct ScriptedTd {
      turns: usize,
      answer: String,
    }
    #[async_trait]
    impl TurnDrivenRuntime for ScriptedTd {
      async fn begin(
        &mut self,
        ctx: AgentContext,
      ) -> Result<Box<dyn LoopSession + Send + '_>, AgentRuntimeError> {
        Ok(Box::new(ScriptedSession {
          remaining: self.turns,
          answer: self.answer.clone(),
          session_id: ctx.session_id.clone(),
          memory: SessionMemory::default_window(),
        }))
      }
      fn runtime_name(&self) -> &'static str {
        "scripted-td"
      }
    }

    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new_turn_driven(Box::new(ScriptedTd {
      turns: 2,
      answer: "driven answer".into(),
    }))
    .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();

    let result = runtime
      .run(HarnessRunOptions::new("go", dir.path(), "mock"))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("driven answer"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    let events = sink.snapshot().await;
    assert!(matches!(
      events.first().unwrap().body,
      HarnessEventBody::SessionStarted(_)
    ));
    assert!(matches!(
      events.last().unwrap().body,
      HarnessEventBody::Stopped(_)
    ));
  }

  /// §6: with `with_context_refresh`, a turn-driven run re-collects the
  /// context providers between turns and, when the workspace context
  /// changed, injects it into session memory and emits a
  /// `memory_summary_added` (`layer = "context_refresh"`) event.
  #[tokio::test]
  async fn turn_driven_context_refresh_injects_on_change() {
    use crate::persistence::InMemoryEventSink;
    use agentflow_agent_spi::runtime::AgentRuntimeError;
    use agentflow_agent_spi::{LoopSession, TurnDrivenRuntime, TurnProgress};
    use agentflow_memory::{MemoryStore, SessionMemory};
    use std::sync::atomic::AtomicUsize;

    // Provider whose content changes on every `collect`.
    struct ChangingProvider {
      n: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl ContextProvider for ChangingProvider {
      fn name(&self) -> &str {
        "changing"
      }
      async fn collect(&self, _ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
        let v = self.n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vec![ContextItem {
          source: "changing".to_owned(),
          priority: ContextPriority::Normal,
          token_estimate: 0,
          content: format!("workspace state v{v}"),
          metadata: serde_json::Value::Null,
        }])
      }
    }

    struct ScriptedSession {
      remaining: usize,
      session_id: String,
      memory: SessionMemory,
    }
    #[async_trait]
    impl LoopSession for ScriptedSession {
      async fn next_turn(&mut self) -> Result<TurnProgress, AgentRuntimeError> {
        if self.remaining > 0 {
          self.remaining -= 1;
          Ok(TurnProgress::Continued)
        } else {
          Ok(TurnProgress::Finished(AgentRunResult {
            session_id: self.session_id.clone(),
            answer: Some("ok".to_owned()),
            stop_reason: AgentStopReason::FinalAnswer,
            steps: Vec::new(),
            events: Vec::new(),
          }))
        }
      }
      fn memory(&self) -> &dyn MemoryStore {
        &self.memory
      }
      fn turn_index(&self) -> usize {
        0
      }
    }
    struct ScriptedTd {
      turns: usize,
    }
    #[async_trait]
    impl TurnDrivenRuntime for ScriptedTd {
      async fn begin(
        &mut self,
        ctx: AgentContext,
      ) -> Result<Box<dyn LoopSession + Send + '_>, AgentRuntimeError> {
        Ok(Box::new(ScriptedSession {
          remaining: self.turns,
          session_id: ctx.session_id.clone(),
          memory: SessionMemory::default_window(),
        }))
      }
      fn runtime_name(&self) -> &'static str {
        "scripted-td"
      }
    }

    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new_turn_driven(Box::new(ScriptedTd { turns: 2 }))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>)
      .with_context_provider(Arc::new(ChangingProvider {
        n: Arc::new(AtomicUsize::new(0)),
      }))
      .with_context_refresh();
    let dir = tempfile::tempdir().unwrap();
    runtime
      .run(HarnessRunOptions::new("go", dir.path(), "mock"))
      .await
      .unwrap();

    let events = sink.snapshot().await;
    let refreshes = events
      .iter()
      .filter(|e| {
        matches!(&e.body, HarnessEventBody::MemorySummaryAdded(p) if p.layer == "context_refresh")
      })
      .count();
    assert!(
      refreshes >= 2,
      "expected context_refresh events from the changing provider, got {refreshes}"
    );
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

  /// Q1.7.2 regression: `tool_call_requested_from_step` redacts
  /// secrets out of `params_summary` before they hit the event sink.
  /// Pre-fix the raw params (which may include API keys / passwords)
  /// were copied straight into the JSONL log.
  #[tokio::test]
  async fn tool_call_requested_redacts_sensitive_params_before_emit() {
    use crate::persistence::InMemoryEventSink;

    let captured = Arc::new(tokio::sync::Mutex::new(None));
    let mut inner = make_runtime("done", captured.clone());
    inner.extra_steps.push(AgentStep::new(
      1,
      AgentStepKind::ToolCall {
        tool: "http".into(),
        params: json!({
          "url": "https://api.example.com/login",
          "headers": {
            "Authorization": "Bearer sk-live-supersecret123",
          },
          "api_key": "ant-api03-do-not-log",
        }),
      },
    ));
    let sink = Arc::new(InMemoryEventSink::new());
    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);
    let dir = tempfile::tempdir().unwrap();
    runtime
      .run(HarnessRunOptions::new("login flow", dir.path(), "mock"))
      .await
      .unwrap();

    let events = sink.snapshot().await;
    let requested = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::ToolCallRequested(p) => Some(p),
        _ => None,
      })
      .expect("tool_call_requested must be emitted");

    let json_str = serde_json::to_string(&requested.params_summary).unwrap();
    assert!(
      !json_str.contains("sk-live-supersecret123"),
      "bearer token leaked into params_summary: {json_str}"
    );
    assert!(
      !json_str.contains("ant-api03-do-not-log"),
      "api_key leaked into params_summary: {json_str}"
    );
    // Non-secret fields still survive so the operator sees what was
    // attempted (URL, tool name, etc.).
    assert!(json_str.contains("api.example.com"));
  }

  /// Q1.7.1 regression: when the runtime and an external writer share
  /// the same `Arc<AtomicU64>` (the way `harness_live.rs` wires
  /// `HookConfig` against `HarnessRuntime`), every emitted seq is
  /// strictly monotonic across both writers — no duplicates, no gaps
  /// at the runtime/external boundary.
  #[tokio::test]
  async fn shared_seq_counter_keeps_runtime_and_hook_emissions_monotonic() {
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

    let shared_counter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let sink = Arc::new(InMemoryEventSink::new());

    // Simulate the hook layer claiming a seq number BEFORE the
    // runtime's first emission — this is what `HookConfig` does
    // when a tool call fires early.
    let hook_seq = shared_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let mut runtime = HarnessRuntime::new(Box::new(inner))
      .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>)
      .with_seq_counter(shared_counter.clone());

    let dir = tempfile::tempdir().unwrap();
    let result = runtime
      .run(HarnessRunOptions::new("call tools", dir.path(), "mock"))
      .await
      .unwrap();

    let runtime_events = sink.snapshot().await;
    let mut all_seqs: Vec<u64> = runtime_events.iter().map(|e| e.seq).collect();
    all_seqs.push(hook_seq);
    all_seqs.sort();

    // No duplicates anywhere.
    let mut deduped = all_seqs.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), all_seqs.len(), "no duplicate seqs allowed");

    // Hook seq (0) precedes every runtime seq.
    assert_eq!(all_seqs[0], 0);
    assert_eq!(
      runtime_events.first().unwrap().seq,
      1,
      "runtime first event must come right after the hook seq"
    );
    // final_event_seq matches the highest emitted seq.
    assert_eq!(
      result.final_event_seq,
      *all_seqs.last().unwrap(),
      "final_event_seq must equal the last emitted seq"
    );
  }
}
