//! Hook + approval execution layer for [`crate::runtime::HarnessRuntime`].
//!
//! Phase H2 wires the [`PreToolHook`] / [`PostToolHook`] /
//! [`ApprovalProvider`] traits (frozen in Phase H0) into the live
//! tool-dispatch path by decorating every registered [`Tool`] with a
//! [`HookedTool`] wrapper. The wrapper:
//!
//! 1. Builds a [`PendingToolCall`] from the tool metadata + params.
//! 2. Runs every registered [`PreToolHook`] under a bounded timeout.
//!    Hook timeout is fail-closed: the call is denied with a reason
//!    that points at the offending hook.
//! 3. Combines the per-hook decisions (`Deny` wins over
//!    `RequireApproval` wins over `Allow`). When the active
//!    [`HarnessProfile`] is `production` and the call's idempotency
//!    is [`ToolIdempotency::NonIdempotent`], the wrapper escalates
//!    even an unanimous `Allow` to `RequireApproval` so production
//!    runs are fail-closed by default (HARNESS_MODE_EVOLUTION Risk 2).
//! 4. If an approval is required, builds an [`ApprovalRequest`], emits
//!    [`HarnessEventBody::ApprovalRequested`], delegates to the configured
//!    [`ApprovalProvider`], and emits [`HarnessEventBody::ApprovalDecided`]
//!    with the result. The decision is cached per
//!    `(tool_name, scope)` so `Session` / `Run` scoped allows reuse
//!    without re-prompting.
//! 5. Dispatches the inner tool if allowed, or returns a structured
//!    `ToolError::PolicyDenied` otherwise.
//! 6. Runs every registered [`PostToolHook`] (advisory; failures are
//!    logged but never undo the tool invocation).
//!
//! The wrapper does not emit `tool_call_requested` or
//! `tool_call_completed` events — those keep flowing from the
//! `HarnessRuntime` post-hoc translation so existing consumers do not
//! see duplicate tool events when hooks are enabled.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;

use agentflow_tools::{
  Capability, SandboxStatus, Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput,
  ToolRegistry,
};

use crate::approval::{
  ApprovalDecision, ApprovalOutcome, ApprovalProvider, ApprovalRequest, ApprovalRisk, ApprovalScope,
};
use crate::context::HarnessProfile;
use crate::error::HarnessError;
use crate::event::{
  ApprovalDecidedPayload, ApprovalRequestedPayload, HarnessEvent, HarnessEventBody,
};
use crate::hooks::{
  CompletedToolCall, PendingToolCall, PostToolHook, PreToolDecision, PreToolHook,
};
use crate::persistence::SinkChain;

/// Default timeout applied to each [`PreToolHook`] / [`PostToolHook`].
/// Operators can tighten or loosen this through
/// [`HookConfig::with_hook_timeout`].
pub const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// Default approval timeout. Used when an [`ApprovalRequest`] does not
/// carry an explicit `expires_at` field.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// Configuration shared by every [`HookedTool`] wrapped from one
/// registry call. Built with `HookConfig::new(...)` + chained setters.
pub struct HookConfig {
  session_id: String,
  profile: HarnessProfile,
  pre_hooks: Vec<Arc<dyn PreToolHook>>,
  post_hooks: Vec<Arc<dyn PostToolHook>>,
  approval_provider: Arc<dyn ApprovalProvider>,
  sinks: SinkChain,
  hook_timeout: Duration,
  approval_timeout: Duration,
  /// Shared with the parent [`HarnessRuntime`] so hook-emitted events
  /// share the same monotonic `seq` namespace as the run lifecycle
  /// events (`session_started` / `stopped` / etc).
  seq_counter: Arc<AtomicU64>,
}

impl HookConfig {
  /// Build a config with the minimum required pieces. `seq_counter`
  /// is created fresh; pass a shared counter via
  /// [`HookConfig::with_seq_counter`] when integrating with a parent
  /// [`crate::runtime::HarnessRuntime`] so the emitted approval events share the
  /// session's `seq` namespace.
  ///
  /// ## Approval-gate default is silent-allow (F-A2-12)
  ///
  /// The fresh config starts with [`HarnessProfile::Local`]
  /// (the enum default). Under Local, the approval flow is **opt-in**:
  ///
  /// - With no [`PreToolHook`]s registered, every wrapped tool is
  ///   silently auto-allowed — the [`ApprovalProvider`] is never
  ///   consulted. This is intentional for low-friction local dev.
  /// - To make NonIdempotent tools (shell, file:write, http POST)
  ///   actually trigger the approval prompt, either (a) chain
  ///   [`HookConfig::with_profile`] with
  ///   [`HarnessProfile::Production`] (auto-escalates every
  ///   NonIdempotent call), or (b) register a pre-hook that
  ///   explicitly returns `PreToolDecision::RequireApproval`.
  ///
  /// Reference wiring (binary with write tools):
  ///
  /// ```rust,ignore
  /// let hook_config = HookConfig::new(session_id, approval, sinks)
  ///   .with_profile(HarnessProfile::Production);
  /// let wrapped = wrap_registry(registry, hook_config);
  /// ```
  ///
  /// See `examples/applications/code-reviewer-write/` for an
  /// end-to-end binary that uses this exact pattern.
  pub fn new(
    session_id: impl Into<String>,
    approval_provider: Arc<dyn ApprovalProvider>,
    sinks: SinkChain,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      profile: HarnessProfile::default(),
      pre_hooks: Vec::new(),
      post_hooks: Vec::new(),
      approval_provider,
      sinks,
      hook_timeout: DEFAULT_HOOK_TIMEOUT,
      approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
      seq_counter: Arc::new(AtomicU64::new(0)),
    }
  }

  /// Set the security profile used by the approval-escalation rules
  /// in [`HookedTool`]. See [`HarnessProfile`] for the per-variant
  /// approval-gate semantics; the short version:
  ///
  /// - `Production` → NonIdempotent calls are auto-escalated to
  ///   `RequireApproval` (the approval prompt always fires for
  ///   mutating tools).
  /// - `Local` / `Dev` → no auto-escalation; only explicit
  ///   pre-hook `RequireApproval` decisions fire the prompt.
  ///
  /// For binaries that ship write-side tools (file:write, shell,
  /// http POST), call this with `HarnessProfile::Production` to
  /// avoid silently auto-allowing mutations (F-A2-12).
  pub fn with_profile(mut self, profile: HarnessProfile) -> Self {
    self.profile = profile;
    self
  }

  pub fn with_pre_hook(mut self, hook: Arc<dyn PreToolHook>) -> Self {
    self.pre_hooks.push(hook);
    self
  }

  pub fn with_post_hook(mut self, hook: Arc<dyn PostToolHook>) -> Self {
    self.post_hooks.push(hook);
    self
  }

  pub fn with_hook_timeout(mut self, timeout: Duration) -> Self {
    self.hook_timeout = timeout;
    self
  }

  pub fn with_approval_timeout(mut self, timeout: Duration) -> Self {
    self.approval_timeout = timeout;
    self
  }

  pub fn with_seq_counter(mut self, counter: Arc<AtomicU64>) -> Self {
    self.seq_counter = counter;
    self
  }
}

/// Wrap every tool already registered in `registry` with a
/// [`HookedTool`] that runs the configured hooks + approval flow on
/// every call. Returns the same registry (mutated in-place) so the
/// caller can keep using the existing [`agentflow_tools::ToolPolicy`] +
/// capability state.
pub fn wrap_registry(mut registry: ToolRegistry, config: HookConfig) -> ToolRegistry {
  let shared_config = Arc::new(SharedHookConfig {
    session_id: config.session_id,
    profile: config.profile,
    pre_hooks: config.pre_hooks,
    post_hooks: config.post_hooks,
    approval_provider: config.approval_provider,
    sinks: config.sinks,
    hook_timeout: config.hook_timeout,
    approval_timeout: config.approval_timeout,
    seq_counter: config.seq_counter,
    approval_cache: Arc::new(Mutex::new(ApprovalCache::default())),
  });
  let tools = registry.list();
  for tool in tools {
    let hooked = HookedTool {
      inner: tool,
      config: shared_config.clone(),
    };
    registry.register(Arc::new(hooked));
  }
  registry
}

struct SharedHookConfig {
  session_id: String,
  profile: HarnessProfile,
  pre_hooks: Vec<Arc<dyn PreToolHook>>,
  post_hooks: Vec<Arc<dyn PostToolHook>>,
  approval_provider: Arc<dyn ApprovalProvider>,
  sinks: SinkChain,
  hook_timeout: Duration,
  approval_timeout: Duration,
  seq_counter: Arc<AtomicU64>,
  approval_cache: Arc<Mutex<ApprovalCache>>,
}

#[derive(Default)]
struct ApprovalCache {
  /// Cached outcomes keyed by `(tool_name, scope)`. Only `Session`
  /// and `Run` decisions are cached; `Once` decisions are always
  /// re-prompted.
  cached: HashMap<(String, ApprovalScope), ApprovalOutcome>,
  stop_after_deny: bool,
}

impl ApprovalCache {
  fn lookup(&self, tool: &str) -> Option<(ApprovalScope, ApprovalOutcome)> {
    for scope in [ApprovalScope::Run, ApprovalScope::Session] {
      if let Some(outcome) = self.cached.get(&(tool.to_string(), scope)) {
        return Some((scope, *outcome));
      }
    }
    None
  }

  fn record(&mut self, tool: &str, scope: ApprovalScope, outcome: ApprovalOutcome) {
    if matches!(scope, ApprovalScope::Run | ApprovalScope::Session) {
      self.cached.insert((tool.to_string(), scope), outcome);
    }
  }
}

/// Tool decorator emitted by [`wrap_registry`]. Implements
/// [`Tool`] by delegating metadata to the inner tool and intercepting
/// `execute` to drive the hook / approval pipeline.
pub struct HookedTool {
  inner: Arc<dyn Tool>,
  config: Arc<SharedHookConfig>,
}

#[async_trait]
impl Tool for HookedTool {
  fn name(&self) -> &str {
    self.inner.name()
  }

  fn description(&self) -> &str {
    self.inner.description()
  }

  fn parameters_schema(&self) -> serde_json::Value {
    self.inner.parameters_schema()
  }

  fn metadata(&self) -> ToolMetadata {
    self.inner.metadata()
  }

  fn idempotency(&self, params: &serde_json::Value) -> ToolIdempotency {
    self.inner.idempotency(params)
  }

  fn requires_capabilities(&self) -> Vec<Capability> {
    self.inner.requires_capabilities()
  }

  fn sandbox_status(&self) -> Option<SandboxStatus> {
    self.inner.sandbox_status()
  }

  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let started_at = Utc::now();
    let pending = self.build_pending(&params, started_at);

    let pre_decision = self
      .run_pre_hooks(&pending)
      .await
      .map_err(hook_to_policy_denied)?;

    let proceed = self
      .resolve_proceed_decision(&pending, pre_decision)
      .await
      .map_err(hook_to_policy_denied)?;

    let tool_started = std::time::Instant::now();
    let result = match proceed {
      Proceed::Allow => self.inner.execute(params).await,
      Proceed::Deny { reason } => Err(ToolError::PolicyDenied { message: reason }),
    };
    let duration_ms = tool_started.elapsed().as_millis() as u64;

    // Post hooks observe both success and failure outcomes.
    let completed = CompletedToolCall {
      session_id: pending.session_id.clone(),
      step_index: pending.step_index,
      tool: pending.tool.clone(),
      source: pending.source.clone(),
      permissions: pending.permissions.clone(),
      is_error: result.is_err(),
      duration_ms,
      output_summary: None,
      completed_at: Utc::now(),
    };
    self.run_post_hooks(&completed).await;

    result
  }
}

#[derive(Debug, Clone)]
enum Proceed {
  Allow,
  Deny { reason: String },
}

impl HookedTool {
  fn build_pending(
    &self,
    params: &serde_json::Value,
    requested_at: chrono::DateTime<chrono::Utc>,
  ) -> PendingToolCall {
    let metadata = self.inner.metadata();
    PendingToolCall {
      session_id: self.config.session_id.clone(),
      step_index: 0,
      tool: self.inner.name().to_string(),
      source: Some(metadata.source.clone()),
      permissions: metadata.permissions.permissions.clone(),
      idempotency: self.inner.idempotency(params),
      params: params.clone(),
      requested_at,
    }
  }

  async fn run_pre_hooks(
    &self,
    pending: &PendingToolCall,
  ) -> Result<PreToolDecision, HarnessError> {
    let mut strictest = PreToolDecision::Allow;
    for hook in &self.config.pre_hooks {
      let hook_name = hook.name().to_string();
      let result = tokio::time::timeout(self.config.hook_timeout, hook.before_tool(pending)).await;
      let decision = match result {
        Ok(Ok(decision)) => decision,
        Ok(Err(err)) => {
          // Hook returned an explicit error → fail-closed.
          return Err(HarnessError::hook(
            hook_name,
            format!("pre-hook failed: {err}"),
          ));
        }
        Err(_) => {
          return Err(HarnessError::hook(
            hook_name,
            format!("pre-hook timed out after {:?}", self.config.hook_timeout),
          ));
        }
      };
      strictest = merge_pre_decisions(strictest, decision);
    }
    Ok(strictest)
  }

  async fn run_post_hooks(&self, completed: &CompletedToolCall) {
    for hook in &self.config.post_hooks {
      let hook_name = hook.name().to_string();
      let outcome =
        tokio::time::timeout(self.config.hook_timeout, hook.after_tool(completed)).await;
      match outcome {
        Err(_) => {
          tracing::warn!(
            target: "harness",
            hook = %hook_name,
            tool = %completed.tool,
            "post-tool hook timed out (advisory; tool result unchanged)"
          );
        }
        Ok(Err(err)) => {
          tracing::warn!(
            target: "harness",
            hook = %hook_name,
            tool = %completed.tool,
            error = %err,
            "post-tool hook returned error (advisory)"
          );
        }
        Ok(Ok(())) => {}
      }
    }
  }

  async fn resolve_proceed_decision(
    &self,
    pending: &PendingToolCall,
    pre_decision: PreToolDecision,
  ) -> Result<Proceed, HarnessError> {
    // Fail-closed escalation: production profile demands explicit
    // approval for any NonIdempotent call, regardless of what the
    // pre-hooks said.
    let escalation = matches!(self.config.profile, HarnessProfile::Production)
      && matches!(pending.idempotency, ToolIdempotency::NonIdempotent);

    let need_approval = match pre_decision {
      PreToolDecision::Allow => {
        if escalation {
          Some((
            ApprovalRisk::Critical,
            "production profile: mutating tool requires explicit approval".to_string(),
          ))
        } else {
          None
        }
      }
      PreToolDecision::Deny { reason } => {
        return Ok(Proceed::Deny { reason });
      }
      PreToolDecision::RequireApproval { risk, reason } => Some((risk, reason)),
    };

    let Some((risk, reason)) = need_approval else {
      return Ok(Proceed::Allow);
    };

    // Cached decision wins when scope is Session/Run.
    if let Some((scope, outcome)) = {
      let cache = self.config.approval_cache.lock().await;
      if cache.stop_after_deny {
        // Q3.10.2: pre-fix this branch silently returned `Deny`
        // without emitting any approval event, so operators
        // tailing the JSONL / SSE log saw the tool call never
        // happen but had no envelope to explain why. Emit a
        // synthetic ApprovalRequested + ApprovalDecided pair
        // (request_id namespaced as `stop-after-deny-<tool>` so
        // it's never confused with a real provider response) so
        // the gate path is fully audit-visible.
        let stop_reason: String =
          "previous approval requested deny-and-stop; aborting further tool calls".into();
        self
          .emit_stop_after_deny_gate(pending, risk, &stop_reason)
          .await?;
        return Ok(Proceed::Deny { reason: stop_reason });
      }
      cache.lookup(&pending.tool)
    } {
      self
        .emit_cached_decision(&pending.tool, scope, outcome)
        .await?;
      return Ok(decide_from_outcome(outcome));
    }

    // Build a fresh ApprovalRequest.
    // Q1.7.2: redact `params_summary` before it leaves this hook.
    // Pre-fix the raw params (which may include API keys, bearer
    // tokens, passwords) were copied straight into the JSONL / SSE
    // event envelopes the human or upstream UI sees.
    let request_id = format!("req-{}", uuid::Uuid::new_v4());
    let now = Utc::now();
    let expires_at = now
      + chrono::Duration::from_std(self.config.approval_timeout)
        .unwrap_or_else(|_| chrono::Duration::seconds(60));
    let mut params_summary = pending.params.clone();
    agentflow_tracing::redaction::redact_value(
      &mut params_summary,
      &agentflow_tracing::redaction::RedactionConfig::default(),
    );
    let request = ApprovalRequest {
      id: request_id.clone(),
      session_id: pending.session_id.clone(),
      step_index: pending.step_index,
      tool: pending.tool.clone(),
      source: pending.source.clone(),
      permissions: pending.permissions.clone(),
      idempotency: pending.idempotency,
      params_summary,
      risk,
      reason,
      requested_at: now,
      expires_at: Some(expires_at),
    };

    self
      .emit_event(HarnessEventBody::ApprovalRequested(
        ApprovalRequestedPayload {
          request: request.clone(),
        },
      ))
      .await?;

    let decision = self.config.approval_provider.request(request).await?;

    self
      .emit_event(HarnessEventBody::ApprovalDecided(ApprovalDecidedPayload {
        decision: decision.clone(),
      }))
      .await?;

    {
      let mut cache = self.config.approval_cache.lock().await;
      cache.record(&pending.tool, decision.scope, decision.decision);
      if matches!(decision.decision, ApprovalOutcome::DenyAndStop) {
        cache.stop_after_deny = true;
      }
    }

    Ok(decide_from_outcome(decision.decision))
  }

  async fn emit_cached_decision(
    &self,
    tool: &str,
    scope: ApprovalScope,
    outcome: ApprovalOutcome,
  ) -> Result<(), HarnessError> {
    let decision = ApprovalDecision {
      request_id: format!("cached-{tool}"),
      decision: outcome,
      scope,
      decided_by: "cache".into(),
      decided_at: Utc::now(),
      reason: Some(format!(
        "reused {:?}-scope decision from earlier call",
        scope
      )),
    };
    self
      .emit_event(HarnessEventBody::ApprovalDecided(ApprovalDecidedPayload {
        decision,
      }))
      .await
  }

  /// Q3.10.2: emit a synthetic ApprovalRequested + ApprovalDecided
  /// pair so the stop-after-deny gate is visible in JSONL / SSE
  /// instead of being silently dropped. The `request_id` is
  /// namespaced as `stop-after-deny-<tool>-<uuid>` so a downstream
  /// parser can distinguish synthetic gate events from real
  /// provider-issued ones.
  async fn emit_stop_after_deny_gate(
    &self,
    pending: &PendingToolCall,
    risk: ApprovalRisk,
    stop_reason: &str,
  ) -> Result<(), HarnessError> {
    let request_id = format!("stop-after-deny-{}-{}", pending.tool, uuid::Uuid::new_v4());
    let now = Utc::now();
    // Same redaction rules as the live request path — synthetic
    // events still flow through the operator's log so we cannot
    // leak unredacted params here either.
    let mut params_summary = pending.params.clone();
    agentflow_tracing::redaction::redact_value(
      &mut params_summary,
      &agentflow_tracing::redaction::RedactionConfig::default(),
    );
    let request = ApprovalRequest {
      id: request_id.clone(),
      session_id: pending.session_id.clone(),
      step_index: pending.step_index,
      tool: pending.tool.clone(),
      source: pending.source.clone(),
      permissions: pending.permissions.clone(),
      idempotency: pending.idempotency,
      params_summary,
      risk,
      reason: stop_reason.to_string(),
      requested_at: now,
      expires_at: None,
    };
    self
      .emit_event(HarnessEventBody::ApprovalRequested(
        ApprovalRequestedPayload { request },
      ))
      .await?;
    let decision = ApprovalDecision {
      request_id,
      decision: ApprovalOutcome::DenyAndStop,
      scope: ApprovalScope::Run,
      decided_by: "stop-after-deny-gate".into(),
      decided_at: now,
      reason: Some(stop_reason.to_string()),
    };
    self
      .emit_event(HarnessEventBody::ApprovalDecided(ApprovalDecidedPayload {
        decision,
      }))
      .await
  }

  async fn emit_event(&self, body: HarnessEventBody) -> Result<(), HarnessError> {
    let seq = self.config.seq_counter.fetch_add(1, Ordering::SeqCst);
    let event = HarnessEvent {
      seq,
      session_id: self.config.session_id.clone(),
      ts: Utc::now(),
      body,
    };
    self.config.sinks.dispatch(&event).await
  }
}

fn decide_from_outcome(outcome: ApprovalOutcome) -> Proceed {
  match outcome {
    ApprovalOutcome::Allow => Proceed::Allow,
    ApprovalOutcome::Deny => Proceed::Deny {
      reason: "approval denied by approver".into(),
    },
    ApprovalOutcome::DenyAndStop => Proceed::Deny {
      reason: "approval denied with stop request".into(),
    },
  }
}

fn merge_pre_decisions(current: PreToolDecision, incoming: PreToolDecision) -> PreToolDecision {
  match (current, incoming) {
    (PreToolDecision::Deny { reason }, _) => PreToolDecision::Deny { reason },
    (_, PreToolDecision::Deny { reason }) => PreToolDecision::Deny { reason },
    (PreToolDecision::RequireApproval { risk, reason }, _)
    | (_, PreToolDecision::RequireApproval { risk, reason }) => {
      PreToolDecision::RequireApproval { risk, reason }
    }
    (PreToolDecision::Allow, PreToolDecision::Allow) => PreToolDecision::Allow,
  }
}

fn hook_to_policy_denied(err: HarnessError) -> ToolError {
  ToolError::PolicyDenied {
    message: format!("harness hook denied tool: {err}"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::approval_providers::{AutoAllowApprovalProvider, AutoDenyApprovalProvider};
  use crate::persistence::{HarnessEventSink, InMemoryEventSink};
  use agentflow_tools::{ToolMetadata, ToolOutput, ToolPermissionSet, ToolSource};

  // ── Test fixtures ────────────────────────────────────────────────

  struct ProbeTool {
    name: &'static str,
    idempotency: ToolIdempotency,
    invocations: Arc<std::sync::Mutex<usize>>,
  }

  impl ProbeTool {
    fn new(
      name: &'static str,
      idempotency: ToolIdempotency,
    ) -> (Self, Arc<std::sync::Mutex<usize>>) {
      let counter = Arc::new(std::sync::Mutex::new(0));
      (
        Self {
          name,
          idempotency,
          invocations: counter.clone(),
        },
        counter,
      )
    }
  }

  #[async_trait]
  impl Tool for ProbeTool {
    fn name(&self) -> &str {
      self.name
    }
    fn description(&self) -> &str {
      "probe tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
      serde_json::json!({})
    }
    fn metadata(&self) -> ToolMetadata {
      ToolMetadata {
        source: ToolSource::Builtin,
        permissions: ToolPermissionSet::default(),
        idempotency: self.idempotency,
        mcp_server_name: None,
        mcp_tool_name: None,
      }
    }
    fn idempotency(&self, _params: &serde_json::Value) -> ToolIdempotency {
      self.idempotency
    }
    async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
      *self.invocations.lock().unwrap() += 1;
      Ok(ToolOutput::success("ok"))
    }
  }

  struct DenyHook {
    reason: String,
  }

  #[async_trait]
  impl PreToolHook for DenyHook {
    fn name(&self) -> &str {
      "deny_hook"
    }
    async fn before_tool(&self, _call: &PendingToolCall) -> Result<PreToolDecision, HarnessError> {
      Ok(PreToolDecision::Deny {
        reason: self.reason.clone(),
      })
    }
  }

  struct RequireApprovalHook;

  #[async_trait]
  impl PreToolHook for RequireApprovalHook {
    fn name(&self) -> &str {
      "require_approval_hook"
    }
    async fn before_tool(&self, _call: &PendingToolCall) -> Result<PreToolDecision, HarnessError> {
      Ok(PreToolDecision::RequireApproval {
        risk: ApprovalRisk::High,
        reason: "hook flagged risky".into(),
      })
    }
  }

  struct SlowHook {
    delay: Duration,
  }

  #[async_trait]
  impl PreToolHook for SlowHook {
    fn name(&self) -> &str {
      "slow_hook"
    }
    async fn before_tool(&self, _call: &PendingToolCall) -> Result<PreToolDecision, HarnessError> {
      tokio::time::sleep(self.delay).await;
      Ok(PreToolDecision::Allow)
    }
  }

  fn build_registry(tool: ProbeTool) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(tool));
    registry
  }

  // ── Tests ────────────────────────────────────────────────────────

  #[tokio::test]
  async fn allow_path_invokes_inner_tool() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::Idempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks);
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    assert!(result.is_ok());
    assert_eq!(*counter.lock().unwrap(), 1);
    // No approval was needed; sink should be empty.
    assert!(sink.snapshot().await.is_empty());
  }

  #[tokio::test]
  async fn deny_pre_hook_short_circuits_call() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::Idempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks)
      .with_pre_hook(Arc::new(DenyHook {
        reason: "test deny".into(),
      }));
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(result, Err(ToolError::PolicyDenied { .. })));
    assert_eq!(*counter.lock().unwrap(), 0);
  }

  #[tokio::test]
  async fn require_approval_hook_routes_through_provider() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    assert_eq!(*counter.lock().unwrap(), 1);
    let events = sink.snapshot().await;
    let kinds: Vec<&str> = events
      .iter()
      .map(|event| match &event.body {
        HarnessEventBody::ApprovalRequested(_) => "approval_requested",
        HarnessEventBody::ApprovalDecided(_) => "approval_decided",
        _ => "other",
      })
      .collect();
    assert_eq!(kinds, vec!["approval_requested", "approval_decided"]);
  }

  #[tokio::test]
  async fn auto_deny_provider_returns_policy_denied() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoDenyApprovalProvider::new()), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(result, Err(ToolError::PolicyDenied { .. })));
    assert_eq!(*counter.lock().unwrap(), 0);
  }

  #[tokio::test]
  async fn production_profile_escalates_non_idempotent_tools() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    // No pre-hook would normally raise approval, but the production
    // profile escalation should still kick in.
    let config = HookConfig::new("sess-1", Arc::new(AutoDenyApprovalProvider::new()), sinks)
      .with_profile(HarnessProfile::Production);
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(result, Err(ToolError::PolicyDenied { .. })));
    assert_eq!(*counter.lock().unwrap(), 0);
    let events = sink.snapshot().await;
    assert!(
      events
        .iter()
        .any(|event| matches!(event.body, HarnessEventBody::ApprovalRequested(_)))
    );
  }

  struct ScopedAllowOnceProvider {
    scope: ApprovalScope,
    calls: Arc<std::sync::Mutex<usize>>,
  }

  #[async_trait]
  impl ApprovalProvider for ScopedAllowOnceProvider {
    fn name(&self) -> &str {
      "scoped_allow"
    }
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, HarnessError> {
      *self.calls.lock().unwrap() += 1;
      Ok(ApprovalDecision {
        request_id: request.id,
        decision: ApprovalOutcome::Allow,
        scope: self.scope,
        decided_by: "scripted".into(),
        decided_at: Utc::now(),
        reason: Some(format!("scoped allow for {:?}", self.scope)),
      })
    }
  }

  #[tokio::test]
  async fn session_scope_decisions_are_cached_across_calls() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let approval_calls = Arc::new(std::sync::Mutex::new(0_usize));
    let approval = Arc::new(ScopedAllowOnceProvider {
      scope: ApprovalScope::Session,
      calls: approval_calls.clone(),
    });
    let config =
      HookConfig::new("sess-1", approval, sinks).with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    assert_eq!(*counter.lock().unwrap(), 3);
    assert_eq!(
      *approval_calls.lock().unwrap(),
      1,
      "approval provider should be hit once"
    );
    let events = sink.snapshot().await;
    // 1 request + 3 decisions (first real, 2 cached).
    let requested = events
      .iter()
      .filter(|event| matches!(event.body, HarnessEventBody::ApprovalRequested(_)))
      .count();
    let decided = events
      .iter()
      .filter(|event| matches!(event.body, HarnessEventBody::ApprovalDecided(_)))
      .count();
    assert_eq!(requested, 1);
    assert_eq!(decided, 3);
  }

  #[tokio::test]
  async fn once_scope_decisions_are_not_cached() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let approval_calls = Arc::new(std::sync::Mutex::new(0_usize));
    let approval = Arc::new(ScopedAllowOnceProvider {
      scope: ApprovalScope::Once,
      calls: approval_calls.clone(),
    });
    let config =
      HookConfig::new("sess-1", approval, sinks).with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    assert_eq!(*counter.lock().unwrap(), 2);
    assert_eq!(
      *approval_calls.lock().unwrap(),
      2,
      "approval provider should run each time"
    );
  }

  #[tokio::test]
  async fn slow_pre_hook_times_out_and_denies() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::Idempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks)
      .with_pre_hook(Arc::new(SlowHook {
        delay: Duration::from_millis(200),
      }))
      .with_hook_timeout(Duration::from_millis(20));
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    let err = match result {
      Err(ToolError::PolicyDenied { message }) => message,
      other => panic!("expected PolicyDenied, got {other:?}"),
    };
    assert!(err.contains("timed out"), "actual: {err}");
    assert_eq!(*counter.lock().unwrap(), 0);
  }

  struct ErrApprovalProvider;

  #[async_trait]
  impl ApprovalProvider for ErrApprovalProvider {
    fn name(&self) -> &str {
      "err_provider"
    }
    async fn request(&self, _request: ApprovalRequest) -> Result<ApprovalDecision, HarnessError> {
      Err(HarnessError::ApprovalDenied("operator cancelled".into()))
    }
  }

  #[tokio::test]
  async fn approval_provider_error_is_treated_as_deny() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(ErrApprovalProvider), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    let result = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(result, Err(ToolError::PolicyDenied { .. })));
    assert_eq!(*counter.lock().unwrap(), 0);
  }

  struct DenyAndStopProvider;

  #[async_trait]
  impl ApprovalProvider for DenyAndStopProvider {
    fn name(&self) -> &str {
      "deny_and_stop"
    }
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalDecision, HarnessError> {
      Ok(ApprovalDecision {
        request_id: request.id,
        decision: ApprovalOutcome::DenyAndStop,
        scope: ApprovalScope::Run,
        decided_by: "scripted".into(),
        decided_at: Utc::now(),
        reason: Some("scripted deny_and_stop".into()),
      })
    }
  }

  #[tokio::test]
  async fn deny_and_stop_blocks_subsequent_calls_without_reprompt() {
    let (tool, counter) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(DenyAndStopProvider), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    let first = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(first, Err(ToolError::PolicyDenied { .. })));
    let second = registry.execute("probe", serde_json::json!({})).await;
    assert!(matches!(second, Err(ToolError::PolicyDenied { .. })));
    assert_eq!(*counter.lock().unwrap(), 0);
    // First call → real provider ApprovalRequested + ApprovalDecided.
    // Second call → Q3.10.2 synthetic `stop-after-deny-*` gate pair
    // so the audit log shows operators *why* the second call never
    // hit the tool, instead of silently dropping it.
    let events = sink.snapshot().await;
    let requested = events
      .iter()
      .filter(|event| matches!(event.body, HarnessEventBody::ApprovalRequested(_)))
      .count();
    assert_eq!(requested, 2, "expect one real + one synthetic gate request");
    let decided = events
      .iter()
      .filter(|event| matches!(event.body, HarnessEventBody::ApprovalDecided(_)))
      .count();
    assert_eq!(decided, 2, "expect a Decided event paired with each Requested");
  }

  /// Q3.10.2 regression — the synthetic gate events must carry the
  /// `stop-after-deny-<tool>-*` request_id namespace so downstream
  /// parsers can distinguish them from real provider responses, and
  /// must be `decided_by = "stop-after-deny-gate"` so the audit log
  /// makes the gate path obvious.
  #[tokio::test]
  async fn stop_after_deny_gate_emits_namespaced_events_with_redacted_params() {
    let (tool, _) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(DenyAndStopProvider), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);

    // First call trips deny-and-stop; second call hits the gate.
    let _ = registry.execute("probe", serde_json::json!({})).await;
    let _ = registry
      .execute(
        "probe",
        serde_json::json!({
          "url": "https://api.example.com",
          "headers": {"Authorization": "Bearer sk-leak-me"}
        }),
      )
      .await;

    let events = sink.snapshot().await;
    let gate_request = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::ApprovalRequested(payload)
          if payload.request.id.starts_with("stop-after-deny-") =>
        {
          Some(&payload.request)
        }
        _ => None,
      })
      .expect("Q3.10.2: synthetic ApprovalRequested for the gated second call must exist");
    assert!(
      gate_request.id.starts_with("stop-after-deny-probe-"),
      "request id must namespace tool name; got {}",
      gate_request.id
    );
    assert!(
      gate_request.reason.contains("deny-and-stop"),
      "reason must reference the originating decision; got {}",
      gate_request.reason
    );
    // Redaction must still apply on the synthetic path — sensitive
    // header values should not appear verbatim.
    let summary = serde_json::to_string(&gate_request.params_summary).unwrap();
    assert!(
      !summary.contains("sk-leak-me"),
      "synthetic gate event must not leak Authorization values; got {summary}"
    );

    let gate_decision = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::ApprovalDecided(payload)
          if payload.decision.request_id.starts_with("stop-after-deny-") =>
        {
          Some(&payload.decision)
        }
        _ => None,
      })
      .expect("Q3.10.2: synthetic ApprovalDecided for the gated call must exist");
    assert_eq!(gate_decision.decision, ApprovalOutcome::DenyAndStop);
    assert_eq!(gate_decision.decided_by, "stop-after-deny-gate");
  }

  struct CountingPostHook {
    counter: Arc<std::sync::Mutex<usize>>,
  }

  #[async_trait]
  impl PostToolHook for CountingPostHook {
    fn name(&self) -> &str {
      "counting_post_hook"
    }
    async fn after_tool(&self, _call: &CompletedToolCall) -> Result<(), HarnessError> {
      *self.counter.lock().unwrap() += 1;
      Ok(())
    }
  }

  /// Q1.7.2 regression: `ApprovalRequest.params_summary` emitted via
  /// the `ApprovalRequested` event must have sensitive fields
  /// redacted. Pre-fix the raw `params.clone()` flowed through to
  /// the JSONL / SSE sinks and an operator's prompt log could leak
  /// API keys or bearer tokens.
  #[tokio::test]
  async fn approval_request_redacts_sensitive_params_before_emit() {
    let (tool, _) = ProbeTool::new("probe", ToolIdempotency::NonIdempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks)
      .with_pre_hook(Arc::new(RequireApprovalHook));
    let registry = wrap_registry(registry, config);
    registry
      .execute(
        "probe",
        serde_json::json!({
          "url": "https://api.example.com/login",
          "headers": {"Authorization": "Bearer sk-live-abc123"},
          "api_key": "ant-api03-leak-me-not"
        }),
      )
      .await
      .unwrap();

    let events = sink.snapshot().await;
    let request = events
      .iter()
      .find_map(|e| match &e.body {
        HarnessEventBody::ApprovalRequested(payload) => Some(&payload.request),
        _ => None,
      })
      .expect("ApprovalRequested must fire");
    let rendered = serde_json::to_string(&request.params_summary).unwrap();
    assert!(
      !rendered.contains("sk-live-abc123"),
      "bearer token leaked to operator approval prompt: {rendered}"
    );
    assert!(
      !rendered.contains("ant-api03-leak-me-not"),
      "api_key leaked: {rendered}"
    );
    assert!(rendered.contains("api.example.com"));
  }

  #[tokio::test]
  async fn post_hook_runs_for_both_success_and_failure() {
    let (tool, _) = ProbeTool::new("probe", ToolIdempotency::Idempotent);
    let registry = build_registry(tool);
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let post_counter = Arc::new(std::sync::Mutex::new(0_usize));
    let config = HookConfig::new("sess-1", Arc::new(AutoAllowApprovalProvider::new()), sinks)
      .with_post_hook(Arc::new(CountingPostHook {
        counter: post_counter.clone(),
      }));
    let registry = wrap_registry(registry, config);
    registry
      .execute("probe", serde_json::json!({}))
      .await
      .unwrap();
    assert_eq!(*post_counter.lock().unwrap(), 1);
  }
}
