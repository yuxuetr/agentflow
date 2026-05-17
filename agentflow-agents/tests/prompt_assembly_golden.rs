//! Prompt-assembly golden tests for the ReAct runtime.
//!
//! These lock down the contract that downstream consumers rely on:
//!
//! 1. Given a fixed (persona / tool registry / session history) input, the
//!    message list `ReActAgent::preview_llm_messages` returns is byte-stable.
//!    Catching drift here is the cheapest possible signal that a prompt edit
//!    elsewhere in the runtime changed the wire shape unintentionally.
//! 2. When session memory exceeds `memory_prompt_token_budget`, a synthetic
//!    summary system message is injected in position 1 (after the persona
//!    system message) and the oldest history is dropped. This is the
//!    compaction crossover the agent eval and Harness use to bound prompt
//!    cost.
//! 3. After compaction, the kept-message token total respects the configured
//!    budget. The summary may itself spend tokens but it shouldn't make the
//!    overall prompt unboundedly large.
//! 4. Tool descriptions injected into the system prompt match the registry.
//!    A tool added or renamed must surface in the assembly path so callers
//!    can find out about it without enumerating the registry separately.
//!
//! Per the P0.3 additive-field contract, the snapshot assertion goes through
//! `assert_eq_json_subset` rather than byte-exact equality so adding an
//! optional field to `MultimodalMessage` doesn't force a fixture rewrite.
//! Removing or renaming a field is still caught, because every key recorded
//! in the fixture must still appear in the actual.

use std::sync::Arc;

use agentflow_agents::react::{MemorySummaryStrategy, ReActAgent, ReActConfig};
use agentflow_llm::MultimodalMessage;
use agentflow_memory::{MemoryStore, Message, SessionMemory};
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

const SESSION_ID: &str = "prompt-assembly-golden";
const MODEL: &str = "mock-prompt-assembly";

// ---- Mock tools --------------------------------------------------------------

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
  fn name(&self) -> &str {
    "echo"
  }
  fn description(&self) -> &str {
    "Echo input back to the caller."
  }
  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {"text": {"type": "string"}},
      "required": ["text"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("echo")
  }
  async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success("ok"))
  }
}

struct LookupTool;

#[async_trait]
impl Tool for LookupTool {
  fn name(&self) -> &str {
    "lookup"
  }
  fn description(&self) -> &str {
    "Lookup a value by key."
  }
  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {"key": {"type": "string"}},
      "required": ["key"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("lookup")
  }
  async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success("ok"))
  }
}

// ---- Test helpers ------------------------------------------------------------

/// Build an agent with a fixed seeded memory + persona + registry. Each test
/// gets its own agent so they don't share session state.
async fn build_agent(
  history: &[Message],
  budget: Option<u32>,
  strategy: MemorySummaryStrategy,
  with_tools: bool,
) -> ReActAgent {
  let mut memory = SessionMemory::default_window();
  for msg in history {
    memory.add_message(msg.clone()).await.unwrap();
  }

  let mut registry = ToolRegistry::new();
  if with_tools {
    registry.register(Arc::new(EchoTool));
    registry.register(Arc::new(LookupTool));
  }

  let mut config = ReActConfig::new(MODEL)
    .with_persona("You assemble golden prompts.")
    .with_memory_summary_strategy(strategy);
  if let Some(budget) = budget {
    config = config.with_memory_prompt_token_budget(budget);
  }

  ReActAgent::new(config, Box::new(memory), Arc::new(registry)).with_session_id(SESSION_ID)
}

/// Serialize messages to a JSON-friendly shape that captures only the
/// behaviorally relevant fields. Drops any metadata `HashMap` keys to keep
/// the snapshot stable across crate-version bumps that add optional fields.
fn snapshot_messages(messages: &[MultimodalMessage]) -> Value {
  let mut out = Vec::with_capacity(messages.len());
  for msg in messages {
    out.push(json!({
      "role": msg.role,
      "content": serde_json::to_value(&msg.content).unwrap(),
    }));
  }
  Value::Array(out)
}

/// Assert that every key path in `expected` also appears in `actual` with the
/// same value. Additive keys on `actual` are tolerated (P0.3 contract). For
/// arrays the lengths must match; per-element keys are checked positionally.
fn assert_json_subset(expected: &Value, actual: &Value, path: &str) {
  match (expected, actual) {
    (Value::Object(exp), Value::Object(act)) => {
      for (key, exp_val) in exp {
        let next_path = format!("{path}.{key}");
        let act_val = act.get(key).unwrap_or_else(|| {
          panic!(
            "missing key `{key}` at `{next_path}`; actual keys: {:?}",
            act.keys().collect::<Vec<_>>()
          )
        });
        assert_json_subset(exp_val, act_val, &next_path);
      }
    }
    (Value::Array(exp), Value::Array(act)) => {
      assert_eq!(
        exp.len(),
        act.len(),
        "array length mismatch at `{path}`: expected {} got {}",
        exp.len(),
        act.len()
      );
      for (i, (e, a)) in exp.iter().zip(act.iter()).enumerate() {
        assert_json_subset(e, a, &format!("{path}[{i}]"));
      }
    }
    _ => {
      assert_eq!(
        expected, actual,
        "value mismatch at `{path}`: expected {expected} got {actual}"
      );
    }
  }
}

// ---- Tests -------------------------------------------------------------------

/// Locks the deterministic prompt shape for the short-context case. A regression
/// in `build_system_prompt` (persona format, tools section, JSON instructions)
/// or `build_llm_messages` (role mapping, ordering) fails this snapshot.
#[tokio::test]
async fn prompt_assembly_short_context_matches_golden() {
  let history = vec![
    Message::user(SESSION_ID, "hello"),
    Message::assistant(SESSION_ID, "hi there"),
    Message::tool_result(SESSION_ID, "echo", "ok"),
  ];

  let agent = build_agent(&history, None, MemorySummaryStrategy::Disabled, true).await;
  let messages = agent
    .preview_llm_messages()
    .await
    .expect("preview should succeed");

  let actual = snapshot_messages(&messages);

  // Update the golden fixture by running with AGENTFLOW_PROMPT_GOLDEN_UPDATE=1.
  if std::env::var("AGENTFLOW_PROMPT_GOLDEN_UPDATE").is_ok() {
    std::fs::write(
      "tests/fixtures/prompt_assembly/short_context.json",
      serde_json::to_string_pretty(&actual).unwrap(),
    )
    .unwrap();
    return;
  }

  let expected: Value =
    serde_json::from_str(include_str!("fixtures/prompt_assembly/short_context.json"))
      .expect("fixture must be valid JSON");

  // Asserting subset (not equality) so adding an optional content type or
  // metadata key to MultimodalMessage doesn't force a fixture rewrite.
  assert_json_subset(&expected, &actual, "$");

  // Shape contract independent of the fixture: 4 messages = system + 3 history.
  assert_eq!(messages.len(), 4, "system + 3 history messages");
  assert_eq!(messages[0].role, "system");
  assert_eq!(messages[1].role, "user");
  assert_eq!(messages[2].role, "assistant");
  // Tool result is mapped to a user message with `[Tool Result: <name>]` prefix.
  assert_eq!(messages[3].role, "user");
  assert!(
    messages[3].get_text().contains("[Tool Result: echo]"),
    "tool messages must carry the tool prefix, got: {}",
    messages[3].get_text()
  );
}

/// Memory compaction crossover: when the configured budget can't fit the full
/// history, a synthetic summary system message is injected at position 1
/// (after the persona system message) and the oldest user/assistant rows are
/// dropped from the rendered prompt.
#[tokio::test]
async fn prompt_assembly_long_context_triggers_summary_message() {
  // 30 short messages * ~4 tokens each ≈ 120 tokens. Budget = 16 forces
  // the compactor to drop most of the history.
  let mut history = Vec::with_capacity(30);
  for idx in 0..30 {
    let role = idx % 2;
    let msg = if role == 0 {
      Message::user(SESSION_ID, format!("user request {idx}"))
    } else {
      Message::assistant(SESSION_ID, format!("assistant reply {idx}"))
    };
    history.push(msg);
  }

  let agent = build_agent(&history, Some(16), MemorySummaryStrategy::Compact, false).await;
  let messages = agent.preview_llm_messages().await.expect("preview");

  assert!(
    messages.len() >= 3,
    "expected ≥ persona + summary + ≥1 kept message, got {}",
    messages.len()
  );
  assert_eq!(
    messages[0].role, "system",
    "first message must be persona system"
  );
  assert_eq!(
    messages[1].role, "system",
    "second message must be the injected memory summary (system)"
  );

  let summary_text = messages[1].get_text();
  assert!(
    !summary_text.is_empty(),
    "summary message must carry text content"
  );
  // CompactMemorySummary contract documents the prefix; lock it down so a
  // backend rewrite at least surfaces here before downstream consumers
  // break.
  assert!(
    summary_text.contains("omitted") || summary_text.to_lowercase().contains("summary"),
    "summary text should reference omitted history, got: {summary_text}"
  );

  // Kept messages must be strictly fewer than the original history.
  let kept_history = messages.len() - 2; // minus 2 system messages
  assert!(
    kept_history < history.len(),
    "compaction must drop at least one history message; kept {kept_history} out of {}",
    history.len()
  );
}

/// Token budget enforcement: after compaction, the kept history must respect
/// the configured budget. The summary itself may spend a few tokens but the
/// kept-history total should be ≤ budget — that's the actual contract
/// downstream consumers (eval cost limits, harness budgets) rely on.
#[tokio::test]
async fn prompt_assembly_token_budget_respected_after_compaction() {
  // Inflate per-message size so each message costs ~16 tokens and the
  // budget can only hold one or two messages.
  let body = "x".repeat(64); // 64 / 4 = 16 token estimate
  let mut history = Vec::with_capacity(20);
  for idx in 0..20 {
    let role = idx % 2;
    let msg = if role == 0 {
      Message::user(SESSION_ID, body.clone())
    } else {
      Message::assistant(SESSION_ID, body.clone())
    };
    history.push(msg);
  }
  let budget: u32 = 32;

  let agent = build_agent(
    &history,
    Some(budget),
    MemorySummaryStrategy::Compact,
    false,
  )
  .await;
  let messages = agent.preview_llm_messages().await.expect("preview");

  // Drop the two system messages (persona + summary) when computing kept
  // history token cost. The summary itself is bounded by the backend's own
  // limit; the per-history budget is what we actually contract on.
  let kept_history_tokens: u32 = messages
    .iter()
    .skip(2)
    .map(|m| (m.get_text().len() / 4).max(1) as u32)
    .sum();

  assert!(
    kept_history_tokens <= budget,
    "kept-history token total {kept_history_tokens} must be ≤ budget {budget} (messages={})",
    messages.len()
  );
  assert!(
    messages.len() < history.len() + 1,
    "compaction must have dropped messages (got {} prompt entries for {} history)",
    messages.len(),
    history.len()
  );
}

/// Every tool registered with the agent must appear by name in the system
/// prompt's tool-listing section. Adding a tool means downstream code can
/// observe it without a separate registry walk.
#[tokio::test]
async fn prompt_assembly_tool_descriptions_in_system_prompt() {
  let agent = build_agent(&[], None, MemorySummaryStrategy::Disabled, true).await;
  let messages = agent.preview_llm_messages().await.expect("preview");

  let system_text = messages
    .first()
    .map(|m| m.get_text())
    .expect("system message present");
  assert!(
    system_text.contains("## Available Tools"),
    "system prompt must include the Available Tools section, got: {system_text}"
  );
  assert!(
    system_text.contains("echo"),
    "system prompt must reference the `echo` tool"
  );
  assert!(
    system_text.contains("lookup"),
    "system prompt must reference the `lookup` tool"
  );
  assert!(
    system_text.contains("Echo input back to the caller."),
    "system prompt must include echo description"
  );
  assert!(
    system_text.contains("Lookup a value by key."),
    "system prompt must include lookup description"
  );
}

/// No registered tools → no `## Available Tools` section, no tool-call
/// instructions. This is the contract a non-tool-bearing skill (pure prompt
/// + memory) relies on.
#[tokio::test]
async fn prompt_assembly_no_tools_omits_tools_section() {
  let agent = build_agent(&[], None, MemorySummaryStrategy::Disabled, false).await;
  let messages = agent.preview_llm_messages().await.expect("preview");

  let system_text = messages
    .first()
    .map(|m| m.get_text())
    .expect("system message present");
  assert!(
    !system_text.contains("## Available Tools"),
    "tool-less prompts must not include the Available Tools section"
  );
  assert!(
    !system_text.contains("To call a tool"),
    "tool-less prompts must not include tool-call instructions"
  );
  assert!(
    system_text.contains("To give a final answer"),
    "tool-less prompts still include the final-answer JSON instruction"
  );
}
