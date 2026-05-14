//! `agentflow harness inspect <session_id>` — summarise a session log.

use std::collections::BTreeMap;

use anyhow::{Context, Result};

use agentflow_harness::{HarnessEventBody, JsonlEventSink, default_session_dir};

use super::{OutputFormat, resolve_run_dir};

pub async fn execute(
  session_id: String,
  run_dir_override: Option<String>,
  output: String,
) -> Result<()> {
  let output = OutputFormat::parse(&output)?;
  let run_root = resolve_run_dir(run_dir_override)?;
  let session_dir = default_session_dir(&run_root);
  let sink = JsonlEventSink::new(session_dir.clone());
  let events = sink
    .read_session(&session_id)
    .await
    .with_context(|| format!("failed to read session '{session_id}' under {session_dir:?}"))?;

  if events.is_empty() {
    anyhow::bail!(
      "no events found for session '{session_id}' under {}",
      session_dir.display()
    );
  }

  let summary = summarise(&events);

  match output {
    OutputFormat::Text => {
      println!("Session: {session_id}");
      if let Some(meta) = summary.session_metadata.as_ref() {
        println!(
          "  runtime: {}   model: {}   profile: {}   workspace: {}",
          meta.runtime, meta.model, meta.profile, meta.workspace_root
        );
        if !meta.skills.is_empty() {
          println!("  skills: {}", meta.skills.join(", "));
        }
      }
      println!("  events: {}", summary.event_count);
      println!("  by kind:");
      for (kind, count) in &summary.counts_by_kind {
        println!("    {kind}: {count}");
      }
      if let Some(stop) = summary.stop_reason.as_deref() {
        println!("  stopped: {stop}");
      }
      if let Some(answer) = summary.final_answer.as_deref() {
        println!("  final answer: {answer}");
      }
    }
    OutputFormat::Json | OutputFormat::StreamJson => {
      println!("{}", serde_json::to_string_pretty(&summary)?);
    }
  }
  Ok(())
}

#[derive(Debug, serde::Serialize)]
struct Summary {
  session_id: String,
  event_count: usize,
  counts_by_kind: BTreeMap<String, usize>,
  #[serde(skip_serializing_if = "Option::is_none")]
  session_metadata: Option<SessionMetadata>,
  #[serde(skip_serializing_if = "Option::is_none")]
  stop_reason: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  final_answer: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct SessionMetadata {
  runtime: String,
  profile: String,
  model: String,
  workspace_root: String,
  skills: Vec<String>,
}

fn summarise(events: &[agentflow_harness::HarnessEvent]) -> Summary {
  let session_id = events
    .first()
    .map(|e| e.session_id.clone())
    .unwrap_or_default();
  let mut counts: BTreeMap<String, usize> = BTreeMap::new();
  let mut session_metadata = None;
  let mut stop_reason = None;
  let mut final_answer = None;
  for event in events {
    let kind = match &event.body {
      HarnessEventBody::SessionStarted(payload) => {
        session_metadata = Some(SessionMetadata {
          runtime: payload.runtime.as_str().to_owned(),
          profile: payload.profile.as_str().to_owned(),
          model: payload.model.clone(),
          workspace_root: payload.workspace_root.clone(),
          skills: payload.skills.clone(),
        });
        "session_started"
      }
      HarnessEventBody::StepStarted(_) => "step_started",
      HarnessEventBody::ToolCallRequested(_) => "tool_call_requested",
      HarnessEventBody::ApprovalRequested(_) => "approval_requested",
      HarnessEventBody::ApprovalDecided(_) => "approval_decided",
      HarnessEventBody::ToolCallCompleted(_) => "tool_call_completed",
      HarnessEventBody::BackgroundTaskUpdated(_) => "background_task_updated",
      HarnessEventBody::MemorySummaryAdded(_) => "memory_summary_added",
      HarnessEventBody::Stopped(payload) => {
        stop_reason = Some(format!("{:?}", payload.reason));
        final_answer = payload.final_answer.clone();
        "stopped"
      }
    };
    *counts.entry(kind.to_owned()).or_default() += 1;
  }
  Summary {
    session_id,
    event_count: events.len(),
    counts_by_kind: counts,
    session_metadata,
    stop_reason,
    final_answer,
  }
}
