//! `agentflow harness resume <session_id>` — re-stream a persisted
//! session log.
//!
//! Phase H1 implements **replay-only resume**: the JSONL session log
//! is read back and presented in the chosen output format. Full memory
//! rehydration (re-attaching the agent's conversation history so the
//! model can continue the dialog) is tracked as a Phase H2 follow-up
//! because it requires either a persistent `MemoryStore` or a memory
//! snapshot recorded alongside the events.

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

  match output {
    OutputFormat::Text => {
      println!("Session: {session_id}");
      println!("Stored events: {}", events.len());
      for event in &events {
        let line = format_event_line(event);
        println!("  {line}");
      }
    }
    OutputFormat::Json => {
      let payload = serde_json::json!({
        "session_id": session_id,
        "event_count": events.len(),
        "events": events,
      });
      println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    OutputFormat::StreamJson => {
      for event in &events {
        println!("{}", serde_json::to_string(event)?);
      }
      let payload = serde_json::json!({
        "type": "harness_resume_summary",
        "session_id": session_id,
        "event_count": events.len(),
      });
      println!("{}", serde_json::to_string(&payload)?);
    }
    OutputFormat::JsonEnvelope => {
      // P3.3 migration: wrap the same `{session_id, event_count,
      // events}` body the `json` mode emits. `stream-json` keeps
      // its per-line raw event format because the envelope would
      // defeat stream framing.
      let payload = serde_json::json!({
        "session_id": session_id,
        "event_count": events.len(),
        "events": events,
      });
      let envelope = crate::json_envelope::CliJsonEnvelope::ok("harness resume", &payload);
      println!("{}", serde_json::to_string_pretty(&envelope)?);
    }
  }

  Ok(())
}

fn format_event_line(event: &agentflow_harness::HarnessEvent) -> String {
  let kind = match &event.body {
    HarnessEventBody::SessionStarted(payload) => format!(
      "session_started runtime={} model={} ctx={}",
      payload.runtime.as_str(),
      payload.model,
      payload.context_item_count
    ),
    HarnessEventBody::StepStarted(payload) => {
      format!("step_started #{} {}", payload.step_index, payload.step_type)
    }
    HarnessEventBody::ToolCallRequested(payload) => {
      format!(
        "tool_call_requested {} (step={})",
        payload.tool, payload.step_index
      )
    }
    HarnessEventBody::ApprovalRequested(payload) => {
      format!(
        "approval_requested tool={} risk={:?}",
        payload.request.tool, payload.request.risk
      )
    }
    HarnessEventBody::ApprovalDecided(payload) => {
      format!("approval_decided request={}", payload.decision.request_id)
    }
    HarnessEventBody::ToolCallCompleted(payload) => format!(
      "tool_call_completed {} err={} dur={}ms",
      payload.tool, payload.is_error, payload.duration_ms
    ),
    HarnessEventBody::BackgroundTaskUpdated(payload) => format!(
      "background_task_updated task={} status={:?}",
      payload.task_id, payload.status
    ),
    HarnessEventBody::MemorySummaryAdded(payload) => {
      format!("memory_summary_added layer={}", payload.layer)
    }
    HarnessEventBody::Stopped(payload) => format!(
      "stopped reason={:?} err={:?}",
      payload.reason, payload.error
    ),
  };
  format!(
    "[{:04}] {} {}",
    event.seq,
    event.ts.format("%H:%M:%S"),
    kind
  )
}
