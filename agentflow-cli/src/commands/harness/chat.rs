//! `agentflow harness chat` — interactive multi-turn Harness REPL.
//!
//! Builds the agent + `HarnessRuntime` once, then loops: read a line from
//! stdin, run one Harness turn against a **fixed session id** (so the
//! conversation memory accumulates across turns), print the answer. Every
//! turn goes through the full harness layer — context assembly, the
//! approval gate, the event bridge, persistence — unlike `skill chat`
//! which drives the skill's agent directly.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

use agentflow_agents::runtime::{AgentCancellationToken, RuntimeLimits};
use agentflow_harness::{
  AgentsMdProvider, DeterministicContextSummarizer, HarnessEventSink, HarnessRunOptions,
  HarnessRuntime, HookConfig, JsonlEventSink, RoadmapMdProvider, SinkChain, TodosMdProvider,
  WorkspaceLayoutProvider, default_session_dir, wrap_registry,
};
use agentflow_llm::AgentFlow;
use agentflow_tools::ToolRegistry;

use super::run::{ApproveMode, build_agent, parse_runtime_kind};
use super::{parse_profile, resolve_run_dir};

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  skill_dir: Option<String>,
  model_override: Option<String>,
  session: Option<String>,
  workspace: Option<String>,
  profile: String,
  approve: String,
  runtime_kind: String,
  run_dir_override: Option<String>,
  context_budget: Option<usize>,
  token_budget: Option<u32>,
  context_refresh: bool,
  max_steps: Option<usize>,
  no_default_context: bool,
) -> Result<()> {
  let profile = parse_profile(&profile)?;
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
  // Persistent memory keyed by session_id — the chat continues across
  // turns and (with --session) across restarts.
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

  let session_id = session.unwrap_or_else(|| format!("chat-{}", uuid::Uuid::new_v4().simple()));

  let jsonl: Arc<dyn HarnessEventSink> = Arc::new(JsonlEventSink::new(session_dir.clone()));
  // One shared seq counter for the hook layer and the runtime so events
  // stay monotonic across the whole chat (Q1.7.1).
  let seq_counter = Arc::new(AtomicU64::new(0));

  if let Some(provider) = approve_mode.provider() {
    let hook_config = HookConfig::new(
      session_id.clone(),
      provider,
      SinkChain::new().push(jsonl.clone()),
    )
    .with_profile(profile)
    .with_seq_counter(seq_counter.clone());
    let mut snapshot = ToolRegistry::new();
    for tool in agent.tools().list() {
      snapshot.register(tool);
    }
    agent = agent.with_tools(Arc::new(wrap_registry(snapshot, hook_config)));
  }

  let mut runtime = if context_refresh {
    HarnessRuntime::new_turn_driven(Box::new(agent)).with_context_refresh()
  } else {
    HarnessRuntime::new(Box::new(agent))
  };
  runtime = runtime
    .with_event_sink(jsonl.clone())
    .with_seq_counter(seq_counter.clone());
  if !no_default_context {
    runtime = runtime
      .with_context_provider(Arc::new(AgentsMdProvider::new()))
      .with_context_provider(Arc::new(TodosMdProvider::new()))
      .with_context_provider(Arc::new(RoadmapMdProvider::new()))
      .with_context_provider(Arc::new(WorkspaceLayoutProvider::new()));
  }
  if context_budget.is_some() {
    runtime = runtime.with_context_summarizer(Arc::new(DeterministicContextSummarizer));
  }

  eprintln!(
    "💬 Harness chat — model: {model}{}",
    skill_name
      .as_ref()
      .map(|s| format!(", skill: {s}"))
      .unwrap_or_default()
  );
  eprintln!(
    "   session: {session_id}  (memory persists; resume later with --session {session_id})"
  );
  eprintln!("   profile: {}  ·  approve: {approve}", profile.as_str());
  eprintln!("   type a message; 'exit' / 'quit' / Ctrl-D to leave.\n");

  let mut lines = BufReader::new(tokio::io::stdin()).lines();
  loop {
    eprint!("› ");
    use std::io::Write;
    std::io::stderr().flush().ok();

    // Read the next line, but let Ctrl-C / SIGTERM break the REPL.
    let next = tokio::select! {
      biased;
      line = lines.next_line() => line,
      _ = crate::shutdown::shutdown_signal() => { eprintln!("\n🛑 bye"); break; }
    };
    let Some(input) = next.context("failed to read stdin")? else {
      eprintln!("\n👋 bye (EOF)");
      break;
    };
    let input = input.trim();
    if input.is_empty() {
      continue;
    }
    if matches!(input, "exit" | "quit" | "/exit" | "/quit") {
      eprintln!("👋 bye");
      break;
    }

    let cancel = AgentCancellationToken::new();
    let mut options = HarnessRunOptions::new(input, workspace.clone(), &model)
      .with_runtime_kind(runtime_kind)
      .with_profile(profile)
      .with_session_id(session_id.clone())
      .with_cancellation_token(cancel.clone());
    if let Some(name) = skill_name.as_ref() {
      options = options.with_skill_name(name.clone());
    }
    options = options.with_limits(RuntimeLimits {
      max_steps,
      max_tool_calls: None,
      timeout_ms: None,
      token_budget,
    });
    if let Some(budget) = context_budget {
      options = options.with_context_token_budget(budget);
    }

    // Race this turn against Ctrl-C: a long turn can be interrupted
    // without killing the whole REPL.
    let run_future = runtime.run(options);
    tokio::pin!(run_future);
    let result = tokio::select! {
      biased;
      r = &mut run_future => r,
      _ = crate::shutdown::shutdown_signal() => {
        cancel.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), &mut run_future).await;
        eprintln!("\n🛑 turn cancelled");
        continue;
      }
    };
    match result {
      Ok(res) => {
        println!("{}", res.answer.as_deref().unwrap_or("(no answer)"));
        if !res.stop_reason.is_success() {
          eprintln!("   (stopped early: {:?})", res.stop_reason);
        }
      }
      Err(err) => eprintln!("⚠️  turn failed: {err:#}"),
    }
    println!();
  }

  Ok(())
}
