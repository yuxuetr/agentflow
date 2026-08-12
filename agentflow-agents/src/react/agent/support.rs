use std::time::{Duration, Instant};

use agentflow_llm::ToolCallRequest;
use agentflow_memory::Message;
use agentflow_tool::{ToolIdempotency, ToolMetadata};
use serde_json::{Value, json};

use crate::react::parser::AgentResponse;
use crate::runtime::{AgentCancellationToken, AgentEvent, AgentRunResult, AgentStepKind};

/// L2.1: render a persisted [`agentflow_memory::TaskSummary`] as a system
/// message. Empty sections are omitted so an early-run summary (e.g. just
/// a goal, no steps yet) doesn't pad the prompt with empty headers.
pub(crate) fn format_task_summary_for_prompt(summary: &agentflow_memory::TaskSummary) -> String {
  let mut out = String::from(
    "[Task Summary — established before this point in the conversation, some earlier messages may have been dropped from the prompt]\n",
  );
  if !summary.goal.is_empty() {
    out.push_str(&format!("Goal: {}\n", summary.goal));
  }
  let section = |title: &str, items: &[String], out: &mut String| {
    if items.is_empty() {
      return;
    }
    out.push_str(title);
    out.push('\n');
    for item in items {
      out.push_str(&format!("- {item}\n"));
    }
  };
  section("Completed steps:", &summary.completed_steps, &mut out);
  section("Key results:", &summary.key_results, &mut out);
  section("Open questions:", &summary.open_questions, &mut out);
  section("Next steps:", &summary.next_steps, &mut out);
  out
}

/// U2.2: render the current user's stored preferences as a system
/// message. Caller already checked the list is non-empty. A string value
/// renders as bare text (`en-GB`); any other JSON shape (number, bool,
/// object) renders as its JSON literal.
pub(crate) fn format_preference_for_prompt(
  prefs: &[(String, agentflow_memory::PreferenceValue)],
) -> String {
  let mut out = String::from("[User Preferences — stated or implied in a prior session]\n");
  for (key, pref) in prefs {
    let rendered = pref
      .value
      .as_str()
      .map(str::to_string)
      .unwrap_or_else(|| pref.value.to_string());
    out.push_str(&format!("- {key}: {rendered}\n"));
  }
  out
}

/// L3.1: render accumulated project facts (commands observed run in
/// prior sessions of this project) as a system message. Caller already
/// checked `!facts.is_empty()`.
pub(crate) fn format_project_facts_for_prompt(facts: &[agentflow_memory::ProjectFact]) -> String {
  let mut out =
    String::from("[Project Memory — commands observed run in prior sessions of this project]\n");
  for fact in facts {
    out.push_str(&format!(
      "- ({}) {} — observed {} time(s), last seen {}\n",
      fact.tool,
      fact.command,
      fact.observation_count,
      fact.last_seen.to_rfc3339()
    ));
  }
  out
}

pub(crate) fn timed_out(started_at: Instant, timeout_ms: Option<u64>) -> bool {
  timeout_ms
    .map(Duration::from_millis)
    .is_some_and(|timeout| started_at.elapsed() >= timeout)
}

pub(crate) fn remaining_timeout(started_at: Instant, timeout_ms: Option<u64>) -> Option<Duration> {
  timeout_ms
    .map(Duration::from_millis)
    .map(|timeout| timeout.saturating_sub(started_at.elapsed()))
}

pub(crate) fn is_cancelled(token: &Option<AgentCancellationToken>) -> bool {
  token
    .as_ref()
    .is_some_and(AgentCancellationToken::is_cancelled)
}

/// L1.2: the `(tool, repeat count)` for whichever `(tool, params)`
/// signature appears most often in `window` — direct `PartialEq` scan
/// rather than hashing (`serde_json::Value` isn't `Hash`), which is fine
/// at the single-digit window sizes loop detection is configured with.
/// Returns `None` for an empty window.
pub(crate) fn most_repeated_signature(
  window: &std::collections::VecDeque<(String, serde_json::Value)>,
) -> Option<(String, usize)> {
  window
    .iter()
    .map(|signature| {
      let count = window.iter().filter(|other| *other == signature).count();
      (signature.0.clone(), count)
    })
    .max_by_key(|(_, count)| *count)
}

/// Convert a provider-emitted native tool call into the existing
/// `AgentResponse::Action` shape so the rest of the ReAct loop is unchanged.
///
/// `thought` is left empty: native tool calls don't carry a separate
/// reasoning field. Tool result correlation still works because the tool
/// name + arguments are preserved verbatim.
pub(crate) fn native_tool_call_to_agent_response(call: &ToolCallRequest) -> AgentResponse {
  AgentResponse::Action {
    thought: String::new(),
    tool: call.name.clone(),
    params: call.arguments.clone(),
  }
}

pub(crate) fn has_unresolved_tool_call(result: &AgentRunResult) -> bool {
  result.steps.iter().any(|step| {
    let AgentStepKind::ToolCall { tool, .. } = &step.kind else {
      return false;
    };
    !result.steps.iter().any(|candidate| {
      matches!(
        &candidate.kind,
        AgentStepKind::ToolResult {
          tool: result_tool,
          ..
        } if result_tool == tool && candidate.index > step.index
      )
    })
  })
}

pub(crate) fn tool_event_metadata(
  metadata: Option<&ToolMetadata>,
) -> (Option<String>, Vec<String>) {
  match metadata {
    Some(metadata) => (
      Some(metadata.source.as_str().to_string()),
      metadata
        .permissions
        .permissions
        .iter()
        .map(|permission| permission.as_str().to_string())
        .collect(),
    ),
    None => (None, Vec::new()),
  }
}

pub(crate) fn annotate_tool_params_for_resume(
  mut params: Value,
  idempotency: Option<ToolIdempotency>,
) -> Value {
  let Some(idempotency) = idempotency else {
    return params;
  };
  let side_effect_class = match idempotency {
    ToolIdempotency::Idempotent => "idempotent",
    ToolIdempotency::NonIdempotent => "mutating",
    ToolIdempotency::Unknown => return params,
  };

  let Value::Object(map) = &mut params else {
    return json!({
      "value": params,
      "_agentflow": {
        "side_effect_class": side_effect_class
      }
    });
  };

  let mut metadata = map.remove("_agentflow").unwrap_or_else(|| json!({}));
  if !metadata.is_object() {
    metadata = json!({});
  }
  if let Value::Object(metadata_map) = &mut metadata {
    metadata_map
      .entry("side_effect_class".to_string())
      .or_insert_with(|| json!(side_effect_class));
  }
  map.insert("_agentflow".to_string(), metadata);
  params
}

pub(crate) fn is_resume_safe_tool_call(params: &Value) -> bool {
  matches!(
    params
      .get("_agentflow")
      .and_then(|metadata| metadata.get("side_effect_class"))
      .or_else(|| params.get("side_effect_class"))
      .and_then(Value::as_str),
    Some("read_only") | Some("idempotent")
  )
}

pub(crate) fn strip_agentflow_metadata(mut params: Value) -> Value {
  let Value::Object(map) = &mut params else {
    return params;
  };
  map.remove("_agentflow");
  map.remove("side_effect_class");
  params
}

pub(crate) fn merge_resumed_result(
  mut prior: AgentRunResult,
  mut resumed: AgentRunResult,
) -> AgentRunResult {
  let step_offset = prior
    .steps
    .iter()
    .map(|step| step.index)
    .max()
    .map(|idx| idx + 1)
    .unwrap_or(0);
  for mut step in resumed.steps {
    step.index += step_offset;
    prior.steps.push(step);
  }

  let event_offset = step_offset;
  for event in &mut resumed.events {
    match event {
      AgentEvent::StepStarted { step_index, .. }
      | AgentEvent::ToolCallStarted { step_index, .. }
      | AgentEvent::ToolPolicyDecision { step_index, .. }
      | AgentEvent::ToolCapabilityDecision { step_index, .. }
      | AgentEvent::ToolCallCompleted { step_index, .. }
      | AgentEvent::LlmCallCompleted { step_index, .. }
      | AgentEvent::ReflectionAdded { step_index, .. }
      | AgentEvent::HandoffOccurred { step_index, .. }
      | AgentEvent::BlackboardWritten { step_index, .. }
      | AgentEvent::DebateVerdictRendered { step_index, .. } => {
        *step_index += event_offset;
      }
      AgentEvent::StepCompleted { step, .. } => {
        step.index += event_offset;
      }
      // Events without a `step_index` (RunStarted / RunStopped /
      // MemorySummaryAdded / DebateRoundStarted) need no offset — and neither
      // does any future variant, since `AgentEvent` is `#[non_exhaustive]`.
      _ => {}
    }
  }
  prior.events.extend(resumed.events);
  prior.answer = resumed.answer;
  prior.stop_reason = resumed.stop_reason;
  prior
}

/// Byte-index slicing (`&s[..n]`) panics when `n` falls inside a multi-byte
/// UTF-8 codepoint — tool observations and messages routinely contain CJK
/// text or emoji, so a fixed byte budget must snap down to the nearest char
/// boundary instead of assuming ASCII.
pub(crate) fn truncate_str_at_char_boundary(s: &str, max_bytes: usize) -> &str {
  if s.len() <= max_bytes {
    return s;
  }
  let mut idx = max_bytes;
  while idx > 0 && !s.is_char_boundary(idx) {
    idx -= 1;
  }
  &s[..idx]
}

pub(crate) fn compact_memory_summary(omitted: &[Message], omitted_tokens: u32) -> String {
  let mut lines = vec![format!(
    "[Memory Summary]\n{} older messages compacted (approx {} tokens):",
    omitted.len(),
    omitted_tokens
  )];
  for message in omitted.iter().take(8) {
    let mut content = message.content.replace('\n', " ");
    if content.len() > 160 {
      let cut = truncate_str_at_char_boundary(&content, 160).len();
      content.truncate(cut);
      content.push_str("...");
    }
    lines.push(format!("- {}: {}", message.role, content));
  }
  if omitted.len() > 8 {
    lines.push(format!("- ... {} more messages", omitted.len() - 8));
  }
  lines.join("\n")
}
