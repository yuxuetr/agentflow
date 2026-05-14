//! `agentflow workflow resume-plan <run-id>` — inspect the resume
//! plan derived from a persisted checkpoint without executing the
//! workflow.
//!
//! The plan lists every unresolved tool call that the checkpoint
//! preserved, classifies each by `ToolIdempotency`, and surfaces the
//! decision the planner would take (`replay` / `skip` /
//! `requires_manual`) with an operator-readable reason. See `P1.7` in
//! `TODOs.md` for the rationale.

use std::path::PathBuf;

use anyhow::{Context, Result};

use agentflow_core::checkpoint::{CheckpointConfig, CheckpointManager};
use agentflow_core::{ResumeDecision, ResumePlan, ResumePlanOptions, build_resume_plan};

pub async fn execute(
  run_id: String,
  checkpoint_dir: Option<String>,
  force_replay: bool,
  format: String,
) -> Result<()> {
  let format = OutputFormat::parse(&format)?;
  let mut config = CheckpointConfig::default();
  if let Some(dir) = checkpoint_dir {
    config = config.with_checkpoint_dir(PathBuf::from(dir));
  }
  let manager = CheckpointManager::new(config).context("failed to open checkpoint manager")?;
  let checkpoint = manager
    .load_latest_checkpoint(&run_id)
    .await
    .with_context(|| format!("failed to load checkpoint for run '{run_id}'"))?
    .with_context(|| format!("no checkpoint found for run '{run_id}'"))?;
  let plan = build_resume_plan(&checkpoint, &ResumePlanOptions { force_replay })
    .context("failed to build resume plan")?;

  match format {
    OutputFormat::Text => print_text(&plan),
    OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&plan)?),
  }

  if !plan.summary.can_auto_resume() && !force_replay {
    eprintln!();
    eprintln!(
      "ℹ️  {} tool call(s) require manual recovery. Re-run with `--force-replay` only after \
       confirming each non-idempotent call is safe to repeat.",
      plan.summary.requires_manual
    );
  }
  Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
  Text,
  Json,
}

impl OutputFormat {
  fn parse(value: &str) -> Result<Self> {
    match value {
      "text" => Ok(Self::Text),
      "json" => Ok(Self::Json),
      other => anyhow::bail!("unsupported --format '{other}', expected text | json"),
    }
  }
}

fn print_text(plan: &ResumePlan) {
  println!("Resume plan for run: {}", plan.workflow_id);
  println!(
    "  status: {:?}   last_completed: {}   schema: v{}",
    plan.status, plan.last_completed_node, plan.schema_version
  );
  let summary = &plan.summary;
  println!(
    "  summary: total={} replay={} skip={} requires_manual={} can_auto_resume={}",
    summary.total,
    summary.to_replay,
    summary.to_skip,
    summary.requires_manual,
    summary.can_auto_resume()
  );
  if plan.tool_calls.is_empty() {
    println!("  no unresolved tool calls — checkpoint can resume cleanly.");
    return;
  }
  println!();
  println!(
    "{:<24} {:<22} {:<14} {:<16} REASON",
    "NODE", "TOOL", "STEP/IDEMP", "DECISION"
  );
  for call in &plan.tool_calls {
    let idempotency_label = format!("{}/{}", call.step_index, call.idempotency.as_str());
    let decision_label = match call.decision {
      ResumeDecision::Replay => "replay",
      ResumeDecision::Skip => "skip",
      ResumeDecision::RequiresManual => "requires_manual",
    };
    println!(
      "{:<24} {:<22} {:<14} {:<16} {}",
      truncate(&call.node_id, 24),
      truncate(&call.tool, 22),
      idempotency_label,
      decision_label,
      call.reason
    );
  }
}

fn truncate(value: &str, max: usize) -> String {
  if value.chars().count() <= max {
    value.to_owned()
  } else {
    let head: String = value.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
  }
}
