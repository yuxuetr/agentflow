//! Resume planning — aggregates per-node resume metadata from a
//! workflow checkpoint into a single inspectable plan.
//!
//! When a checkpoint is reloaded, embedded `AgentNode` outputs may carry
//! an `agent_resume` record (`AgentNodeResumeContract` produced by
//! `agentflow-agents`) that classifies each unfinished tool call. This
//! module reads those records out of the checkpoint state pool and
//! produces a uniform [`ResumePlan`] that:
//!
//! - lists every unresolved tool call,
//! - tags each one with an idempotency classification, and
//! - assigns a [`ResumeDecision`] (replay / skip / requires manual).
//!
//! The plan is the structured input the CLI (`agentflow workflow
//! resume-plan <run-id>`), the server route
//! (`GET /v1/runs/{id}/resume-plan`), and a future Harness approval
//! flow (`P-H.2`) all share. Tool-side metadata is read out of the
//! checkpoint state without re-importing `agentflow-agents`, so the
//! core crate stays at the bottom of the dependency graph.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::checkpoint::{Checkpoint, WorkflowStatus};
use crate::error::AgentFlowError;

/// Schema version for [`ResumePlan`]. Bump on any breaking wire shape
/// change. Additive optional fields keep the same value.
pub const RESUME_PLAN_SCHEMA_VERSION: u32 = 1;

/// Replay-safety classification for a tool call, transported as a
/// stable string in [`ResumeToolCall::idempotency`]. Reuses the
/// `agentflow_tools::ToolIdempotency` vocabulary intentionally — both
/// surfaces stay in lock-step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeIdempotency {
  /// Safe to repeat with the same parameters.
  Idempotent,
  /// Not safe to repeat automatically because it mutates state or
  /// triggers external side effects.
  NonIdempotent,
  /// The tool did not declare replay semantics; the resume planner
  /// treats this as unsafe unless the operator opts in.
  Unknown,
}

impl ResumeIdempotency {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Idempotent => "idempotent",
      Self::NonIdempotent => "non_idempotent",
      Self::Unknown => "unknown",
    }
  }

  fn from_replay_policy_and_side_effect(replay_policy: &str, side_effect_class: &str) -> Self {
    match replay_policy {
      "reuse_recorded_result" | "replay_allowed" => Self::Idempotent,
      "manual_required" | "requires_idempotent_retry" => match side_effect_class {
        "read_only" | "idempotent" => Self::Idempotent,
        "mutating" | "external" => Self::NonIdempotent,
        _ => Self::Unknown,
      },
      _ => Self::Unknown,
    }
  }
}

/// Decision the planner reached for a single unresolved tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeDecision {
  /// The result is already recorded; skip the tool call and reuse the
  /// observation.
  Skip,
  /// Safe to call the tool again (idempotent or explicitly opted in
  /// via `--force-replay`).
  Replay,
  /// Unsafe to call the tool again without operator approval. The
  /// runtime must surface this to a human (CLI prompt, server-side
  /// approval, Harness ApprovalProvider, ...).
  RequiresManual,
}

impl ResumeDecision {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Skip => "skip",
      Self::Replay => "replay",
      Self::RequiresManual => "requires_manual",
    }
  }
}

/// One unresolved tool call surfaced in the resume plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumeToolCall {
  /// Workflow node that owns this tool call (matches the DAG node id).
  pub node_id: String,
  /// Stable call id taken from the agent runtime trace.
  pub tool_call_id: String,
  /// Tool name as registered with the `ToolRegistry`.
  pub tool: String,
  /// Step index inside the agent run.
  pub step_index: usize,
  /// Idempotency classification.
  pub idempotency: ResumeIdempotency,
  /// Resolved decision the planner reached.
  pub decision: ResumeDecision,
  /// Operator-readable reason for the decision.
  pub reason: String,
  /// True when a result for this call was already recorded in the
  /// agent trace (i.e. the runtime can reuse the observation).
  pub has_recorded_result: bool,
}

/// Per-decision counts surfaced as a summary block at the top of the
/// plan. Useful for `--output json` consumers and trace events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeSummary {
  pub total: usize,
  pub to_replay: usize,
  pub to_skip: usize,
  pub requires_manual: usize,
}

impl ResumeSummary {
  fn record(&mut self, decision: ResumeDecision) {
    self.total += 1;
    match decision {
      ResumeDecision::Replay => self.to_replay += 1,
      ResumeDecision::Skip => self.to_skip += 1,
      ResumeDecision::RequiresManual => self.requires_manual += 1,
    }
  }

  /// True when no tool call requires manual recovery — i.e. resume is
  /// safe to proceed without operator intervention.
  pub fn can_auto_resume(&self) -> bool {
    self.requires_manual == 0
  }
}

/// Aggregated resume plan derived from a workflow checkpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumePlan {
  pub schema_version: u32,
  pub workflow_id: String,
  pub last_completed_node: String,
  pub status: WorkflowStatus,
  pub created_at: DateTime<Utc>,
  /// Sorted by (node_id, step_index) so the output is stable across
  /// runs and trace replays.
  pub tool_calls: Vec<ResumeToolCall>,
  pub summary: ResumeSummary,
  /// True when `force_replay` was applied to upgrade `Unknown`
  /// classifications from `RequiresManual` to `Replay`.
  pub force_replay: bool,
}

/// Options that influence plan resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumePlanOptions {
  /// When true, [`ResumeIdempotency::Unknown`] entries are upgraded
  /// to [`ResumeDecision::Replay`] with an operator-supplied reason.
  /// Operators must pass `--force-replay` on the CLI / server to
  /// trigger this path. Idempotency=`NonIdempotent` entries are
  /// never auto-replayed — the planner keeps them as
  /// `RequiresManual` even when this flag is set.
  pub force_replay: bool,
}

/// Build a [`ResumePlan`] from a loaded [`Checkpoint`] and the active
/// [`ResumePlanOptions`].
pub fn build_resume_plan(
  checkpoint: &Checkpoint,
  options: &ResumePlanOptions,
) -> Result<ResumePlan, AgentFlowError> {
  let mut tool_calls: Vec<ResumeToolCall> = Vec::new();
  // Iterate over nodes in deterministic order so consumers see stable
  // output. BTreeMap implements `IntoIterator` in key order.
  let sorted: BTreeMap<&String, &Value> = checkpoint.state.iter().collect();
  for (node_id, node_state) in sorted {
    let Some(records) = extract_agent_tool_records(node_state) else {
      continue;
    };
    for record in records {
      let entry = build_entry(node_id, record, options)?;
      tool_calls.push(entry);
    }
  }
  // Final stable order: node_id then step_index then tool_call_id.
  tool_calls.sort_by(|a, b| {
    a.node_id
      .cmp(&b.node_id)
      .then_with(|| a.step_index.cmp(&b.step_index))
      .then_with(|| a.tool_call_id.cmp(&b.tool_call_id))
  });

  let mut summary = ResumeSummary::default();
  for entry in &tool_calls {
    summary.record(entry.decision);
  }

  Ok(ResumePlan {
    schema_version: RESUME_PLAN_SCHEMA_VERSION,
    workflow_id: checkpoint.workflow_id.clone(),
    last_completed_node: checkpoint.last_completed_node.clone(),
    status: checkpoint.status,
    created_at: checkpoint.created_at,
    tool_calls,
    summary,
    force_replay: options.force_replay,
  })
}

fn build_entry(
  node_id: &str,
  record: &Value,
  options: &ResumePlanOptions,
) -> Result<ResumeToolCall, AgentFlowError> {
  let tool_call_id =
    string_field(record, "call_id").ok_or_else(|| AgentFlowError::PersistenceError {
      message: format!("resume plan: tool record under node '{node_id}' is missing `call_id`"),
    })?;
  let tool = string_field(record, "tool").unwrap_or_else(|| "unknown".to_owned());
  let step_index = record
    .get("step_index")
    .and_then(|v| v.as_u64())
    .unwrap_or_default() as usize;
  let replay_policy = string_field(record, "replay_policy").unwrap_or_default();
  let side_effect_class = string_field(record, "side_effect_class").unwrap_or_default();
  let idempotency =
    ResumeIdempotency::from_replay_policy_and_side_effect(&replay_policy, &side_effect_class);
  let has_recorded_result = record
    .get("result_step_index")
    .map(|v| !v.is_null())
    .unwrap_or(false);

  let (decision, reason) = resolve_decision(
    replay_policy.as_str(),
    idempotency,
    has_recorded_result,
    options,
  );

  Ok(ResumeToolCall {
    node_id: node_id.to_owned(),
    tool_call_id,
    tool,
    step_index,
    idempotency,
    decision,
    reason,
    has_recorded_result,
  })
}

fn resolve_decision(
  replay_policy: &str,
  idempotency: ResumeIdempotency,
  has_recorded_result: bool,
  options: &ResumePlanOptions,
) -> (ResumeDecision, String) {
  if has_recorded_result || replay_policy == "reuse_recorded_result" {
    return (
      ResumeDecision::Skip,
      "result already recorded; reuse the observation".to_owned(),
    );
  }
  match (replay_policy, idempotency, options.force_replay) {
    ("replay_allowed", _, _) => (
      ResumeDecision::Replay,
      "tool declared idempotent — safe to repeat".to_owned(),
    ),
    (_, ResumeIdempotency::Idempotent, _) => (
      ResumeDecision::Replay,
      "tool classified as idempotent — safe to repeat".to_owned(),
    ),
    (_, ResumeIdempotency::Unknown, true) => (
      ResumeDecision::Replay,
      "idempotency unknown but --force-replay supplied; replay opted in by operator".to_owned(),
    ),
    (_, ResumeIdempotency::Unknown, false) => (
      ResumeDecision::RequiresManual,
      "idempotency unknown; rerun with --force-replay or resolve the call manually".to_owned(),
    ),
    (_, ResumeIdempotency::NonIdempotent, _) => (
      ResumeDecision::RequiresManual,
      "tool reports mutating / external side effects; manual recovery required".to_owned(),
    ),
  }
}

/// Pulls the `tool_calls` array out of an `agent_resume` value attached
/// to a workflow node's checkpoint output. Returns `None` when the
/// node does not expose an agent resume contract.
fn extract_agent_tool_records(node_state: &Value) -> Option<Vec<&Value>> {
  let resume = locate_agent_resume(node_state)?;
  let array = resume.get("tool_calls")?.as_array()?;
  Some(array.iter().collect())
}

fn locate_agent_resume(value: &Value) -> Option<&Value> {
  // Two checkpoint encodings exist:
  // 1. Map<NodeKey, Value> — `value` itself is the per-node output
  //    map. We look for an `agent_resume` key directly.
  // 2. The output map may also be wrapped in a `FlowValue` tag. We
  //    look one level deeper through `value`.
  if let Some(resume) = value.get("agent_resume") {
    return Some(resume_inner(resume));
  }
  // Treat `value` as a per-node output map of `{key: FlowValue::Json(...)}`.
  if let Some(map) = value.as_object() {
    if let Some(agent_resume) = map.get("agent_resume") {
      return Some(resume_inner(agent_resume));
    }
    // Fallback: search any field that contains an inner `agent_resume`
    // map (used for nested map / while node outputs).
    for inner in map.values() {
      if let Some(found) = locate_agent_resume(inner) {
        return Some(found);
      }
    }
  }
  None
}

fn resume_inner(value: &Value) -> &Value {
  if let Some(inner) = value.get("value") {
    return inner;
  }
  value
}

fn string_field(value: &Value, name: &str) -> Option<String> {
  value
    .get(name)
    .and_then(|v| v.as_str())
    .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  use std::collections::HashMap;

  fn make_checkpoint(state: HashMap<String, Value>) -> Checkpoint {
    Checkpoint {
      workflow_id: "wf-resume".into(),
      last_completed_node: "agent_node".into(),
      state,
      created_at: Utc::now(),
      status: WorkflowStatus::Running,
      metadata: HashMap::new(),
    }
  }

  fn agent_node_state(records: Vec<Value>) -> Value {
    json!({
      "agent_resume": {
        "tool_calls": records,
      }
    })
  }

  fn tool_record(
    call_id: &str,
    tool: &str,
    step_index: usize,
    side_effect: &str,
    replay_policy: &str,
    result_step: Option<usize>,
  ) -> Value {
    let mut value = json!({
      "call_id": call_id,
      "tool": tool,
      "step_index": step_index,
      "side_effect_class": side_effect,
      "replay_policy": replay_policy,
    });
    if let Some(idx) = result_step {
      value["result_step_index"] = json!(idx);
    }
    value
  }

  #[test]
  fn idempotent_tool_call_is_marked_replay() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "http",
        2,
        "idempotent",
        "replay_allowed",
        None,
      )]),
    )]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    assert_eq!(plan.tool_calls.len(), 1);
    let call = &plan.tool_calls[0];
    assert_eq!(call.idempotency, ResumeIdempotency::Idempotent);
    assert_eq!(call.decision, ResumeDecision::Replay);
    assert!(call.reason.contains("idempotent"));
    assert!(plan.summary.can_auto_resume());
  }

  #[test]
  fn non_idempotent_call_requires_manual_recovery() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "send_email",
        2,
        "mutating",
        "manual_required",
        None,
      )]),
    )]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    let call = &plan.tool_calls[0];
    assert_eq!(call.idempotency, ResumeIdempotency::NonIdempotent);
    assert_eq!(call.decision, ResumeDecision::RequiresManual);
    assert!(call.reason.contains("manual"));
    assert!(!plan.summary.can_auto_resume());
  }

  #[test]
  fn unknown_call_denied_without_force_replay() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "mystery_tool",
        2,
        "unknown",
        "manual_required",
        None,
      )]),
    )]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    let call = &plan.tool_calls[0];
    assert_eq!(call.idempotency, ResumeIdempotency::Unknown);
    assert_eq!(call.decision, ResumeDecision::RequiresManual);
    assert!(call.reason.contains("--force-replay"));
  }

  #[test]
  fn unknown_call_allowed_with_force_replay() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "mystery_tool",
        2,
        "unknown",
        "manual_required",
        None,
      )]),
    )]);
    let plan = build_resume_plan(
      &make_checkpoint(state),
      &ResumePlanOptions { force_replay: true },
    )
    .unwrap();
    let call = &plan.tool_calls[0];
    assert_eq!(call.decision, ResumeDecision::Replay);
    assert!(call.reason.contains("--force-replay"));
    assert!(plan.summary.can_auto_resume());
    assert!(plan.force_replay);
  }

  #[test]
  fn force_replay_does_not_auto_resume_mutating_calls() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "send_email",
        2,
        "mutating",
        "manual_required",
        None,
      )]),
    )]);
    let plan = build_resume_plan(
      &make_checkpoint(state),
      &ResumePlanOptions { force_replay: true },
    )
    .unwrap();
    assert_eq!(plan.tool_calls[0].decision, ResumeDecision::RequiresManual);
    assert!(!plan.summary.can_auto_resume());
  }

  #[test]
  fn recorded_result_causes_skip_decision() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![tool_record(
        "call-1",
        "search",
        2,
        "idempotent",
        "reuse_recorded_result",
        Some(3),
      )]),
    )]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    let call = &plan.tool_calls[0];
    assert_eq!(call.decision, ResumeDecision::Skip);
    assert!(call.has_recorded_result);
    assert!(call.reason.contains("already recorded"));
  }

  #[test]
  fn summary_counts_each_decision_independently() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![
        tool_record("call-1", "http", 1, "idempotent", "replay_allowed", None),
        tool_record(
          "call-2",
          "send_email",
          2,
          "mutating",
          "manual_required",
          None,
        ),
        tool_record(
          "call-3",
          "search",
          3,
          "idempotent",
          "reuse_recorded_result",
          Some(4),
        ),
      ]),
    )]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    assert_eq!(plan.summary.total, 3);
    assert_eq!(plan.summary.to_replay, 1);
    assert_eq!(plan.summary.to_skip, 1);
    assert_eq!(plan.summary.requires_manual, 1);
    assert!(!plan.summary.can_auto_resume());
  }

  #[test]
  fn nodes_without_agent_resume_are_ignored() {
    let state = HashMap::from([
      ("ordinary_node".to_string(), json!({"value": 42})),
      (
        "agent_node".to_string(),
        agent_node_state(vec![tool_record(
          "call-1",
          "http",
          1,
          "idempotent",
          "replay_allowed",
          None,
        )]),
      ),
    ]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    assert_eq!(plan.tool_calls.len(), 1);
    assert_eq!(plan.tool_calls[0].node_id, "agent_node");
  }

  #[test]
  fn tool_call_order_is_stable() {
    let mut records: Vec<Value> = (0..5)
      .rev()
      .map(|i| {
        tool_record(
          &format!("call-{i}"),
          "tool",
          i,
          "idempotent",
          "replay_allowed",
          None,
        )
      })
      .collect();
    records.reverse();
    let state = HashMap::from([("agent_node".to_string(), agent_node_state(records))]);
    let plan = build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap();
    let indices: Vec<usize> = plan.tool_calls.iter().map(|c| c.step_index).collect();
    assert_eq!(indices, (0..5).collect::<Vec<_>>());
  }

  #[test]
  fn missing_call_id_produces_persistence_error() {
    let state = HashMap::from([(
      "agent_node".to_string(),
      agent_node_state(vec![json!({"tool": "no_id"})]),
    )]);
    let err =
      build_resume_plan(&make_checkpoint(state), &ResumePlanOptions::default()).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("call_id"), "actual: {message}");
  }
}
