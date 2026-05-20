//! `agentflow harness replay <session_id>` — time-paced re-stream of a
//! persisted session log (P10.10.2).
//!
//! ## How `replay` differs from `resume`
//!
//! - [`crate::commands::harness::resume`] dumps the entire JSONL log at
//!   once. Fastest path; ideal for `| jq` / scripted post-mortems.
//! - `replay` re-streams events with their original timing (or a speed
//!   multiplier) so an operator can watch a long-finished session
//!   "happen" in real time / accelerated time. Useful for debugging
//!   long Harness sessions where the *order and pacing* of events
//!   carries diagnostic value (e.g. spotting a tool call that fired
//!   right before a long stall).
//!
//! ## Filters
//!
//! `--from-seq` / `--to-seq` clip the visible window without
//! breaking the ts deltas (the sleep between a filtered-out event
//! and the next visible one collapses; the runtime never sleeps
//! through events it didn't intend to show). `--filter-kind` is
//! repeatable for an additive include-list.

use std::time::Duration;

use agentflow_harness::{HarnessEvent, HarnessEventBody, JsonlEventSink, default_session_dir};
use anyhow::{Context, Result};

use super::{OutputFormat, resolve_run_dir};

/// Pacing mode for the replay loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedMode {
  /// Multiply the original ts deltas by `1.0 / multiplier`.
  /// `multiplier = 1.0` → real time; `2.0` → 2× faster; `0.5` →
  /// half speed.
  Realtime(f64),
  /// No sleeps between events — equivalent to `resume` but routed
  /// through the per-event formatter. Useful when piping the
  /// replay stream into `jq` / a test harness.
  Instant,
}

/// Parse the `--speed` flag.
///
/// Accepted forms:
/// - `1x` / `2x` / `0.5x` — real-time multiplier (positive finite).
/// - `inf` / `instant` — case-insensitive; no sleeps.
///
/// Rejected (with a clear message):
/// - bare number without `x` suffix (would silently degrade if some
///   future locale dropped the suffix).
/// - non-positive / NaN / non-finite multipliers.
pub fn parse_speed(raw: &str) -> Result<SpeedMode> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    anyhow::bail!("--speed must not be empty");
  }
  let lower = trimmed.to_lowercase();
  if lower == "inf" || lower == "instant" {
    return Ok(SpeedMode::Instant);
  }
  let multiplier_str = lower.strip_suffix('x').ok_or_else(|| {
    anyhow::anyhow!(
      "--speed '{raw}' must end in 'x' (e.g. '1x', '2x', '0.5x') or be 'inf' / 'instant'"
    )
  })?;
  let multiplier: f64 = multiplier_str
    .parse()
    .with_context(|| format!("--speed '{raw}': could not parse '{multiplier_str}' as a number"))?;
  if !multiplier.is_finite() {
    anyhow::bail!("--speed '{raw}' must be a finite number (use 'inf' for no-sleep mode)");
  }
  if multiplier <= 0.0 {
    anyhow::bail!("--speed '{raw}' must be strictly positive (got {multiplier})");
  }
  Ok(SpeedMode::Realtime(multiplier))
}

/// Stable string label per [`HarnessEventBody`] variant, matching
/// the snake_case `kind` discriminator in the wire serialisation.
/// Centralised so the filter logic + the text formatter agree on
/// names without each having to walk the enum independently.
fn event_kind_str(body: &HarnessEventBody) -> &'static str {
  match body {
    HarnessEventBody::SessionStarted(_) => "session_started",
    HarnessEventBody::StepStarted(_) => "step_started",
    HarnessEventBody::ToolCallRequested(_) => "tool_call_requested",
    HarnessEventBody::ApprovalRequested(_) => "approval_requested",
    HarnessEventBody::ApprovalDecided(_) => "approval_decided",
    HarnessEventBody::ToolCallCompleted(_) => "tool_call_completed",
    HarnessEventBody::BackgroundTaskUpdated(_) => "background_task_updated",
    HarnessEventBody::MemorySummaryAdded(_) => "memory_summary_added",
    HarnessEventBody::Stopped(_) => "stopped",
  }
}

/// Apply seq + kind filters in one pass. Pulled out for unit
/// testability (the pure-logic core has no I/O dependencies).
pub fn apply_filters<'a>(
  events: &'a [HarnessEvent],
  from_seq: Option<u64>,
  to_seq: Option<u64>,
  kinds: &[String],
) -> Vec<&'a HarnessEvent> {
  events
    .iter()
    .filter(|e| from_seq.is_none_or(|min| e.seq >= min))
    .filter(|e| to_seq.is_none_or(|max| e.seq <= max))
    .filter(|e| {
      if kinds.is_empty() {
        true
      } else {
        kinds.iter().any(|k| k == event_kind_str(&e.body))
      }
    })
    .collect()
}

/// Compute the sleep between two adjacent events under the given
/// pacing mode. Returns `Duration::ZERO` for non-positive deltas
/// (clock skew, same-ms events) and for `Instant` mode.
///
/// Capped at 1 hour so a session that paused overnight doesn't
/// hang the replay — operators wanting the real overnight gap can
/// rerun with `--speed inf` and reconstruct timing from the ts
/// field in `--output stream-json` mode.
fn sleep_between(prev: &HarnessEvent, next: &HarnessEvent, mode: SpeedMode) -> Duration {
  let SpeedMode::Realtime(multiplier) = mode else {
    return Duration::ZERO;
  };
  let delta_ms = (next.ts - prev.ts).num_milliseconds();
  if delta_ms <= 0 {
    return Duration::ZERO;
  }
  let scaled = (delta_ms as f64) / multiplier;
  if !scaled.is_finite() || scaled <= 0.0 {
    return Duration::ZERO;
  }
  const MAX_SLEEP: Duration = Duration::from_secs(3_600);
  let dur = Duration::from_millis(scaled as u64);
  if dur > MAX_SLEEP { MAX_SLEEP } else { dur }
}

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  session_id: String,
  run_dir_override: Option<String>,
  speed: String,
  from_seq: Option<u64>,
  to_seq: Option<u64>,
  filter_kinds: Vec<String>,
  output: String,
) -> Result<()> {
  let output = OutputFormat::parse(&output)?;
  let speed_mode = parse_speed(&speed)?;
  // `json` and `json-envelope` are bounded by design — they wrap a
  // single payload. Combining them with replay doesn't make sense
  // because replay's whole point is streaming. Reject up front
  // with a message naming the right alternative, mirroring the
  // `workflow logs --follow + --format json-envelope` rejection in
  // P10.11.1.
  if matches!(output, OutputFormat::Json | OutputFormat::JsonEnvelope) {
    anyhow::bail!(
      "--output {} is incompatible with `harness replay`: the replay produces an open-ended \
       event stream, but {} wraps a single bounded payload. Use --output text (per-event \
       human-readable lines) or --output stream-json (one JSON event per line, JSONL).",
      if matches!(output, OutputFormat::Json) {
        "json"
      } else {
        "json-envelope"
      },
      if matches!(output, OutputFormat::Json) {
        "json"
      } else {
        "json-envelope"
      },
    );
  }

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

  let filtered = apply_filters(&events, from_seq, to_seq, &filter_kinds);
  if filtered.is_empty() {
    anyhow::bail!(
      "all {} events filtered out — check --from-seq / --to-seq / --filter-kind",
      events.len()
    );
  }

  // Header goes to stderr in stream-json mode so stdout stays a
  // pure JSONL stream that `jq` / downstream tooling can parse.
  if matches!(output, OutputFormat::StreamJson) {
    eprintln!(
      "▶ replaying session {session_id} ({} of {} events, speed={speed})",
      filtered.len(),
      events.len()
    );
  } else {
    println!(
      "▶ Session: {session_id}  ({} of {} events, speed={speed})",
      filtered.len(),
      events.len()
    );
  }

  let mut prev: Option<&HarnessEvent> = None;
  for event in filtered {
    if let Some(p) = prev {
      let sleep = sleep_between(p, event, speed_mode);
      if !sleep.is_zero() {
        tokio::time::sleep(sleep).await;
      }
    }
    match output {
      OutputFormat::Text => println!("  {}", format_event_line(event)),
      OutputFormat::StreamJson => println!("{}", serde_json::to_string(event)?),
      OutputFormat::Json | OutputFormat::JsonEnvelope => {
        unreachable!("bounded output formats rejected at the top of execute()")
      }
    }
    prev = Some(event);
  }
  Ok(())
}

/// Single-line human-readable formatter. Mirrors
/// `harness::resume::format_event_line` shape so operators see the
/// same per-event rendering across both commands; centralising the
/// formatter is tracked as a follow-up.
fn format_event_line(event: &HarnessEvent) -> String {
  let kind = event_kind_str(&event.body);
  let body_summary = match &event.body {
    HarnessEventBody::SessionStarted(payload) => format!(
      "runtime={} model={} ctx={}",
      payload.runtime.as_str(),
      payload.model,
      payload.context_item_count
    ),
    HarnessEventBody::StepStarted(payload) => {
      format!("#{} {}", payload.step_index, payload.step_type)
    }
    HarnessEventBody::ToolCallRequested(payload) => {
      format!("{} (step={})", payload.tool, payload.step_index)
    }
    HarnessEventBody::ApprovalRequested(payload) => {
      format!(
        "tool={} risk={:?}",
        payload.request.tool, payload.request.risk
      )
    }
    HarnessEventBody::ApprovalDecided(payload) => {
      format!("request={}", payload.decision.request_id)
    }
    HarnessEventBody::ToolCallCompleted(payload) => format!(
      "{} err={} dur={}ms",
      payload.tool, payload.is_error, payload.duration_ms
    ),
    HarnessEventBody::BackgroundTaskUpdated(payload) => {
      format!("task={} status={:?}", payload.task_id, payload.status)
    }
    HarnessEventBody::MemorySummaryAdded(payload) => format!("layer={}", payload.layer),
    HarnessEventBody::Stopped(payload) => {
      format!("reason={:?} err={:?}", payload.reason, payload.error)
    }
  };
  format!(
    "[{:04}] {} {} {}",
    event.seq,
    event.ts.format("%H:%M:%S%.3f"),
    kind,
    body_summary
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{TimeZone, Utc};

  // ── parse_speed ────────────────────────────────────────────────────

  #[test]
  fn parse_speed_accepts_realtime_multipliers() {
    // Pin each canonical form: integer, fractional, sub-1 for
    // slow-motion. A regression that drops one of these breaks
    // the most common operator inputs.
    assert!(matches!(
      parse_speed("1x").unwrap(),
      SpeedMode::Realtime(m) if (m - 1.0).abs() < 1e-9
    ));
    assert!(matches!(
      parse_speed("2x").unwrap(),
      SpeedMode::Realtime(m) if (m - 2.0).abs() < 1e-9
    ));
    assert!(matches!(
      parse_speed("0.5x").unwrap(),
      SpeedMode::Realtime(m) if (m - 0.5).abs() < 1e-9
    ));
    // Whitespace is trimmed — operators copy-pasting from shells
    // with trailing newlines shouldn't trip.
    assert!(matches!(
      parse_speed(" 3x ").unwrap(),
      SpeedMode::Realtime(_)
    ));
  }

  #[test]
  fn parse_speed_accepts_instant_aliases() {
    assert_eq!(parse_speed("inf").unwrap(), SpeedMode::Instant);
    assert_eq!(parse_speed("instant").unwrap(), SpeedMode::Instant);
    // Case-insensitivity matters — operators type INSTANT in
    // shell history sometimes.
    assert_eq!(parse_speed("Instant").unwrap(), SpeedMode::Instant);
    assert_eq!(parse_speed("INF").unwrap(), SpeedMode::Instant);
  }

  #[test]
  fn parse_speed_rejects_bare_integer_without_x() {
    // `--speed 2` is ambiguous — does the user mean 2x or 2
    // seconds? The CLI rejects it so the operator picks one.
    let err = parse_speed("2").expect_err("bare integer must err");
    let msg = err.to_string();
    assert!(msg.contains("must end in 'x'"), "{msg}");
    assert!(msg.contains("'inf'") || msg.contains("'instant'"), "{msg}");
  }

  #[test]
  fn parse_speed_rejects_non_positive() {
    let err = parse_speed("0x").expect_err("zero must err");
    assert!(err.to_string().contains("strictly positive"), "{err}");
    let err = parse_speed("-1x").expect_err("negative must err");
    assert!(err.to_string().contains("strictly positive"), "{err}");
  }

  #[test]
  fn parse_speed_rejects_non_finite() {
    // `infx` is a parse-as-f64-as-INFINITY edge case. Pin
    // explicitly — the runtime would otherwise emit
    // Realtime(INFINITY) and the sleep loop would divide by it
    // (giving 0ms sleeps, which is exactly Instant). Surfacing
    // the rejection is the clearer UX.
    let err = parse_speed("infx").expect_err("non-finite must err");
    assert!(err.to_string().contains("finite"), "{err}");
  }

  #[test]
  fn parse_speed_rejects_garbage() {
    let err = parse_speed("abc").expect_err("garbage must err");
    // Garbage falls through to the parse-as-number path; the
    // error names the offending substring so the operator sees
    // exactly what failed.
    assert!(err.to_string().contains("'abc'"), "{err}");
  }

  // ── apply_filters ─────────────────────────────────────────────────

  fn sample_event(seq: u64, body: HarnessEventBody) -> HarnessEvent {
    HarnessEvent {
      seq,
      session_id: "s1".into(),
      ts: Utc.timestamp_opt(seq as i64, 0).single().unwrap(),
      body,
    }
  }

  fn step_started_event(seq: u64, idx: usize) -> HarnessEvent {
    use agentflow_harness::event::StepStartedPayload;
    sample_event(
      seq,
      HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: idx,
        step_type: "plan".into(),
      }),
    )
  }

  fn stopped_event(seq: u64) -> HarnessEvent {
    use agentflow_harness::event::{StopReason, StoppedPayload};
    sample_event(
      seq,
      HarnessEventBody::Stopped(StoppedPayload {
        reason: StopReason::Completed,
        final_answer: None,
        error: None,
      }),
    )
  }

  #[test]
  fn apply_filters_no_filters_returns_everything() {
    let events = vec![step_started_event(0, 0), stopped_event(1)];
    let out = apply_filters(&events, None, None, &[]);
    assert_eq!(out.len(), 2);
  }

  #[test]
  fn apply_filters_from_seq_inclusive() {
    let events = vec![
      step_started_event(0, 0),
      step_started_event(1, 1),
      stopped_event(2),
    ];
    let out = apply_filters(&events, Some(1), None, &[]);
    assert_eq!(out.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2]);
  }

  #[test]
  fn apply_filters_to_seq_inclusive() {
    let events = vec![
      step_started_event(0, 0),
      step_started_event(1, 1),
      stopped_event(2),
    ];
    let out = apply_filters(&events, None, Some(1), &[]);
    assert_eq!(out.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![0, 1]);
  }

  #[test]
  fn apply_filters_kind_only_includes_matching() {
    let events = vec![step_started_event(0, 0), stopped_event(1)];
    let out = apply_filters(&events, None, None, &["stopped".to_string()]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].seq, 1);
  }

  #[test]
  fn apply_filters_kind_additive_include_list() {
    // Multiple --filter-kind values combine as OR. Pin so a
    // future refactor that flipped to AND would surface.
    let events = vec![step_started_event(0, 0), stopped_event(1)];
    let out = apply_filters(
      &events,
      None,
      None,
      &["step_started".to_string(), "stopped".to_string()],
    );
    assert_eq!(out.len(), 2);
  }

  // ── sleep_between ─────────────────────────────────────────────────

  #[test]
  fn sleep_between_returns_zero_for_instant_mode() {
    let a = step_started_event(0, 0);
    let b = step_started_event(1, 1); // 1s later
    assert_eq!(sleep_between(&a, &b, SpeedMode::Instant), Duration::ZERO);
  }

  #[test]
  fn sleep_between_scales_with_realtime_multiplier() {
    let a = step_started_event(0, 0);
    let b = step_started_event(2, 1); // 2s later in ts
    // At 1x: 2000 ms; at 2x: 1000 ms; at 0.5x: 4000 ms. Pin all
    // three so a sign-flip / inversion regression surfaces.
    assert_eq!(
      sleep_between(&a, &b, SpeedMode::Realtime(1.0)),
      Duration::from_millis(2000)
    );
    assert_eq!(
      sleep_between(&a, &b, SpeedMode::Realtime(2.0)),
      Duration::from_millis(1000)
    );
    assert_eq!(
      sleep_between(&a, &b, SpeedMode::Realtime(0.5)),
      Duration::from_millis(4000)
    );
  }

  #[test]
  fn sleep_between_returns_zero_for_backwards_ts() {
    // Clock skew or out-of-order events: prev.ts > next.ts. The
    // function must NOT panic and must NOT sleep — the only
    // sensible behaviour is to flow through to the next event.
    let a = step_started_event(5, 0);
    let b = step_started_event(1, 1); // earlier ts
    assert_eq!(
      sleep_between(&a, &b, SpeedMode::Realtime(1.0)),
      Duration::ZERO
    );
  }

  #[test]
  fn sleep_between_caps_at_one_hour() {
    use agentflow_harness::event::StepStartedPayload;
    // 25-hour gap simulates a session that sat idle overnight.
    // The cap means the replay continues after 1 hour instead of
    // hanging — operators can rerun with --speed inf if they
    // really want the original timing.
    let a = HarnessEvent {
      seq: 0,
      session_id: "s1".into(),
      ts: Utc.timestamp_opt(0, 0).single().unwrap(),
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: 0,
        step_type: "plan".into(),
      }),
    };
    let b = HarnessEvent {
      seq: 1,
      session_id: "s1".into(),
      ts: Utc.timestamp_opt(25 * 3600, 0).single().unwrap(),
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: 1,
        step_type: "plan".into(),
      }),
    };
    let sleep = sleep_between(&a, &b, SpeedMode::Realtime(1.0));
    assert_eq!(sleep, Duration::from_secs(3600), "cap to 1 hour");
  }
}
