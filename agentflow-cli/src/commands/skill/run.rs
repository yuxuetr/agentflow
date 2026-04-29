use anyhow::{Context, Result};
use std::path::Path;

use super::error_context::mcp_context;
use super::runtime_options::{apply_memory_override, memory_label};
use crate::redaction::{redact_cli_text, to_redacted_json_value};
use agentflow_llm::AgentFlow;
use agentflow_skills::{SkillBuilder, SkillLoader};

pub async fn execute(
  skill_dir: String,
  message: String,
  model_override: Option<String>,
  memory_override: Option<String>,
  session_id: Option<String>,
  trace: bool,
) -> Result<()> {
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
  for w in &warnings {
    eprintln!("⚠  {}", w);
  }

  println!(
    "🚀 Running skill '{}' v{}",
    manifest.skill.name, manifest.skill.version
  );
  println!("🤖 Model: {}", manifest.model.resolved_model());
  println!("🧠 Memory: {}", memory_label(&manifest));

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

  println!("📝 Session: {}", agent.session_id);
  println!("💬 User: {}\n", redact_cli_text(&message));

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

  println!("🤖 Agent: {}", redact_cli_text(&answer));
  if trace {
    println!("\n📋 Runtime Trace:");
    println!(
      "{}",
      serde_json::to_string_pretty(&to_redacted_json_value(&result)?)?
    );
  }
  println!("\n⏱  Completed in {:.2?}", elapsed);

  Ok(())
}
