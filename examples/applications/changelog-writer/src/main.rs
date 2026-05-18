//! `changelog-writer` binary — A7 Plan A (binary form after skill form rejected).
//!
//! Input: a git tag range string (e.g. `v0.2.0..HEAD`).
//! Output: markdown changelog grouped by Conventional Commits type,
//! written to a destination file or stdout.
//!
//! Two AgentFlow nodes in a Flow:
//!
//! 1. `RunGitLogNode` — invokes `git log <range> ...` via std::process,
//!    captures stdout. Pure Rust, no agent involved. Validates the
//!    "shell out from inside a node" pattern.
//! 2. `ClassifyAndRenderNode` — single LLM call (Moonshot
//!    `moonshot-v1-128k`) with a strict prompt that takes raw `git log`
//!    output and returns categorized markdown. No tool calling, no
//!    ReAct loop; one prompt, one response.
//!
//! Why a binary instead of a skill: A7 dogfooding (2026-05-18) showed
//! L3 ReAct-based skills are fragile for "pass-through" tasks where
//! the LLM should forward user input verbatim to a tool. The model
//! substitutes hallucinated typical-example ranges (`v1.0.0..v2.0.0`)
//! instead of using the user's range. L1 binary path threads the
//! range as a literal string through std::process and bypasses the
//! LLM's input-substitution failure mode entirely. See README + the
//! preserved `skill.toml.rejected` next to this file for the full
//! reasoning.
//!
//! ## Usage
//!
//! ```bash
//! cd examples/applications/changelog-writer
//! export MOONSHOT_API_KEY=...     # or rely on ~/.agentflow/.env
//!
//! cargo run --release -- \
//!   --range v0.2.0..HEAD \
//!   --output /tmp/CHANGELOG.md
//!
//! # Or print to stdout:
//! cargo run --release -- --range v0.2.0..HEAD
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::error::AgentFlowError;
use agentflow_core::events::ConsoleListener;
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::value::FlowValue;
use agentflow_llm::AgentFlow as LlmInit;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn load_agentflow_dotenv() {
  if let Some(home) = std::env::home_dir() {
    let _ = dotenvy::from_path(home.join(".agentflow").join(".env"));
  }
}

/// First node — shell out to `git log` and capture stdout. Pure
/// std::process; agent isn't involved. The range comes through
/// `initial_inputs` as a `FlowValue::Json(String)` so this node
/// is a self-contained step in the Flow.
struct RunGitLogNode;

#[async_trait]
impl AsyncNode for RunGitLogNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let range = match inputs.get("range") {
      Some(FlowValue::Json(Value::String(s))) => s.clone(),
      _ => {
        return Err(AgentFlowError::NodeInputError {
          message: "input `range` must be a JSON string (e.g. \"v0.2.0..HEAD\")".into(),
        });
      }
    };

    // Standard git log invocation. `|||` field separator + `===COMMIT===`
    // block terminator make downstream parsing unambiguous even when
    // commit bodies contain newlines, pipes, or other punctuation.
    let output = Command::new("git")
      .args([
        "log",
        &range,
        "--no-merges",
        "--pretty=format:%h|||%s|||%b%n===COMMIT===",
      ])
      .output()
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("failed to spawn git: {err}"),
      })?;

    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr);
      return Err(AgentFlowError::AsyncExecutionError {
        message: format!("git log {range} failed (exit {}): {stderr}", output.status),
      });
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let count = raw.matches("===COMMIT===").count();
    info!(
      range = %range,
      commit_count = count,
      raw_bytes = raw.len(),
      "git log completed"
    );

    let mut outputs = HashMap::new();
    outputs.insert("raw".to_string(), FlowValue::Json(Value::String(raw)));
    outputs.insert("range".to_string(), FlowValue::Json(Value::String(range)));
    outputs.insert(
      "commit_count".to_string(),
      FlowValue::Json(Value::from(count)),
    );
    Ok(outputs)
  }
}

/// Second node — single LLM call to classify + render the raw git log
/// into a markdown changelog. The prompt is the full instruction; the
/// LLM does the categorization in one shot. No ReAct loop, no tool
/// calls, so the L3 pass-through failure mode (input substitution)
/// doesn't apply.
struct ClassifyAndRenderNode {
  model: String,
}

#[async_trait]
impl AsyncNode for ClassifyAndRenderNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let raw = match inputs.get("raw") {
      Some(FlowValue::Json(Value::String(s))) => s.clone(),
      _ => {
        return Err(AgentFlowError::NodeInputError {
          message: "input `raw` (git log output) must be a JSON string".into(),
        });
      }
    };
    let range = match inputs.get("range") {
      Some(FlowValue::Json(Value::String(s))) => s.clone(),
      _ => "(unknown range)".to_string(),
    };

    if raw.trim().is_empty() {
      return Err(AgentFlowError::NodeInputError {
        message: format!("git log for range `{range}` returned no commits"),
      });
    }

    let prompt = build_prompt(&range, &raw);

    let response = LlmInit::model(&self.model)
      .prompt(&prompt)
      .execute()
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("LLM call failed: {err}"),
      })?;

    let markdown = response.to_string();
    info!(
      response_chars = markdown.chars().count(),
      "classify+render LLM call complete"
    );

    let mut outputs = HashMap::new();
    outputs.insert(
      "markdown".to_string(),
      FlowValue::Json(Value::String(markdown)),
    );
    Ok(outputs)
  }
}

fn build_prompt(range: &str, raw: &str) -> String {
  // Single-shot prompt. We deliberately do NOT show example ranges
  // anywhere in the prompt — A7 dogfooding showed Moonshot models
  // happily substitute examples for user input. The range only
  // appears as a literal in the meta line, never as a "use this
  // format" example.
  format!(
    r#"You are a release-notes formatter. Below are commits from `git log {range}` in pipe-separated form, one block per `===COMMIT===` separator. Each block is `<short-hash>|||<subject>|||<body>`.

Your job: classify each commit by its Conventional Commits prefix and output a markdown changelog section. Group by type in this order, omitting empty sections:

- `feat` → `### ✨ Features`
- `fix` → `### 🐛 Bug Fixes`
- `perf` → `### ⚡ Performance`
- `refactor` → `### ♻️ Refactoring`
- `docs` → `### 📚 Documentation`
- `test` → `### 🧪 Tests`
- `ci` → `### 🔧 CI`
- `chore` → `### 🧹 Chores`
- `style` → `### 🎨 Style`
- `revert` → `### ⏪ Reverts`
- no prefix / unrecognised → `### 📦 Other`

For each commit, render exactly one bullet:

    - <subject> (`<short-hash>`)

Preserve the subject verbatim (including scope, e.g. `feat(cli): foo`).
Output **only** the markdown changelog. No preamble, no postscript,
no JSON wrapping, no explanation.

Raw git log output:

```
{raw}
```
"#
  )
}

#[derive(Debug)]
struct Args {
  range: String,
  output: Option<PathBuf>,
  model: String,
}

fn parse_args() -> Result<Args> {
  let mut range: Option<String> = None;
  let mut output: Option<PathBuf> = None;
  let mut model: String = "moonshot-v1-128k".to_string();

  let mut it = std::env::args().skip(1);
  while let Some(flag) = it.next() {
    match flag.as_str() {
      "--range" => range = Some(it.next().context("--range expects a value")?),
      "--output" => output = Some(it.next().context("--output expects a path")?.into()),
      "--model" => model = it.next().context("--model expects a model name")?,
      "-h" | "--help" => {
        print_help();
        std::process::exit(0);
      }
      other => anyhow::bail!("unknown flag `{other}` — pass --help for usage"),
    }
  }

  Ok(Args {
    range: range.context("--range is required (e.g. v0.2.0..HEAD)")?,
    output,
    model,
  })
}

fn print_help() {
  println!(
    "changelog-writer — A7 AgentFlow app (git log → markdown changelog)\n\
     \n\
     USAGE:\n  \
       changelog-writer --range <git-range> [--output <path>] [--model <name>]\n\
     \n\
     FLAGS:\n  \
       --range <git-range>  Git range string passed verbatim to `git log` (required)\n  \
       --output <path>      Write changelog to this file (default: stdout)\n  \
       --model <name>       LLM model (default: moonshot-v1-128k)\n  \
       -h, --help           Show this help\n\
     \n\
     ENV:\n  \
       MOONSHOT_API_KEY     Required by the default model. Auto-loaded\n  \
                            from ~/.agentflow/.env if present.\n"
  );
}

#[tokio::main]
async fn main() -> Result<()> {
  load_agentflow_dotenv();
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();

  let args = parse_args()?;
  info!(?args, "starting changelog-writer");

  // Initialise the LLM registry so the LlmInit::model call inside
  // ClassifyAndRenderNode resolves the model name.
  LlmInit::init()
    .await
    .context("failed to initialise agentflow-llm (model registry / provider config)")?;

  let flow = Flow::new(vec![
    GraphNode {
      id: "git_log".to_string(),
      node_type: NodeType::Standard(Arc::new(RunGitLogNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::from([(
        "range".to_string(),
        FlowValue::Json(Value::String(args.range.clone())),
      )]),
    },
    GraphNode {
      id: "classify_render".to_string(),
      node_type: NodeType::Standard(Arc::new(ClassifyAndRenderNode {
        model: args.model.clone(),
      })),
      dependencies: vec!["git_log".to_string()],
      input_mapping: Some(HashMap::from([
        (
          "raw".to_string(),
          ("git_log".to_string(), "raw".to_string()),
        ),
        (
          "range".to_string(),
          ("git_log".to_string(), "range".to_string()),
        ),
      ])),
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ])
  .with_event_listener(Arc::new(ConsoleListener));

  let state = flow.run().await?;
  let markdown = state
    .get("classify_render")
    .and_then(|res| res.as_ref().ok())
    .and_then(|outs| outs.get("markdown"))
    .and_then(|v| {
      if let FlowValue::Json(Value::String(s)) = v {
        Some(s.as_str())
      } else {
        None
      }
    })
    .ok_or_else(|| anyhow::anyhow!("classify_render node did not produce a markdown string"))?;

  if let Some(path) = &args.output {
    std::fs::write(path, markdown)
      .with_context(|| format!("failed to write changelog to {path}", path = path.display()))?;
    eprintln!("✅ changelog written to {}", path.display());
  } else {
    println!("{markdown}");
  }

  Ok(())
}
