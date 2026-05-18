use anyhow::{Context, Result};
use std::path::Path;

use super::error_context::mcp_context;
use super::runtime_options::{apply_memory_override, memory_label};
use crate::redaction::{redact_cli_text, to_redacted_json_value};
use agentflow_llm::AgentFlow;
use agentflow_skills::{SkillBuilder, SkillLoader};

/// Resolved value of `--output` (F-A2-6). Text mode preserves the
/// pre-existing emoji-prefixed banner + `🤖 Agent:` line; json mode
/// emits a single JSON object to stdout suitable for piping into other
/// tooling. All warnings and progress messages go to stderr in json
/// mode so stdout stays pure JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
  Text,
  Json,
}

impl OutputFormat {
  fn parse(value: &str) -> Result<Self> {
    match value {
      "text" => Ok(Self::Text),
      "json" => Ok(Self::Json),
      other => anyhow::bail!("unsupported --output '{other}', expected text | json"),
    }
  }
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  skill_dir: String,
  message: String,
  model_override: Option<String>,
  memory_override: Option<String>,
  session_id: Option<String>,
  trace: bool,
  output: String,
) -> Result<()> {
  let output = OutputFormat::parse(&output)?;
  let dir = Path::new(&skill_dir);

  // Load + validate manifest
  let mut manifest =
    SkillLoader::load(dir).with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

  if let Some(model) = model_override {
    manifest.model.name = Some(model);
  }
  apply_memory_override(&mut manifest, memory_override.as_deref());

  let warnings =
    SkillLoader::validate(&manifest, dir).with_context(|| "Skill validation failed")?;
  // Warnings always go to stderr; that holds for both output modes so
  // operators don't lose them when piping json into jq.
  for w in &warnings {
    eprintln!("⚠  {}", w);
  }

  if matches!(output, OutputFormat::Text) {
    println!(
      "🚀 Running skill '{}' v{}",
      manifest.skill.name, manifest.skill.version
    );
    println!("🤖 Model: {}", manifest.model.resolved_model());
    println!("🧠 Memory: {}", memory_label(&manifest));
  }

  // Initialise AgentFlow (loads LLM provider config)
  AgentFlow::init()
    .await
    .context("Failed to initialise AgentFlow — is your API key configured?")?;

  // Build the agent from the skill manifest
  let mut agent = SkillBuilder::build(&manifest, dir)
    .await
    .with_context(|| mcp_context("Failed to build agent from skill manifest", &manifest))?;

  // Optionally reuse an existing session
  if let Some(sid) = session_id {
    agent = agent.with_session_id(sid);
  }

  if matches!(output, OutputFormat::Text) {
    println!("📝 Session: {}", agent.session_id);
    println!("💬 User: {}\n", redact_cli_text(&message));
  }

  let start = std::time::Instant::now();
  let result = agent
    .run_with_trace(&message)
    .await
    .context("Agent run failed")?;
  let elapsed = start.elapsed();
  if !result.stop_reason.is_success() {
    anyhow::bail!(
      "Agent stopped before final answer: {:?}",
      result.stop_reason
    );
  }
  let answer = result.answer.clone().unwrap_or_default();

  match output {
    OutputFormat::Text => {
      println!("🤖 Agent: {}", redact_cli_text(&answer));
      if trace {
        println!("\n📋 Runtime Trace:");
        println!(
          "{}",
          serde_json::to_string_pretty(&to_redacted_json_value(&result)?)?
        );
      }
      println!("\n⏱  Completed in {:.2?}", elapsed);
    }
    OutputFormat::Json => {
      // One JSON object to stdout. Stable keys; `trace` only present
      // when --trace was passed so callers can opt-in to the larger
      // payload. Answer is redacted just like text mode.
      let mut payload = serde_json::json!({
        "skill": manifest.skill.name,
        "skill_version": manifest.skill.version,
        "model": manifest.model.resolved_model(),
        "memory": memory_label(&manifest),
        "session_id": result.session_id,
        "message": redact_cli_text(&message),
        "answer": redact_cli_text(&answer),
        "stop_reason": result.stop_reason,
        "elapsed_ms": elapsed.as_millis(),
      });
      if trace {
        payload["trace"] = to_redacted_json_value(&result)?;
      }
      println!("{}", serde_json::to_string_pretty(&payload)?);
    }
  }

  Ok(())
}
