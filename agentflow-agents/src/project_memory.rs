//! Deterministic project-fact extraction (L3.1).
//!
//! Mirrors `task_summary`'s design (which itself mirrors
//! `agentflow-harness::ContextSummarizer`): the default extractor is
//! LLM-free, so it can only safely record what it can observe directly —
//! here, that's "this exact command was run," nothing about what the
//! command was *for*. A richer, LLM-backed [`ProjectFactGenerator`] that
//! classifies commands (build vs. test vs. deploy) or infers a tech stack
//! from file reads could be supplied instead without changing the
//! [`agentflow_memory::ProjectFact`] shape.

use async_trait::async_trait;

use crate::runtime::{AgentStep, AgentStepKind};

/// One observed `(tool, command)` pair extracted from a run's steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFactCandidate {
  pub tool: String,
  pub command: String,
}

/// Extracts [`ProjectFactCandidate`]s from a completed run's steps.
#[async_trait]
pub trait ProjectFactGenerator: Send + Sync {
  fn name(&self) -> &str;

  async fn extract(&self, steps: &[AgentStep]) -> Vec<ProjectFactCandidate>;
}

/// LLM-free, deterministic [`ProjectFactGenerator`]: every
/// `shell`/`script`/`code_exec` tool call's command becomes a candidate.
/// Deduplicated within the run (the store's own upsert handles dedup
/// *across* runs) so a command looped 50 times in one session doesn't
/// produce 50 identical candidates.
#[derive(Debug, Default, Clone)]
pub struct DeterministicProjectFactGenerator;

#[async_trait]
impl ProjectFactGenerator for DeterministicProjectFactGenerator {
  fn name(&self) -> &str {
    "deterministic"
  }

  async fn extract(&self, steps: &[AgentStep]) -> Vec<ProjectFactCandidate> {
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    for step in steps {
      let AgentStepKind::ToolCall { tool, params } = &step.kind else {
        continue;
      };
      let command = match tool.as_str() {
        "shell" => params.get("command").and_then(|v| v.as_str()),
        "script" => params.get("script").and_then(|v| v.as_str()),
        // S4.2: code_exec is the same "ran something in this project"
        // signal as shell/script, just llm-generated Python instead of a
        // shell one-liner or an author-signed script filename — recorded
        // verbatim like the other two, no summarization (this extractor
        // stays LLM-free by design, see the module doc comment).
        "code_exec" => params.get("code").and_then(|v| v.as_str()),
        _ => None,
      };
      let Some(command) = command else { continue };
      let key = (tool.clone(), command.to_string());
      if seen.insert(key) {
        candidates.push(ProjectFactCandidate {
          tool: tool.clone(),
          command: command.to_string(),
        });
      }
    }
    candidates
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use serde_json::json;

  fn tool_call_step(index: usize, tool: &str, params: serde_json::Value) -> AgentStep {
    AgentStep {
      index,
      kind: AgentStepKind::ToolCall {
        tool: tool.to_string(),
        params,
      },
      timestamp: Utc::now(),
      duration_ms: None,
    }
  }

  #[tokio::test]
  async fn extracts_shell_commands() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![tool_call_step(
      0,
      "shell",
      json!({"command": "cargo build"}),
    )];
    let candidates = generator.extract(&steps).await;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].tool, "shell");
    assert_eq!(candidates[0].command, "cargo build");
  }

  #[tokio::test]
  async fn extracts_script_commands() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![tool_call_step(
      0,
      "script",
      json!({"script": "check_syntax.py"}),
    )];
    let candidates = generator.extract(&steps).await;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].tool, "script");
    assert_eq!(candidates[0].command, "check_syntax.py");
  }

  #[tokio::test]
  async fn extracts_code_exec_code() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![tool_call_step(
      0,
      "code_exec",
      json!({"code": "print(sum(range(10)))"}),
    )];
    let candidates = generator.extract(&steps).await;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].tool, "code_exec");
    assert_eq!(candidates[0].command, "print(sum(range(10)))");
  }

  #[tokio::test]
  async fn ignores_non_shell_script_tools() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![tool_call_step(
      0,
      "file",
      json!({"operation": "read", "path": "README.md"}),
    )];
    let candidates = generator.extract(&steps).await;
    assert!(candidates.is_empty());
  }

  #[tokio::test]
  async fn ignores_non_tool_call_steps() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![AgentStep {
      index: 0,
      kind: AgentStepKind::Plan {
        thought: "let's run the build".to_string(),
      },
      timestamp: Utc::now(),
      duration_ms: None,
    }];
    let candidates = generator.extract(&steps).await;
    assert!(candidates.is_empty());
  }

  #[tokio::test]
  async fn dedups_repeated_commands_within_one_run() {
    let generator = DeterministicProjectFactGenerator;
    let steps = vec![
      tool_call_step(0, "shell", json!({"command": "cargo test"})),
      tool_call_step(1, "shell", json!({"command": "cargo test"})),
    ];
    let candidates = generator.extract(&steps).await;
    assert_eq!(candidates.len(), 1);
  }
}
