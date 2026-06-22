//! Session-scoped context types and the [`ContextProvider`] trait.

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::HarnessError;

/// What a Harness session governs.
///
/// The agent variants map 1:1 onto `agentflow-agents` runtime kinds; `Flow`
/// (P-A2.2) is the orthogonal case where the harness governs a deterministic
/// `agentflow-graph::Flow` run rather than an agent loop. Serialized as
/// snake_case strings so CLI flags (`--runtime react`) and JSON envelopes share
/// spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessRuntimeKind {
  /// `agentflow-agents::ReActAgent` (default).
  React,
  /// `agentflow-agents::PlanExecuteAgent`.
  PlanExecute,
  /// `HandoffSupervisor` multi-agent collaboration.
  Handoff,
  /// `BlackboardSupervisor` multi-agent collaboration.
  Blackboard,
  /// `DebateSupervisor` multi-agent collaboration.
  Debate,
  /// A deterministic `agentflow-graph::Flow` run governed by the harness
  /// (P-A2.2): the harness brackets the run with `session_started` / `stopped`
  /// and tool calls inside the Flow's nodes flow through the harness
  /// hook / approval pipeline via a wrapped tool registry.
  Flow,
}

impl HarnessRuntimeKind {
  /// Stable identifier used in trace events and CLI surfaces.
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::React => "react",
      Self::PlanExecute => "plan_execute",
      Self::Handoff => "handoff",
      Self::Blackboard => "blackboard",
      Self::Debate => "debate",
      Self::Flow => "flow",
    }
  }
}

/// Security profile the Harness session is running under. Mirrors
/// `agentflow-tools::SecurityProfile` but is kept here as a stable enum
/// to avoid pulling the entire tools crate into UI / SDK consumers.
///
/// ## Approval-gate interaction (F-A2-12)
///
/// The profile is **load-bearing** for the approval flow run by
/// the harness `HookedTool` / `wrap_registry` (in `agentflow-harness`):
///
/// - [`HarnessProfile::Production`] auto-escalates every
///   `ToolIdempotency::NonIdempotent` call (shell, file:write, http
///   POST, etc.) to a `RequireApproval` decision, so the approval
///   prompt always fires before mutation.
/// - [`HarnessProfile::Local`] (the [`Default`]) and
///   [`HarnessProfile::Dev`] do **NOT** auto-escalate. Without an
///   explicit pre-hook returning `PreToolDecision::RequireApproval`,
///   tools are silently auto-allowed. This is intentional for local
///   dev ergonomics, but is the wrong default for any binary that
///   ships write-side tools — call
///   `HookConfig::with_profile` (in `agentflow-harness`) with
///   `HarnessProfile::Production` explicitly.
///
/// See `examples/applications/code-reviewer-write/src/main.rs` for a
/// reference wiring that uses Production.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessProfile {
  /// Permissive defaults for local development. Like [`Local`] for
  /// approval-gate purposes (no auto-escalation of mutating tools).
  ///
  /// [`Local`]: HarnessProfile::Local
  Dev,
  /// Conservative defaults for a personal local server.
  ///
  /// **Approval gate**: does NOT auto-escalate NonIdempotent calls.
  /// Tools are auto-allowed unless a pre-hook explicitly returns
  /// `PreToolDecision::RequireApproval`. If you want the approval
  /// prompt to fire for write tools without writing a pre-hook,
  /// use [`HarnessProfile::Production`] instead.
  #[default]
  Local,
  /// Fail-closed defaults for production deployments.
  ///
  /// **Approval gate**: auto-escalates every NonIdempotent call to
  /// `RequireApproval`, so any shell / file:write / mutating HTTP
  /// call must be explicitly approved by the
  /// [`crate::ApprovalProvider`] before the inner tool executes.
  Production,
}

impl HarnessProfile {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Dev => "dev",
      Self::Local => "local",
      Self::Production => "production",
    }
  }
}

/// Session-scoped descriptor handed to context providers and hooks.
///
/// Phase H0 freezes the shape of this struct; Phase H1 will populate it
/// inside `HarnessRuntime::start`. Extra runtime metadata can be
/// attached via [`HarnessContext::metadata`] without changing the wire
/// shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessContext {
  /// Stable id for the Harness session.
  pub session_id: String,
  /// Filesystem root the session treats as its workspace.
  pub workspace_root: PathBuf,
  /// The original user message that opened the session.
  pub user_input: String,
  /// Model id resolved by `agentflow-llm` (e.g. `step-1`).
  pub model: String,
  /// Underlying agent runtime the session is running.
  pub runtime: HarnessRuntimeKind,
  /// Active security profile.
  #[serde(default)]
  pub profile: HarnessProfile,
  /// Free-form runtime metadata (skill list, request id, parent
  /// session, etc). Keep payloads small to control trace size.
  #[serde(default, skip_serializing_if = "is_null_value")]
  pub metadata: serde_json::Value,
}

/// Priority assigned by [`ContextProvider`] implementations. The runtime
/// uses this together with [`ContextItem::token_estimate`] to assemble
/// the prompt under a budget. Higher priority items are admitted first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextPriority {
  /// Drop other context before dropping this (e.g. an explicit
  /// `AGENTS.md` instructions block).
  Critical,
  /// Important but droppable when token budget is tight.
  High,
  /// Default priority.
  #[default]
  Normal,
  /// Drop first when over budget.
  Low,
}

/// A single piece of context surfaced by a [`ContextProvider`].
///
/// Providers MUST emit structured items with priority and token cost
/// (HARNESS_MODE_EVOLUTION Risk 4); they must not dump arbitrarily
/// large files. The runtime composes items into the final prompt under
/// a configured token budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
  /// Stable identifier of the producing provider (matches
  /// [`ContextProvider::name`]).
  pub source: String,
  /// Priority used by the prompt assembler.
  #[serde(default)]
  pub priority: ContextPriority,
  /// Approximate token cost. Implementations should err on the high
  /// side rather than under-report.
  pub token_estimate: usize,
  /// The text body that will be injected into the prompt.
  pub content: String,
  /// Optional structured metadata (file path, git SHA, retrieval
  /// score, etc.) preserved alongside the item in trace events.
  #[serde(default, skip_serializing_if = "is_null_value")]
  pub metadata: serde_json::Value,
}

/// Async trait every project-context provider implements.
///
/// Providers run before the agent loop. They MUST be deterministic for
/// a given [`HarnessContext`] when no external state has changed, so
/// trace replay can reproduce the assembled prompt.
#[async_trait]
pub trait ContextProvider: Send + Sync {
  /// Stable identifier (e.g. `agents_md`, `todos_md`). Used in trace
  /// events and matches [`ContextItem::source`].
  fn name(&self) -> &str;

  /// Optional declared priority hint used by the runtime when wiring
  /// providers; falls back to [`ContextPriority::Normal`].
  fn priority_hint(&self) -> ContextPriority {
    ContextPriority::default()
  }

  /// Collect zero or more [`ContextItem`]s for the given session.
  async fn collect(&self, context: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError>;
}

fn is_null_value(value: &serde_json::Value) -> bool {
  value.is_null()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn runtime_kind_serializes_snake_case() {
    let kind = HarnessRuntimeKind::PlanExecute;
    let json = serde_json::to_string(&kind).unwrap();
    assert_eq!(json, "\"plan_execute\"");
    let parsed: HarnessRuntimeKind = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, kind);
  }

  #[test]
  fn profile_defaults_to_local() {
    assert_eq!(HarnessProfile::default(), HarnessProfile::Local);
  }

  #[test]
  fn context_priority_ordering_matches_intent() {
    assert!(ContextPriority::Critical < ContextPriority::High);
    assert!(ContextPriority::High < ContextPriority::Normal);
    assert!(ContextPriority::Normal < ContextPriority::Low);
  }

  #[test]
  fn context_item_skips_null_metadata() {
    let item = ContextItem {
      source: "agents_md".into(),
      priority: ContextPriority::Critical,
      token_estimate: 120,
      content: "do not break the build".into(),
      metadata: serde_json::Value::Null,
    };
    let json = serde_json::to_value(&item).unwrap();
    assert!(
      json.get("metadata").is_none(),
      "null metadata should be skipped: {json}"
    );
  }
}
