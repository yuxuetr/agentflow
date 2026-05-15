//! `agentflow harness run` — bootstrap a Harness session.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::runtime::RuntimeLimits;
use agentflow_harness::{
  AgentsMdProvider, HarnessEventSink, HarnessRunOptions, HarnessRuntime, HarnessRuntimeKind,
  JsonlEventSink, RoadmapMdProvider, StdoutEventSink, TodosMdProvider, WorkspaceLayoutProvider,
  default_session_dir,
};
use agentflow_llm::AgentFlow;
use agentflow_memory::SessionMemory;
use agentflow_skills::{SkillBuilder, SkillLoader};
use agentflow_tools::ToolRegistry;

use super::{OutputFormat, parse_profile, resolve_run_dir};

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  user_input: String,
  skill_dir: Option<String>,
  model_override: Option<String>,
  session: Option<String>,
  workspace: Option<String>,
  profile: String,
  runtime_kind: String,
  output: String,
  run_dir_override: Option<String>,
  max_steps: Option<usize>,
  max_tool_calls: Option<usize>,
  timeout_ms: Option<u64>,
  no_default_context: bool,
) -> Result<()> {
  let profile = parse_profile(&profile)?;
  let output = OutputFormat::parse(&output)?;
  let runtime_kind = parse_runtime_kind(&runtime_kind)?;

  if skill_dir.is_none() && model_override.is_none() {
    anyhow::bail!("either --skill or --model is required");
  }

  let workspace = workspace
    .map(PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .context("could not determine workspace root (pass --workspace or run from a real dir)")?;
  let run_root = resolve_run_dir(run_dir_override)?;
  let session_dir = default_session_dir(&run_root);

  AgentFlow::init()
    .await
    .context("failed to initialise AgentFlow LLM config — is your API key configured?")?;

  let (agent, model, skill_name) = build_agent(skill_dir.as_deref(), model_override.as_deref())
    .await
    .context("failed to construct the inner Harness agent")?;

  // Persist every session as JSONL. Stream-json mode additionally fans
  // out the same envelope to stdout.
  let jsonl = Arc::new(JsonlEventSink::new(session_dir.clone()));
  let mut runtime = HarnessRuntime::new(Box::new(agent))
    .with_event_sink(jsonl.clone() as Arc<dyn HarnessEventSink>);
  if !no_default_context {
    runtime = runtime
      .with_context_provider(Arc::new(AgentsMdProvider::new()))
      .with_context_provider(Arc::new(TodosMdProvider::new()))
      .with_context_provider(Arc::new(RoadmapMdProvider::new()))
      .with_context_provider(Arc::new(WorkspaceLayoutProvider::new()));
  }
  if matches!(output, OutputFormat::StreamJson) {
    runtime =
      runtime.with_event_sink(Arc::new(StdoutEventSink::new()) as Arc<dyn HarnessEventSink>);
  }

  let mut options = HarnessRunOptions::new(user_input, workspace.clone(), &model)
    .with_runtime_kind(runtime_kind)
    .with_profile(profile);
  if let Some(name) = skill_name.as_ref() {
    options = options.with_skill_name(name.clone());
  }
  if let Some(session_id) = session {
    options = options.with_session_id(session_id);
  }
  options = options.with_limits(RuntimeLimits {
    max_steps,
    max_tool_calls,
    timeout_ms,
    token_budget: None,
  });

  let started = std::time::Instant::now();
  if matches!(output, OutputFormat::Text) {
    eprintln!("🚀 Harness run starting");
    eprintln!("   model: {model}");
    if let Some(name) = &skill_name {
      eprintln!("   skill: {name}");
    }
    eprintln!("   workspace: {}", workspace.display());
    eprintln!("   session log: {}", session_dir.display());
  }

  let result = runtime
    .run(options)
    .await
    .context("Harness session failed")?;
  let elapsed = started.elapsed();

  match output {
    OutputFormat::Text => {
      let answer = result.answer.as_deref().unwrap_or("(no answer)");
      println!("{answer}");
      println!();
      println!("Session: {}", result.session_id);
      println!(
        "Stop reason: {} — {} events, {} context items (admitted), elapsed {:.2?}",
        format_stop_reason(&result.stop_reason),
        result.final_event_seq + 1,
        result.context_items_admitted,
        elapsed
      );
    }
    OutputFormat::Json => {
      let payload = serde_json::json!({
        "session_id": result.session_id,
        "answer": result.answer,
        "stop_reason": result.stop_reason,
        "final_event_seq": result.final_event_seq,
        "context_items_admitted": result.context_items_admitted,
        "context_items_dropped": result.context_items_dropped,
        "model": model,
        "skill": skill_name,
        "session_log_path": jsonl.session_path(&result.session_id),
        "elapsed_ms": elapsed.as_millis(),
      });
      println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    OutputFormat::StreamJson => {
      // Events were already streamed by `StdoutEventSink`. Emit a final
      // summary line so consumers can join on a deterministic
      // terminator. The summary itself is not a HarnessEvent envelope;
      // mark it explicitly so parsers can skip it if they only want
      // the closed enum.
      let payload = serde_json::json!({
        "type": "harness_run_summary",
        "session_id": result.session_id,
        "stop_reason": result.stop_reason,
        "elapsed_ms": elapsed.as_millis(),
      });
      println!("{}", serde_json::to_string(&payload)?);
    }
  }

  if !result.stop_reason.is_success() {
    anyhow::bail!(
      "Harness session stopped before producing a final answer: {:?}",
      result.stop_reason
    );
  }

  Ok(())
}

async fn build_agent(
  skill_dir: Option<&str>,
  model_override: Option<&str>,
) -> Result<(ReActAgent, String, Option<String>)> {
  if let Some(dir) = skill_dir {
    let dir_path = std::path::Path::new(dir);
    let mut manifest =
      SkillLoader::load(dir_path).with_context(|| format!("failed to load skill from '{dir}'"))?;
    if let Some(model) = model_override {
      manifest.model.name = Some(model.to_owned());
    }
    let _warnings =
      SkillLoader::validate(&manifest, dir_path).with_context(|| "skill validation failed")?;
    let model = manifest.model.resolved_model().to_owned();
    let skill_name = manifest.skill.name.clone();
    let agent = SkillBuilder::build(&manifest, dir_path)
      .await
      .with_context(|| "failed to build agent from skill manifest")?;
    Ok((agent, model, Some(skill_name)))
  } else {
    let model = model_override
      .context("--model is required when no --skill is provided")?
      .to_owned();
    let agent = ReActAgent::new(
      ReActConfig::new(&model),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    Ok((agent, model, None))
  }
}

fn parse_runtime_kind(value: &str) -> Result<HarnessRuntimeKind> {
  match value {
    "react" => Ok(HarnessRuntimeKind::React),
    "plan_execute" | "plan-execute" => Ok(HarnessRuntimeKind::PlanExecute),
    "handoff" => Ok(HarnessRuntimeKind::Handoff),
    "blackboard" => Ok(HarnessRuntimeKind::Blackboard),
    "debate" => Ok(HarnessRuntimeKind::Debate),
    other => {
      anyhow::bail!(
        "unsupported --runtime '{other}', expected react | plan_execute | handoff | blackboard | debate"
      )
    }
  }
}

fn format_stop_reason(reason: &agentflow_agents::runtime::AgentStopReason) -> String {
  use agentflow_agents::runtime::AgentStopReason::*;
  match reason {
    FinalAnswer => "final_answer".into(),
    StopCondition { condition } => format!("stop_condition({condition})"),
    MaxSteps { max_steps } => format!("max_steps({max_steps})"),
    MaxToolCalls { max_tool_calls } => format!("max_tool_calls({max_tool_calls})"),
    Timeout { timeout_ms } => format!("timeout({timeout_ms}ms)"),
    Cancelled { message } => format!("cancelled({message})"),
    TokenBudgetExceeded { used, budget } => format!("token_budget({used}/{budget})"),
    CostLimitExceeded {
      used_usd,
      budget_usd,
    } => format!("cost_limit(${used_usd:.4}/${budget_usd:.4})"),
    Error { message } => format!("error({message})"),
  }
}
