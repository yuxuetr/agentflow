use clap::{Args, Subcommand};

use super::{chat, inspect, list, replay, resume, resume_loop, run, run_flow};

#[derive(Args)]
pub struct HarnessArgs {
  #[command(subcommand)]
  command: HarnessCommands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum HarnessCommands {
  /// Run a Harness session and stream events to disk (+ optional stdout)
  Run {
    /// User input that opens the session
    input: String,
    /// Path to a skill directory to load (optional)
    #[arg(long)]
    skill: Option<String>,
    /// Model id (required when no --skill is supplied)
    #[arg(long)]
    model: Option<String>,
    /// Resume an existing session id rather than generating a fresh one.
    /// Conversation memory is persisted (SQLite under the run-dir, keyed
    /// by session id), so reusing an id continues the prior turns across
    /// processes — a long-lived session. (Applies to the `--model` path;
    /// the `--skill` path's memory is configured by the skill manifest.)
    #[arg(long)]
    session: Option<String>,
    /// Workspace root (default: current working directory)
    #[arg(long)]
    workspace: Option<String>,
    /// Security profile
    #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
    profile: String,
    /// Approval-gate provider for the wrapped tool registry. `none`
    /// means no `HookedTool` wrapping, no approval prompt, regardless of
    /// `--profile`. Pass `cli` for an interactive stdin prompt per
    /// NonIdempotent call, `auto-allow` for CI smoke, or `auto-deny` for
    /// a fail-closed default that also halts the run on first deny.
    /// Combine with `--profile production` to make every NonIdempotent
    /// tool (shell / file:write / mutating HTTP) escalate to a required
    /// approval before executing (F-A2-11). Unset defaults to `cli`
    /// under `local`/`production` `--profile` and to `none` under `dev`
    /// (U2.3, mirrors `workflow dynamic`'s T1.3 default) — pass
    /// `--approve none` explicitly to run unsupervised on any profile.
    #[arg(long, value_parser = ["none", "cli", "auto-allow", "auto-deny"])]
    approve: Option<String>,
    /// Underlying agent runtime
    #[arg(long, default_value = "react", value_parser = ["react", "plan_execute", "plan-execute", "handoff", "blackboard", "debate"])]
    runtime: String,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
    /// Override the run-dir (session log root). Defaults to AGENTFLOW_RUN_DIR or ~/.agentflow/runs
    #[arg(long)]
    run_dir: Option<String>,
    /// Maximum total agent steps
    #[arg(long)]
    max_steps: Option<usize>,
    /// Maximum total tool calls
    #[arg(long)]
    max_tool_calls: Option<usize>,
    /// Wall-clock timeout in milliseconds
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Soft cap (tokens) on the assembled workspace context. When set, the
    /// budget is enforced with the model's real tokenizer and over-budget
    /// items are compacted into a summary (emitting `memory_summary_added`)
    /// rather than dropped.
    #[arg(long)]
    context_budget: Option<usize>,
    /// Agent prompt-memory token budget. When the conversation exceeds it
    /// the agent compacts older turns mid-run (surfaced as
    /// `memory_summary_added`).
    #[arg(long)]
    token_budget: Option<u32>,
    /// Maximum estimated USD cost for the session. Runtime enforcement
    /// was already wired into `RuntimeLimits`/`ReActAgent`/
    /// `PlanExecuteAgent` by T1.1; this flag is the CLI entry point for
    /// it (U1.3). The agent stops once cumulative estimated spend
    /// reaches this value.
    #[arg(long)]
    cost_limit_usd: Option<f64>,
    /// Drive the agent loop turn-by-turn at the harness layer and re-run
    /// the context providers between turns, injecting refreshed workspace
    /// context when it changed (so a long-running agent perceives edits to
    /// AGENTS.md / TODOs.md / the workspace mid-run). Emits
    /// `memory_summary_added` with `layer = "context_refresh"`.
    #[arg(long)]
    context_refresh: bool,
    /// Skip the default workspace context providers (AGENTS.md / TODOs.md / ...)
    #[arg(long)]
    no_default_context: bool,
  },
  /// Run a config workflow (DAG) under harness governance (P-A2.2), streaming
  /// the Harness envelope (`session_started` runtime=flow → per-node
  /// `step_started` → `stopped`) to disk + optional stdout.
  RunFlow {
    /// Path to the workflow YAML file.
    workflow_file: String,
    /// Optional model override applied to the workflow's LLM nodes.
    #[arg(long)]
    model: Option<String>,
    /// Initial input as key=value (repeatable); values parse as JSON when possible.
    #[arg(long = "input")]
    input: Vec<String>,
    /// Security profile recorded on the session.
    #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
    profile: String,
    /// Output format.
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
    /// Workspace root (default: current working directory).
    #[arg(long)]
    workspace: Option<String>,
    /// Override the run-dir (session log root). Defaults to AGENTFLOW_RUN_DIR or ~/.agentflow/runs.
    #[arg(long)]
    run_dir: Option<String>,
    /// Wall-clock timeout in milliseconds for the whole flow run.
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Resume / correlate a specific session id (default: a fresh one).
    #[arg(long)]
    session: Option<String>,
    /// Maximum concurrently running nodes.
    #[arg(long, default_value_t = 8)]
    max_concurrency: usize,
  },
  /// Interactive multi-turn Harness chat (REPL). Each message runs one
  /// Harness turn against a fixed session id, so the conversation
  /// continues across turns (and, with --session, across restarts).
  Chat {
    /// Path to a skill directory to load (optional)
    #[arg(long)]
    skill: Option<String>,
    /// Model id (required when no --skill is supplied)
    #[arg(long)]
    model: Option<String>,
    /// Resume / continue a specific session id (default: a fresh chat id)
    #[arg(long)]
    session: Option<String>,
    /// Workspace root (default: current working directory)
    #[arg(long)]
    workspace: Option<String>,
    /// Security profile
    #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
    profile: String,
    /// Approval-gate provider (see `harness run --help`). Unset defaults
    /// to `cli` under `local`/`production` `--profile`, `none` under
    /// `dev` (U2.3) — pass `--approve none` explicitly to run
    /// unsupervised on any profile.
    #[arg(long, value_parser = ["none", "cli", "auto-allow", "auto-deny"])]
    approve: Option<String>,
    /// Underlying agent runtime
    #[arg(long, default_value = "react", value_parser = ["react", "plan_execute", "plan-execute", "handoff", "blackboard", "debate"])]
    runtime: String,
    /// Override the run-dir (session log + memory root)
    #[arg(long)]
    run_dir: Option<String>,
    /// Soft cap (tokens) on assembled context; over-budget context is compacted
    #[arg(long)]
    context_budget: Option<usize>,
    /// Agent prompt-memory token budget (compacts older turns mid-run)
    #[arg(long)]
    token_budget: Option<u32>,
    /// Maximum estimated USD cost, enforced across the whole chat session
    /// (see `harness run --help`)
    #[arg(long)]
    cost_limit_usd: Option<f64>,
    /// Drive turns at the harness layer + refresh workspace context between turns
    #[arg(long)]
    context_refresh: bool,
    /// Maximum agent steps per message
    #[arg(long)]
    max_steps: Option<usize>,
    /// Skip the default workspace context providers
    #[arg(long)]
    no_default_context: bool,
  },
  /// Replay a persisted Harness session log (no LLM is invoked).
  Resume {
    /// Session id to replay
    session_id: String,
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
  },
  /// Resume a ReAct agent loop from its last saved checkpoint (V2.4),
  /// continuing from the interrupted turn instead of restarting the
  /// session from scratch. Distinct from `resume` (JSONL replay-only,
  /// prints the persisted event log) — this actually re-enters the agent
  /// loop and keeps executing. Requires `harness run` to have been
  /// invoked earlier against the same `--run-dir` (loop checkpointing is
  /// on by default there) and a checkpoint to still exist (cleared
  /// automatically once a session finishes normally).
  ResumeLoop {
    /// Session id whose checkpoint should be resumed
    session_id: String,
    /// Path to a skill directory to load (optional) — must match the
    /// interrupted run's `--skill`/`--model`.
    #[arg(long)]
    skill: Option<String>,
    /// Model id (required when no --skill is supplied)
    #[arg(long)]
    model: Option<String>,
    /// Workspace root (default: current working directory)
    #[arg(long)]
    workspace: Option<String>,
    /// Override the run-dir (must match the interrupted run's)
    #[arg(long)]
    run_dir: Option<String>,
    /// Agent runtime that produced the checkpoint — must match the
    /// interrupted run's `--runtime` (V2.3: `react` and `plan_execute`
    /// checkpoints use different loop-state shapes and are not
    /// interchangeable). Only these two are constructible via `--skill`
    /// / `--model` today, unlike `run`/`chat`'s broader `--runtime` list.
    #[arg(long, default_value = "react", value_parser = ["react", "plan_execute", "plan-execute"])]
    runtime: String,
    /// V2.3: answer to a pending `AgentStopReason::AwaitingInput` question
    /// (the checkpoint's `pending_question`). Required when the
    /// checkpoint is paused on a question, and rejected otherwise. When
    /// omitted on a paused checkpoint, reads one line from stdin.
    #[arg(long)]
    answer: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
  },
  /// List persisted Harness session logs
  List {
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
  },
  /// Inspect a single persisted Harness session log
  Inspect {
    /// Session id to inspect
    session_id: String,
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json", "json-envelope"])]
    output: String,
  },
  /// Time-paced re-stream of a persisted Harness session log
  /// (P10.10.2).
  ///
  /// Unlike `resume` (dump-all-at-once), `replay` sleeps between
  /// events based on their original `ts` deltas so an operator
  /// can watch a long-finished session "happen" in real time or
  /// accelerated time. Useful for spotting tool calls that fired
  /// right before a stall, or for debugging the *pacing* of a
  /// long-running multi-hour session after the fact.
  Replay {
    /// Session id to replay
    session_id: String,
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Replay speed: `1x` real-time, `2x` / `0.5x` / etc. for
    /// scaled timing, or `inf` / `instant` for no sleeps.
    /// Bare integers (e.g. `2`) are rejected because the
    /// `x` suffix is the only disambiguation between speed
    /// multiplier and seconds.
    #[arg(long, default_value = "1x")]
    speed: String,
    /// Start from this seq (inclusive). Earlier events are
    /// silently skipped + their inter-event sleeps collapse.
    #[arg(long)]
    from_seq: Option<u64>,
    /// Stop at this seq (inclusive).
    #[arg(long)]
    to_seq: Option<u64>,
    /// Only show events of this `kind` (repeatable for OR
    /// semantics). Names match the `kind` discriminator in the
    /// JSONL wire shape (`session_started`, `step_started`,
    /// `tool_call_requested`, `approval_requested`,
    /// `approval_decided`, `tool_call_completed`,
    /// `background_task_updated`, `memory_summary_added`,
    /// `stopped`).
    #[arg(long = "filter-kind", value_name = "KIND")]
    filter_kinds: Vec<String>,
    /// Output format: `text` (one human-readable line per event)
    /// or `stream-json` (one JSON event per line, JSONL —
    /// pipeable to `jq -c`). `json` / `json-envelope` are
    /// rejected because replay is open-ended; use `harness
    /// resume` for the bounded shape.
    #[arg(long, default_value = "text", value_parser = ["text", "stream-json"])]
    output: String,
  },
}

pub async fn dispatch(args: HarnessArgs) -> anyhow::Result<()> {
  match args.command {
    HarnessCommands::Run {
      input,
      skill,
      model,
      session,
      workspace,
      profile,
      approve,
      runtime,
      output,
      run_dir,
      max_steps,
      max_tool_calls,
      timeout_ms,
      context_budget,
      token_budget,
      cost_limit_usd,
      context_refresh,
      no_default_context,
    } => {
      run::execute(
        input,
        skill,
        model,
        session,
        workspace,
        profile,
        approve,
        runtime,
        output,
        run_dir,
        max_steps,
        max_tool_calls,
        timeout_ms,
        context_budget,
        token_budget,
        cost_limit_usd,
        context_refresh,
        no_default_context,
      )
      .await
    }
    HarnessCommands::RunFlow {
      workflow_file,
      model,
      input,
      profile,
      output,
      workspace,
      run_dir,
      timeout_ms,
      session,
      max_concurrency,
    } => {
      run_flow::execute(
        workflow_file,
        model,
        input,
        profile,
        output,
        workspace,
        run_dir,
        timeout_ms,
        session,
        max_concurrency,
      )
      .await
    }
    HarnessCommands::Chat {
      skill,
      model,
      session,
      workspace,
      profile,
      approve,
      runtime,
      run_dir,
      context_budget,
      token_budget,
      cost_limit_usd,
      context_refresh,
      max_steps,
      no_default_context,
    } => {
      chat::execute(
        skill,
        model,
        session,
        workspace,
        profile,
        approve,
        runtime,
        run_dir,
        context_budget,
        token_budget,
        cost_limit_usd,
        context_refresh,
        max_steps,
        no_default_context,
      )
      .await
    }
    HarnessCommands::Resume {
      session_id,
      run_dir,
      output,
    } => resume::execute(session_id, run_dir, output).await,
    HarnessCommands::ResumeLoop {
      session_id,
      skill,
      model,
      workspace,
      run_dir,
      runtime,
      answer,
      output,
    } => {
      resume_loop::execute(
        session_id, skill, model, workspace, run_dir, runtime, answer, output,
      )
      .await
    }
    HarnessCommands::List { run_dir, output } => list::execute(run_dir, output).await,
    HarnessCommands::Inspect {
      session_id,
      run_dir,
      output,
    } => inspect::execute(session_id, run_dir, output).await,
    HarnessCommands::Replay {
      session_id,
      run_dir,
      speed,
      from_seq,
      to_seq,
      filter_kinds,
      output,
    } => {
      replay::execute(
        session_id,
        run_dir,
        speed,
        from_seq,
        to_seq,
        filter_kinds,
        output,
      )
      .await
    }
  }
}
