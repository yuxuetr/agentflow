//! `agentflow agent replay <current> --diff <baseline>` (P10.8.1).
//!
//! Pure file-to-file comparator for ReAct-style agent trace JSONL streams.
//! Each input file is a sequence of `agentflow_agents::AgentEvent` lines
//! (one event per line, snake_case-tagged JSON). The diff reduces the
//! event stream to the three operator-facing dimensions called out in
//! the TODO:
//!
//!   1. Tool-call order — `ToolCall` step kinds compared by index + tool name.
//!   2. Stop reason     — the terminal `RunStopped` event's variant.
//!   3. Token usage     — `LlmCallCompleted` per-step prompt/completion counts.
//!
//! Divergence in (1) or (2) exits non-zero. Token deltas in (3) are
//! reported but don't fail the gate by default — LLM token accounting
//! jitters by a few tokens between identical requests. Pass
//! `--strict-tokens` to make any non-zero delta fail too.
//!
//! Output formats:
//!   - `text` (default): human-readable per-event lines, summary at the end.
//!   - `stream-json`: one JSON object per divergence/variance on stdout.
//!   - `json-envelope`: canonical `agentflow.cli/1` envelope wrapping the
//!     full structured report.
//!
//! What this is *not*: this command does NOT run a fresh ReAct loop. The
//! user is responsible for producing both JSONL files (e.g. by running
//! ReAct twice and capturing the AgentEvent stream each time). A
//! "run-then-diff" wrapper can land later as a separate subcommand once
//! `agentflow agent run` exists.

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::path::{Path, PathBuf};

use agentflow_agents::runtime::{AgentEvent, AgentStep, AgentStepKind, AgentStopReason};

/// Output format. Mirrors the convention used by `harness replay` /
/// `workflow logs` so an operator who's used one knows what to expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
  Text,
  StreamJson,
  JsonEnvelope,
}

impl OutputFormat {
  fn parse(raw: &str) -> Result<Self> {
    match raw {
      "text" => Ok(Self::Text),
      "stream-json" => Ok(Self::StreamJson),
      "json-envelope" => Ok(Self::JsonEnvelope),
      other => Err(anyhow!(
        "unknown --format '{other}' (expected text | stream-json | json-envelope)"
      )),
    }
  }
}

/// Reduced view of a trace: the three dimensions the diff cares about.
/// Intentionally cheap to copy/compare and entirely free of
/// timestamps — wall-clock varies between runs and would be pure noise.
#[derive(Debug, Clone, Default)]
pub struct ParsedTrace {
  /// Steps in event order, derived from `step_completed` events.
  pub steps: Vec<AgentStep>,
  /// Terminal reason, derived from the last `run_stopped` event.
  /// `None` when the trace was captured mid-run.
  pub stop_reason: Option<AgentStopReason>,
  /// Per-step LLM token usage, keyed by `step_index`. A run can emit
  /// multiple `llm_call_completed` events for the same step (e.g. tool-
  /// use retry); we collapse to the last sample to match the
  /// dashboard convention.
  pub llm_calls: std::collections::BTreeMap<usize, LlmCallSummary>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct LlmCallSummary {
  pub prompt_tokens: Option<u32>,
  pub completion_tokens: Option<u32>,
}

/// One unrecoverable divergence between baseline and current. Any
/// non-empty list fails the gate.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Divergence {
  /// The two traces have different step counts. Indexes past the
  /// shorter one are reported in this single rolled-up entry instead
  /// of one StepKindMismatch per "missing" step.
  StepCount {
    baseline_count: usize,
    current_count: usize,
  },
  /// At step `index`, the two traces emitted different
  /// `AgentStepKind` discriminants (e.g. baseline `tool_call` vs
  /// current `plan`).
  StepKindMismatch {
    index: usize,
    baseline_kind: &'static str,
    current_kind: &'static str,
  },
  /// Both steps are `tool_call`, but the `tool` name differs.
  ToolNameMismatch {
    index: usize,
    baseline_tool: String,
    current_tool: String,
  },
  /// The `run_stopped` reasons disagree (or one trace is missing
  /// its terminal event when the other has one).
  StopReasonMismatch {
    baseline: Option<String>,
    current: Option<String>,
  },
  /// Token counts differ AND `--strict-tokens` is set. Without
  /// strict-tokens, this same delta is reported as a `Variance`
  /// instead.
  TokenDelta {
    index: usize,
    prompt_delta: i64,
    completion_delta: i64,
  },
}

/// Soft-flag observation: surfaced but doesn't fail the gate by default.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Variance {
  /// Per-step LLM token delta (current - baseline). Reported when at
  /// least one of the two sides has a recorded count and they differ.
  TokenDelta {
    index: usize,
    prompt_delta: i64,
    completion_delta: i64,
  },
}

/// Full structured diff report. Used as both the in-memory result type
/// and the wire shape for `--format json-envelope`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiffReport {
  pub baseline_steps: usize,
  pub current_steps: usize,
  pub divergences: Vec<Divergence>,
  pub variances: Vec<Variance>,
}

impl DiffReport {
  /// Did we find anything that fails the gate?
  pub fn has_divergence(&self) -> bool {
    !self.divergences.is_empty()
  }
}

/// Public entry point used by the CLI dispatch in `main.rs`.
pub async fn execute(
  current_path: PathBuf,
  baseline_path: PathBuf,
  format: String,
  strict_tokens: bool,
) -> Result<()> {
  let format = OutputFormat::parse(&format)?;

  let baseline = read_trace_file(&baseline_path)
    .with_context(|| format!("reading baseline '{}'", baseline_path.display()))?;
  let current = read_trace_file(&current_path)
    .with_context(|| format!("reading current '{}'", current_path.display()))?;

  let report = compute_diff(&baseline, &current, strict_tokens);
  render_report(&report, format, &baseline_path, &current_path)?;

  if report.has_divergence() {
    bail!(
      "{} divergence(s) found between baseline and current trace",
      report.divergences.len()
    );
  }
  Ok(())
}

/// Read a JSONL file (one `AgentEvent` per line, blank lines tolerated)
/// and reduce it to a [`ParsedTrace`].
pub fn read_trace_file(path: &Path) -> Result<ParsedTrace> {
  let text = std::fs::read_to_string(path)
    .with_context(|| format!("failed to read '{}'", path.display()))?;
  parse_trace_jsonl(&text)
}

/// Lower-level: parse JSONL text without touching the filesystem.
/// Exposed for unit tests + the (future) stdin path.
pub fn parse_trace_jsonl(text: &str) -> Result<ParsedTrace> {
  let mut trace = ParsedTrace::default();
  for (line_no, raw) in text.lines().enumerate() {
    let line = raw.trim();
    if line.is_empty() {
      continue;
    }
    let event: AgentEvent = serde_json::from_str(line).with_context(|| {
      format!(
        "line {} is not a valid AgentEvent JSON object: {}",
        line_no + 1,
        // Truncate to keep error messages diff-friendly when a giant
        // tool result blob is the offender.
        line.chars().take(120).collect::<String>()
      )
    })?;
    apply_event(&mut trace, event);
  }
  Ok(trace)
}

fn apply_event(trace: &mut ParsedTrace, event: AgentEvent) {
  match event {
    AgentEvent::StepCompleted { step, .. } => {
      trace.steps.push(step);
    }
    AgentEvent::RunStopped { reason, .. } => {
      // Last `run_stopped` wins. A trace shouldn't contain more than
      // one, but tolerate it cheaply rather than failing the diff.
      trace.stop_reason = Some(reason);
    }
    AgentEvent::LlmCallCompleted {
      step_index,
      prompt_tokens,
      completion_tokens,
      ..
    } => {
      trace.llm_calls.insert(
        step_index,
        LlmCallSummary {
          prompt_tokens,
          completion_tokens,
        },
      );
    }
    // Every other event is structural (step_started, tool-call
    // policy decisions, etc.). The diff doesn't surface them today;
    // adding more dimensions is additive.
    _ => {}
  }
}

/// Pure comparator. Side-effect-free so the unit tests pin every
/// boundary without spawning the CLI binary.
pub fn compute_diff(
  baseline: &ParsedTrace,
  current: &ParsedTrace,
  strict_tokens: bool,
) -> DiffReport {
  let mut report = DiffReport {
    baseline_steps: baseline.steps.len(),
    current_steps: current.steps.len(),
    ..DiffReport::default()
  };

  // 1. Step count.
  if baseline.steps.len() != current.steps.len() {
    report.divergences.push(Divergence::StepCount {
      baseline_count: baseline.steps.len(),
      current_count: current.steps.len(),
    });
  }

  // 2. Step kind + tool-name comparison over the common prefix. We
  //    walk the *shorter* of the two so an extra trailing step doesn't
  //    avalanche into N per-index mismatches — the rolled-up
  //    StepCount above already records that.
  let common = baseline.steps.len().min(current.steps.len());
  for index in 0..common {
    let base = &baseline.steps[index].kind;
    let curr = &current.steps[index].kind;
    let base_kind = step_kind_discriminant(base);
    let curr_kind = step_kind_discriminant(curr);
    if base_kind != curr_kind {
      report.divergences.push(Divergence::StepKindMismatch {
        index,
        baseline_kind: base_kind,
        current_kind: curr_kind,
      });
      continue;
    }
    // Same kind; check tool name when both are ToolCall (tool params
    // are richer signals but deliberately out of scope — LLM-driven
    // params vary noisily, and step-order divergence usually
    // dominates anyway).
    if let (
      AgentStepKind::ToolCall { tool: bt, .. },
      AgentStepKind::ToolCall { tool: ct, .. },
    ) = (base, curr)
      && bt != ct
    {
      report.divergences.push(Divergence::ToolNameMismatch {
        index,
        baseline_tool: bt.clone(),
        current_tool: ct.clone(),
      });
    }
  }

  // 3. Stop reason.
  let base_reason = baseline.stop_reason.as_ref().map(stop_reason_label);
  let curr_reason = current.stop_reason.as_ref().map(stop_reason_label);
  if base_reason != curr_reason {
    report.divergences.push(Divergence::StopReasonMismatch {
      baseline: base_reason,
      current: curr_reason,
    });
  }

  // 4. Token usage. Iterate the union of step indices on either side.
  //    A step that appears in only one side gets full-credit "delta"
  //    against zero — operationally this is "current emitted a new
  //    LLM call at step N" or vice versa.
  let mut indices: std::collections::BTreeSet<usize> =
    baseline.llm_calls.keys().copied().collect();
  indices.extend(current.llm_calls.keys().copied());
  for index in indices {
    let base = baseline.llm_calls.get(&index).copied().unwrap_or_default();
    let curr = current.llm_calls.get(&index).copied().unwrap_or_default();
    let prompt_delta = token_delta(curr.prompt_tokens, base.prompt_tokens);
    let completion_delta = token_delta(curr.completion_tokens, base.completion_tokens);
    if prompt_delta == 0 && completion_delta == 0 {
      continue;
    }
    if strict_tokens {
      report.divergences.push(Divergence::TokenDelta {
        index,
        prompt_delta,
        completion_delta,
      });
    } else {
      report.variances.push(Variance::TokenDelta {
        index,
        prompt_delta,
        completion_delta,
      });
    }
  }

  report
}

/// Subtract `Option<u32>` pairs into a signed delta, treating `None`
/// on either side as zero. Returns `i64` so a (rare) (u32::MAX - 0)
/// delta doesn't overflow.
fn token_delta(current: Option<u32>, baseline: Option<u32>) -> i64 {
  current.unwrap_or(0) as i64 - baseline.unwrap_or(0) as i64
}

/// Map an [`AgentStepKind`] to its snake_case discriminant string.
/// `&'static str` so the diff structs can pin them in their fields
/// without cloning per comparison.
pub fn step_kind_discriminant(kind: &AgentStepKind) -> &'static str {
  match kind {
    AgentStepKind::Observe { .. } => "observe",
    AgentStepKind::Plan { .. } => "plan",
    AgentStepKind::ToolCall { .. } => "tool_call",
    AgentStepKind::ToolResult { .. } => "tool_result",
    AgentStepKind::Reflect { .. } => "reflect",
    AgentStepKind::FinalAnswer { .. } => "final_answer",
    AgentStepKind::Handoff { .. } => "handoff",
    AgentStepKind::BlackboardOp { .. } => "blackboard_op",
    AgentStepKind::DebateProposal { .. } => "debate_proposal",
    AgentStepKind::DebateVerdict { .. } => "debate_verdict",
  }
}

/// Map an [`AgentStopReason`] to its public label. Same string the
/// JSON serialisation uses (the `reason` tag from the
/// `#[serde(tag = "reason")]` attribute on the enum).
pub fn stop_reason_label(reason: &AgentStopReason) -> String {
  match reason {
    AgentStopReason::FinalAnswer => "final_answer".to_string(),
    AgentStopReason::StopCondition { .. } => "stop_condition".to_string(),
    AgentStopReason::MaxSteps { .. } => "max_steps".to_string(),
    AgentStopReason::MaxToolCalls { .. } => "max_tool_calls".to_string(),
    AgentStopReason::Timeout { .. } => "timeout".to_string(),
    AgentStopReason::Cancelled { .. } => "cancelled".to_string(),
    AgentStopReason::TokenBudgetExceeded { .. } => "token_budget_exceeded".to_string(),
    AgentStopReason::CostLimitExceeded { .. } => "cost_limit_exceeded".to_string(),
    AgentStopReason::Error { .. } => "error".to_string(),
  }
}

fn render_report(
  report: &DiffReport,
  format: OutputFormat,
  baseline_path: &Path,
  current_path: &Path,
) -> Result<()> {
  match format {
    OutputFormat::Text => render_text(report, baseline_path, current_path),
    OutputFormat::StreamJson => render_stream_json(report),
    OutputFormat::JsonEnvelope => render_envelope(report, baseline_path, current_path),
  }
}

fn render_text(report: &DiffReport, baseline_path: &Path, current_path: &Path) -> Result<()> {
  println!(
    "agent replay --diff: baseline={} current={}",
    baseline_path.display(),
    current_path.display()
  );
  println!(
    "  steps: baseline={} current={}",
    report.baseline_steps, report.current_steps
  );

  for d in &report.divergences {
    println!("  ✗ {}", format_divergence_line(d));
  }
  for v in &report.variances {
    println!("  · {}", format_variance_line(v));
  }

  if report.divergences.is_empty() {
    println!(
      "  match: {} step(s) agree; {} variance(s) reported",
      report.baseline_steps,
      report.variances.len()
    );
  } else {
    println!(
      "  {} divergence(s); {} variance(s)",
      report.divergences.len(),
      report.variances.len()
    );
  }
  Ok(())
}

fn render_stream_json(report: &DiffReport) -> Result<()> {
  for d in &report.divergences {
    println!(
      "{}",
      serde_json::to_string(&serde_json::json!({
        "type": "divergence",
        "data": d,
      }))?
    );
  }
  for v in &report.variances {
    println!(
      "{}",
      serde_json::to_string(&serde_json::json!({
        "type": "variance",
        "data": v,
      }))?
    );
  }
  Ok(())
}

fn render_envelope(
  report: &DiffReport,
  baseline_path: &Path,
  current_path: &Path,
) -> Result<()> {
  let envelope = serde_json::json!({
    "version": "agentflow.cli/1",
    "command": "agent replay --diff",
    "result": {
      "baseline_path": baseline_path.display().to_string(),
      "current_path": current_path.display().to_string(),
      "baseline_steps": report.baseline_steps,
      "current_steps": report.current_steps,
      "divergences": report.divergences,
      "variances": report.variances,
    },
    "errors": Vec::<String>::new(),
  });
  println!("{}", serde_json::to_string_pretty(&envelope)?);
  Ok(())
}

fn format_divergence_line(d: &Divergence) -> String {
  match d {
    Divergence::StepCount {
      baseline_count,
      current_count,
    } => format!(
      "step count differs: baseline has {baseline_count}, current has {current_count}"
    ),
    Divergence::StepKindMismatch {
      index,
      baseline_kind,
      current_kind,
    } => format!("step {index} kind: baseline={baseline_kind}, current={current_kind}"),
    Divergence::ToolNameMismatch {
      index,
      baseline_tool,
      current_tool,
    } => format!(
      "step {index} tool name: baseline={baseline_tool}, current={current_tool}"
    ),
    Divergence::StopReasonMismatch { baseline, current } => format!(
      "stop reason: baseline={}, current={}",
      baseline.as_deref().unwrap_or("<none>"),
      current.as_deref().unwrap_or("<none>")
    ),
    Divergence::TokenDelta {
      index,
      prompt_delta,
      completion_delta,
    } => format!(
      "step {index} tokens (strict): prompt Δ={prompt_delta:+}, completion Δ={completion_delta:+}"
    ),
  }
}

fn format_variance_line(v: &Variance) -> String {
  match v {
    Variance::TokenDelta {
      index,
      prompt_delta,
      completion_delta,
    } => format!(
      "step {index} tokens: prompt Δ={prompt_delta:+}, completion Δ={completion_delta:+}"
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;
  use serde_json::json;

  fn step(index: usize, kind: AgentStepKind) -> AgentStep {
    AgentStep {
      index,
      kind,
      timestamp: Utc::now(),
      duration_ms: None,
    }
  }

  fn trace_with(steps: Vec<AgentStep>, stop: Option<AgentStopReason>) -> ParsedTrace {
    ParsedTrace {
      steps,
      stop_reason: stop,
      llm_calls: std::collections::BTreeMap::new(),
    }
  }

  #[test]
  fn identical_traces_yield_no_divergence() {
    let steps = vec![
      step(0, AgentStepKind::Observe { input: "go".into() }),
      step(
        1,
        AgentStepKind::ToolCall {
          tool: "search".into(),
          params: json!({"q": "rust"}),
        },
      ),
      step(
        2,
        AgentStepKind::FinalAnswer {
          answer: "done".into(),
        },
      ),
    ];
    let base = trace_with(steps.clone(), Some(AgentStopReason::FinalAnswer));
    let curr = trace_with(steps, Some(AgentStopReason::FinalAnswer));
    let report = compute_diff(&base, &curr, false);
    assert!(!report.has_divergence(), "got: {report:?}");
    assert_eq!(report.variances.len(), 0);
  }

  #[test]
  fn step_count_mismatch_is_flagged_once() {
    let base = trace_with(
      vec![step(0, AgentStepKind::Observe { input: "a".into() })],
      None,
    );
    let curr = trace_with(
      vec![
        step(0, AgentStepKind::Observe { input: "a".into() }),
        step(
          1,
          AgentStepKind::FinalAnswer {
            answer: "b".into(),
          },
        ),
      ],
      None,
    );
    let report = compute_diff(&base, &curr, false);
    // Exactly one StepCount divergence — must NOT also emit per-index
    // mismatches for the trailing step.
    let step_count_divs: Vec<_> = report
      .divergences
      .iter()
      .filter(|d| matches!(d, Divergence::StepCount { .. }))
      .collect();
    assert_eq!(step_count_divs.len(), 1);
    // Common prefix matched, so no kind-mismatch.
    assert!(
      !report
        .divergences
        .iter()
        .any(|d| matches!(d, Divergence::StepKindMismatch { .. })),
      "got: {:#?}",
      report.divergences
    );
  }

  #[test]
  fn step_kind_mismatch_in_common_prefix_is_flagged() {
    let base = trace_with(
      vec![step(
        0,
        AgentStepKind::ToolCall {
          tool: "search".into(),
          params: json!({}),
        },
      )],
      None,
    );
    let curr = trace_with(
      vec![step(0, AgentStepKind::Plan { thought: "..".into() })],
      None,
    );
    let report = compute_diff(&base, &curr, false);
    let mismatch = report.divergences.iter().find(|d| {
      matches!(
        d,
        Divergence::StepKindMismatch {
          baseline_kind: "tool_call",
          current_kind: "plan",
          index: 0,
        }
      )
    });
    assert!(
      mismatch.is_some(),
      "expected StepKindMismatch, got: {:?}",
      report.divergences
    );
  }

  #[test]
  fn tool_name_mismatch_when_same_kind() {
    let base = trace_with(
      vec![step(
        0,
        AgentStepKind::ToolCall {
          tool: "search".into(),
          params: json!({}),
        },
      )],
      None,
    );
    let curr = trace_with(
      vec![step(
        0,
        AgentStepKind::ToolCall {
          tool: "browse".into(),
          params: json!({}),
        },
      )],
      None,
    );
    let report = compute_diff(&base, &curr, false);
    let found = report.divergences.iter().any(|d| {
      matches!(
        d,
        Divergence::ToolNameMismatch {
          index: 0,
          baseline_tool: bt,
          current_tool: ct,
        } if bt == "search" && ct == "browse"
      )
    });
    assert!(found, "got: {:?}", report.divergences);
  }

  #[test]
  fn tool_params_difference_alone_is_not_a_divergence() {
    // Params drift between LLM-driven runs is expected jitter; we only
    // flag tool *name* divergence at the step level.
    let base = trace_with(
      vec![step(
        0,
        AgentStepKind::ToolCall {
          tool: "search".into(),
          params: json!({"q": "rust"}),
        },
      )],
      None,
    );
    let curr = trace_with(
      vec![step(
        0,
        AgentStepKind::ToolCall {
          tool: "search".into(),
          params: json!({"q": "rust language"}),
        },
      )],
      None,
    );
    let report = compute_diff(&base, &curr, false);
    assert!(!report.has_divergence(), "got: {:?}", report.divergences);
  }

  #[test]
  fn stop_reason_mismatch_is_flagged() {
    let base = trace_with(vec![], Some(AgentStopReason::FinalAnswer));
    let curr = trace_with(
      vec![],
      Some(AgentStopReason::MaxSteps { max_steps: 10 }),
    );
    let report = compute_diff(&base, &curr, false);
    let found = report.divergences.iter().any(|d| {
      matches!(
        d,
        Divergence::StopReasonMismatch {
          baseline: Some(b),
          current: Some(c),
        } if b == "final_answer" && c == "max_steps"
      )
    });
    assert!(found, "got: {:?}", report.divergences);
  }

  #[test]
  fn missing_terminal_event_on_one_side_is_a_divergence() {
    let base = trace_with(vec![], Some(AgentStopReason::FinalAnswer));
    let curr = trace_with(vec![], None);
    let report = compute_diff(&base, &curr, false);
    let found = report.divergences.iter().any(|d| {
      matches!(
        d,
        Divergence::StopReasonMismatch {
          baseline: Some(b),
          current: None,
        } if b == "final_answer"
      )
    });
    assert!(found, "got: {:?}", report.divergences);
  }

  #[test]
  fn token_delta_is_variance_not_divergence_by_default() {
    let mut llm = std::collections::BTreeMap::new();
    llm.insert(
      0,
      LlmCallSummary {
        prompt_tokens: Some(100),
        completion_tokens: Some(50),
      },
    );
    let base = ParsedTrace {
      llm_calls: llm.clone(),
      ..Default::default()
    };
    let mut llm2 = std::collections::BTreeMap::new();
    llm2.insert(
      0,
      LlmCallSummary {
        prompt_tokens: Some(110),
        completion_tokens: Some(45),
      },
    );
    let curr = ParsedTrace {
      llm_calls: llm2,
      ..Default::default()
    };
    let report = compute_diff(&base, &curr, false);
    assert!(!report.has_divergence());
    let v = report.variances.first().map(|v| match v {
      Variance::TokenDelta {
        index,
        prompt_delta,
        completion_delta,
      } => (*index, *prompt_delta, *completion_delta),
    });
    assert_eq!(v, Some((0, 10, -5)));
  }

  #[test]
  fn token_delta_is_divergence_under_strict_tokens() {
    let mut llm = std::collections::BTreeMap::new();
    llm.insert(
      3,
      LlmCallSummary {
        prompt_tokens: Some(100),
        completion_tokens: Some(50),
      },
    );
    let base = ParsedTrace {
      llm_calls: llm,
      ..Default::default()
    };
    let mut llm2 = std::collections::BTreeMap::new();
    llm2.insert(
      3,
      LlmCallSummary {
        prompt_tokens: Some(101),
        completion_tokens: Some(50),
      },
    );
    let curr = ParsedTrace {
      llm_calls: llm2,
      ..Default::default()
    };
    let report = compute_diff(&base, &curr, true);
    let found = report.divergences.iter().any(|d| {
      matches!(
        d,
        Divergence::TokenDelta {
          index: 3,
          prompt_delta: 1,
          completion_delta: 0,
        }
      )
    });
    assert!(found, "got: {:?}", report.divergences);
    assert!(report.variances.is_empty(), "strict mode promotes deltas");
  }

  #[test]
  fn missing_llm_call_on_one_side_is_a_delta_against_zero() {
    let mut llm = std::collections::BTreeMap::new();
    llm.insert(
      5,
      LlmCallSummary {
        prompt_tokens: Some(200),
        completion_tokens: Some(40),
      },
    );
    let base = ParsedTrace {
      llm_calls: llm,
      ..Default::default()
    };
    let curr = ParsedTrace::default();
    let report = compute_diff(&base, &curr, false);
    let v = report.variances.first().map(|v| match v {
      Variance::TokenDelta {
        index,
        prompt_delta,
        completion_delta,
      } => (*index, *prompt_delta, *completion_delta),
    });
    // current has nothing → delta = current(0) - baseline(200) = -200.
    assert_eq!(v, Some((5, -200, -40)));
  }

  #[test]
  fn parse_trace_jsonl_skips_blank_lines_and_handles_real_event_shapes() {
    let text = r#"
{"event":"run_started","session_id":"s","model":"m","timestamp":"2026-05-21T00:00:00Z"}

{"event":"step_completed","session_id":"s","step":{"index":0,"kind":{"type":"observe","input":"go"},"timestamp":"2026-05-21T00:00:01Z","duration_ms":null}}
{"event":"llm_call_completed","session_id":"s","step_index":0,"model":"m","prompt_tokens":12,"completion_tokens":7,"total_tokens":19,"duration_ms":100,"timestamp":"2026-05-21T00:00:02Z"}
{"event":"run_stopped","session_id":"s","reason":{"reason":"final_answer"},"timestamp":"2026-05-21T00:00:03Z"}
"#;
    let parsed = parse_trace_jsonl(text).expect("must parse");
    assert_eq!(parsed.steps.len(), 1);
    assert_eq!(parsed.steps[0].index, 0);
    assert!(matches!(
      parsed.steps[0].kind,
      AgentStepKind::Observe { .. }
    ));
    assert_eq!(parsed.stop_reason, Some(AgentStopReason::FinalAnswer));
    assert_eq!(
      parsed.llm_calls.get(&0),
      Some(&LlmCallSummary {
        prompt_tokens: Some(12),
        completion_tokens: Some(7),
      })
    );
  }

  #[test]
  fn parse_trace_jsonl_reports_line_number_on_malformed_input() {
    let text = "{\"event\":\"run_started\",\"session_id\":\"s\",\"model\":\"m\",\"timestamp\":\"2026-05-21T00:00:00Z\"}\nnot-json\n";
    let err = parse_trace_jsonl(text).unwrap_err().to_string();
    assert!(
      err.contains("line 2"),
      "expected line number in error: {err}"
    );
  }

  #[test]
  fn output_format_parser_rejects_unknown() {
    assert!(OutputFormat::parse("bogus").is_err());
    assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
    assert_eq!(
      OutputFormat::parse("stream-json").unwrap(),
      OutputFormat::StreamJson
    );
    assert_eq!(
      OutputFormat::parse("json-envelope").unwrap(),
      OutputFormat::JsonEnvelope
    );
  }

  #[test]
  fn last_run_stopped_wins_when_multiple_emitted() {
    let text = r#"
{"event":"run_stopped","session_id":"s","reason":{"reason":"max_steps","max_steps":3},"timestamp":"2026-05-21T00:00:00Z"}
{"event":"run_stopped","session_id":"s","reason":{"reason":"final_answer"},"timestamp":"2026-05-21T00:00:01Z"}
"#;
    let parsed = parse_trace_jsonl(text).expect("must parse");
    // Second one wins.
    assert_eq!(parsed.stop_reason, Some(AgentStopReason::FinalAnswer));
  }
}
