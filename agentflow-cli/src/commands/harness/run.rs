//! `agentflow harness run` — bootstrap a Harness session.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use anyhow::{Context, Result};

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::runtime::{AgentCancellationToken, RuntimeLimits};
use agentflow_harness::{
  AgentsMdProvider, ApprovalProvider, AutoAllowApprovalProvider, AutoDenyApprovalProvider,
  CliApprovalProvider, DeterministicContextSummarizer, HarnessEventSink, HarnessRunOptions,
  HarnessRuntime, HarnessRuntimeKind, HookConfig, JsonlEventSink, RoadmapMdProvider, SinkChain,
  StdoutEventSink, TodosMdProvider, WorkspaceLayoutProvider, default_session_dir, wrap_registry,
};
use agentflow_llm::AgentFlow;
use agentflow_memory::SqliteMemory;
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
  approve: String,
  runtime_kind: String,
  output: String,
  run_dir_override: Option<String>,
  max_steps: Option<usize>,
  max_tool_calls: Option<usize>,
  timeout_ms: Option<u64>,
  context_budget: Option<usize>,
  token_budget: Option<u32>,
  context_refresh: bool,
  no_default_context: bool,
) -> Result<()> {
  let profile = parse_profile(&profile)?;
  let output = OutputFormat::parse(&output)?;
  let runtime_kind = parse_runtime_kind(&runtime_kind)?;
  let approve_mode = ApproveMode::parse(&approve)?;

  if skill_dir.is_none() && model_override.is_none() {
    anyhow::bail!("either --skill or --model is required");
  }

  let workspace = workspace
    .map(PathBuf::from)
    .or_else(|| std::env::current_dir().ok())
    .context("could not determine workspace root (pass --workspace or run from a real dir)")?;
  let run_root = resolve_run_dir(run_dir_override)?;
  let session_dir = default_session_dir(&run_root);
  // Persist conversation memory under the run-dir so `--session <id>`
  // resumes the prior turns (long-lived sessions). One DB keyed by
  // session_id; only the `--model` path uses it (the `--skill` path's
  // memory is configured by the skill manifest).
  let memory_db = run_root.join("harness").join("memory.sqlite");
  if let Some(parent) = memory_db.parent() {
    std::fs::create_dir_all(parent)
      .with_context(|| format!("could not create harness memory dir {}", parent.display()))?;
  }

  AgentFlow::init()
    .await
    .context("failed to initialise AgentFlow LLM config — is your API key configured?")?;

  let (mut agent, model, skill_name) =
    build_agent(skill_dir.as_deref(), model_override.as_deref(), &memory_db)
      .await
      .context("failed to construct the inner Harness agent")?;

  // Persist every session as JSONL. Stream-json mode additionally fans
  // out the same envelope to stdout.
  let jsonl = Arc::new(JsonlEventSink::new(session_dir.clone()));
  let jsonl_sink: Arc<dyn HarnessEventSink> = jsonl.clone();
  let stdout_sink: Option<Arc<dyn HarnessEventSink>> = matches!(output, OutputFormat::StreamJson)
    .then(|| Arc::new(StdoutEventSink::new()) as Arc<dyn HarnessEventSink>);

  // Resolve session id eagerly so HookConfig and HarnessRuntime share
  // the same id namespace. Mirrors the server's `LiveHarnessExecutor`
  // pattern; if the user passed --session we honour that, otherwise
  // generate a fresh one.
  let session_id = session.unwrap_or_else(|| format!("session-{}", uuid::Uuid::new_v4().simple()));

  // ── F-A2-11: wrap the agent's tool registry with the approval-gate
  // pipeline if requested. Without this, `agentflow harness run` had
  // no approval flow at all (the bare ReActAgent went straight to the
  // inner tools, even under --profile production), forcing users to
  // hand-roll binaries to dogfood Harness Mode from the CLI.
  if let Some(provider) = approve_mode.provider() {
    let mut hook_sinks = SinkChain::new().push(jsonl_sink.clone());
    if let Some(sink) = stdout_sink.as_ref() {
      hook_sinks = hook_sinks.push(sink.clone());
    }
    let hook_config = HookConfig::new(session_id.clone(), provider, hook_sinks)
      .with_profile(profile)
      .with_seq_counter(Arc::new(AtomicU64::new(0)));

    // Snapshot the agent's current registry into a fresh one so
    // `wrap_registry` can decorate each tool. Tools come back as the
    // same `Arc<dyn Tool>` instances so any inner state (sandbox
    // policy, MCP session, etc.) is preserved.
    let mut snapshot = ToolRegistry::new();
    for tool in agent.tools().list() {
      snapshot.register(tool);
    }
    let wrapped = wrap_registry(snapshot, hook_config);
    agent = agent.with_tools(Arc::new(wrapped));
  }

  // §6: `--context-refresh` drives the agent turn-by-turn at the harness
  // layer (ReActAgent implements `TurnDrivenRuntime`) and re-runs the
  // context providers between turns; otherwise the agent owns iteration.
  let mut runtime = if context_refresh {
    HarnessRuntime::new_turn_driven(Box::new(agent)).with_context_refresh()
  } else {
    HarnessRuntime::new(Box::new(agent))
  };
  runtime = runtime.with_event_sink(jsonl_sink.clone());
  if !no_default_context {
    runtime = runtime
      .with_context_provider(Arc::new(AgentsMdProvider::new()))
      .with_context_provider(Arc::new(TodosMdProvider::new()))
      .with_context_provider(Arc::new(RoadmapMdProvider::new()))
      .with_context_provider(Arc::new(WorkspaceLayoutProvider::new()));
  }
  if let Some(sink) = stdout_sink.as_ref() {
    runtime = runtime.with_event_sink(sink.clone());
  }
  // Phase 2a: when a context budget is set, compact over-budget context
  // into a summary (deterministic, replay-safe) instead of dropping it.
  if context_budget.is_some() {
    runtime = runtime.with_context_summarizer(Arc::new(DeterministicContextSummarizer));
  }

  // Q3.1.2: shared cancellation token so the Ctrl-C / SIGTERM future
  // below can ask the inner agent to stop the loop after the current
  // step instead of the runtime aborting the in-flight LLM call.
  let cancel_token = AgentCancellationToken::new();
  let mut options = HarnessRunOptions::new(user_input, workspace.clone(), &model)
    .with_runtime_kind(runtime_kind)
    .with_profile(profile)
    .with_session_id(session_id)
    .with_cancellation_token(cancel_token.clone());
  if let Some(name) = skill_name.as_ref() {
    options = options.with_skill_name(name.clone());
  }
  options = options.with_limits(RuntimeLimits {
    max_steps,
    max_tool_calls,
    timeout_ms,
    token_budget,
  });
  if let Some(budget) = context_budget {
    options = options.with_context_token_budget(budget);
  }

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

  // Q3.1.2: race the harness session against SIGINT/SIGTERM. On signal
  // we trip the cancellation token (the inner ReAct/plan-execute loop
  // notices on its next iteration and exits with `AgentStopReason::Cancelled`),
  // give the agent a bounded drain window to surface the terminal
  // `stopped` event through the sink chain, then exit 130.
  let run_future = runtime.run(options);
  tokio::pin!(run_future);
  let result = tokio::select! {
    biased;
    res = &mut run_future => res.context("Harness session failed")?,
    _ = crate::shutdown::shutdown_signal() => {
      eprintln!("\n🛑 Cancelled (received SIGINT/SIGTERM)");
      cancel_token.cancel();
      const CANCEL_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
      let _ = tokio::time::timeout(CANCEL_DRAIN_TIMEOUT, &mut run_future).await;
      std::process::exit(crate::shutdown::SIGINT_EXIT_CODE);
    }
  };
  let elapsed = started.elapsed();

  match output {
    OutputFormat::Text => {
      let answer = result.answer.as_deref().unwrap_or("(no answer)");
      println!("{answer}");
      println!();
      println!("Session: {}", result.session_id);
      println!(
        "Stop reason: {} — {} events, {} context items (admitted, {} truncated, {} dropped), elapsed {:.2?}",
        format_stop_reason(&result.stop_reason),
        result.final_event_seq + 1,
        result.context_items_admitted,
        result.context_items_truncated,
        result.context_items_dropped,
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
        "context_items_truncated": result.context_items_truncated,
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
    OutputFormat::JsonEnvelope => {
      // P3.3 migration: wrap the same summary the `json` mode emits
      // in the canonical envelope. `stream-json` keeps its raw
      // per-line event format because envelope-per-line would
      // defeat stream framing — operators wanting the run summary
      // alongside live events can run with stream-json and parse
      // the trailing `harness_run_summary` line.
      let payload = serde_json::json!({
        "session_id": result.session_id,
        "answer": result.answer,
        "stop_reason": result.stop_reason,
        "final_event_seq": result.final_event_seq,
        "context_items_admitted": result.context_items_admitted,
        "context_items_truncated": result.context_items_truncated,
        "context_items_dropped": result.context_items_dropped,
        "model": model,
        "skill": skill_name,
        "session_log_path": jsonl.session_path(&result.session_id),
        "elapsed_ms": elapsed.as_millis(),
      });
      // Surface a non-success stop reason as an actionable error
      // string so shell consumers can branch on `errors.length > 0`
      // without inspecting `result.stop_reason`.
      let errors: Vec<String> = if result.stop_reason.is_success() {
        Vec::new()
      } else {
        vec![format!(
          "Harness session stopped before final answer: {:?}",
          result.stop_reason
        )]
      };
      let envelope =
        crate::json_envelope::CliJsonEnvelope::with_errors("harness run", &payload, errors);
      println!("{}", serde_json::to_string_pretty(&envelope)?);
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
  memory_db: &std::path::Path,
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
    // Persistent conversation memory keyed by session_id: a `harness run
    // --session <id>` that reuses an id reads the prior turns back, so
    // long-lived sessions continue across processes.
    let memory = SqliteMemory::open(memory_db).await.with_context(|| {
      format!(
        "failed to open harness memory db at {}",
        memory_db.display()
      )
    })?;
    let agent = ReActAgent::new(
      ReActConfig::new(&model),
      Box::new(memory),
      Arc::new(ToolRegistry::new()),
    );
    Ok((agent, model, None))
  }
}

/// Resolved value of the `--approve` flag.
///
/// `None` means the legacy "no wrapping" behaviour: the inner agent
/// drives tools directly, the `ApprovalProvider` is never instantiated,
/// and `--profile` is only used for `HarnessContext`. Any other mode
/// installs a [`HookConfig`] with the matching provider so every
/// NonIdempotent tool call passes through the approval gate (and is
/// auto-escalated to `RequireApproval` under `--profile production`).
#[derive(Debug, Clone, Copy)]
enum ApproveMode {
  None,
  Cli,
  AutoAllow,
  AutoDeny,
}

impl ApproveMode {
  fn parse(value: &str) -> Result<Self> {
    match value {
      "none" => Ok(Self::None),
      "cli" => Ok(Self::Cli),
      "auto-allow" => Ok(Self::AutoAllow),
      "auto-deny" => Ok(Self::AutoDeny),
      other => anyhow::bail!(
        "unsupported --approve '{other}', expected none | cli | auto-allow | auto-deny"
      ),
    }
  }

  fn provider(self) -> Option<Arc<dyn ApprovalProvider>> {
    match self {
      Self::None => None,
      Self::Cli => Some(Arc::new(CliApprovalProvider::stdin())),
      Self::AutoAllow => Some(Arc::new(AutoAllowApprovalProvider::new())),
      Self::AutoDeny => Some(Arc::new(
        AutoDenyApprovalProvider::new().with_stop_on_deny(true),
      )),
    }
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

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_memory::Message;

  /// Resume contract: `build_agent` (the `--model` path) backs the agent
  /// with a persistent SQLite store at the run-dir path, keyed by
  /// session_id. A second `build_agent` against the same DB therefore
  /// reads the prior conversation back — which is what makes
  /// `harness run --session <id>` continue a long-lived session across
  /// processes.
  #[tokio::test]
  async fn build_agent_persists_memory_for_resume() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("memory.sqlite");

    // First "run": construct the agent and record a turn for a session.
    let (agent1, _model, _skill) = build_agent(None, Some("mock"), &db).await.unwrap();
    agent1
      .memory_ref()
      .add_message(Message::user("sess-resume", "remember the secret token"))
      .await
      .unwrap();
    drop(agent1);

    // Second "run" (resume): same DB + same session id sees the message.
    let (agent2, _model, _skill) = build_agent(None, Some("mock"), &db).await.unwrap();
    let history = agent2.memory_ref().get_all("sess-resume").await.unwrap();
    assert!(
      history
        .iter()
        .any(|m| m.content.contains("remember the secret token")),
      "resume must restore the prior conversation from the persistent store"
    );
  }
}
