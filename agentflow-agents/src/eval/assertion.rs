//! Closed assertion DSL operating on a finished `AgentRunResult`.
//!
//! Six variants, locked at v1: `contains`, `regex`, `tool_called`,
//! `tool_not_called`, `step_count_below`, `final_answer_matches_skill`.
//! Any new variant must come through a `schema_version` bump in the
//! dataset format (see `docs/AGENT_EVAL_FORMAT.md`).

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{AgentStep, AgentStepKind};

/// Where the `contains` / `regex` assertion searches for its needle /
/// pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionTarget {
  /// The agent's final answer text. Default.
  #[default]
  FinalAnswer,
  /// Any AgentStep's payload string (plan thought, tool param, tool
  /// result, reflection text, final answer).
  AnyStep,
  /// Only the `ToolResult` step bodies.
  AnyToolResult,
}

/// Closed set of six assertion variants. See `docs/AGENT_EVAL_FORMAT.md`
/// for the user-facing reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
  /// Substring match against the configured target.
  Contains {
    needle: String,
    #[serde(default)]
    target: AssertionTarget,
    #[serde(default)]
    case_insensitive: bool,
  },
  /// Rust-flavour regex match against the configured target.
  Regex {
    pattern: String,
    #[serde(default)]
    target: AssertionTarget,
  },
  /// Tool was called at least `min_count` times and at most `max_count`.
  ToolCalled {
    tool: String,
    #[serde(default = "default_min_count")]
    min_count: usize,
    #[serde(default = "default_max_count")]
    max_count: usize,
    /// Optional subset of params every call must match (all key/value
    /// pairs in `with_params` must equal the corresponding fields in the
    /// recorded call params).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    with_params: Option<Value>,
  },
  /// Strict refusal — tool must never be called. Surfaced as a separate
  /// variant because the failure message reads more naturally that way.
  ToolNotCalled { tool: String },
  /// Run terminated with strictly fewer than `max_steps` `AgentStep`s.
  StepCountBelow { max_steps: usize },
  /// Defer to the skill's bundled validator. Implementation is provided
  /// by the runner; here we just store the marker variant.
  FinalAnswerMatchesSkill {},
}

fn default_min_count() -> usize {
  1
}
fn default_max_count() -> usize {
  usize::MAX
}

/// Where a [`Assertion::Contains`] / [`Assertion::Regex`] hit was found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionInScope {
  /// The substring or pattern was found in the final answer text.
  FinalAnswer,
  /// Found inside an `AgentStep` payload (plan / tool call / tool
  /// result / reflection / final answer) at the given index.
  Step { index: usize, kind: String },
}

/// Richer outcome returned by the skill validator closure. The
/// `final_answer_matches_skill` assertion maps each variant to a
/// distinct `AssertionOutcome.reason` text so operators can tell
/// "skill rejected this answer" from "we couldn't ask the skill"
/// without re-parsing the trace. See
/// `docs/SKILL_VALIDATOR_PROTOCOL.md` for the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillValidatorVerdict {
  Pass,
  Fail { reason: String },
  Unrunnable { reason: String },
}

/// Type alias for the skill validator closure passed through
/// [`AssertionContext`]. The closure is `None` when no validator is
/// wired ("skill declares no validator"); when present, it returns
/// the rich [`SkillValidatorVerdict`].
pub type SkillValidator<'a> = dyn Fn(&str) -> SkillValidatorVerdict + Send + Sync + 'a;

/// Context passed to [`Assertion::evaluate`]. Borrowed-only; the
/// assertion never owns the agent's run output.
pub struct AssertionContext<'a> {
  /// Append-only step stream produced by the run.
  pub steps: &'a [AgentStep],
  /// Final answer when the run produced one.
  pub final_answer: Option<&'a str>,
  /// Optional skill validator. Wired by the runner from the skill
  /// manifest.
  pub skill_validator: Option<&'a SkillValidator<'a>>,
}

/// Outcome of evaluating one [`Assertion`] against the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionOutcome {
  /// The assertion description (variant + fields). Surfaced verbatim in
  /// the report so failure rows are self-explanatory.
  pub assertion: Assertion,
  /// `true` when the assertion held, `false` otherwise.
  pub passed: bool,
  /// Where the hit was found (only set for `contains` / `regex` on
  /// success).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub matched_in: Option<AssertionInScope>,
  /// Observed count for tool-related assertions. `None` for the other
  /// variants.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub actual_count: Option<usize>,
  /// Free-form failure reason. `None` on success.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

impl Assertion {
  /// Evaluate this assertion against a finished run. Always returns an
  /// outcome — never panics.
  pub fn evaluate(&self, context: &AssertionContext<'_>) -> AssertionOutcome {
    match self {
      Self::Contains {
        needle,
        target,
        case_insensitive,
      } => evaluate_contains(self.clone(), needle, *target, *case_insensitive, context),
      Self::Regex { pattern, target } => evaluate_regex(self.clone(), pattern, *target, context),
      Self::ToolCalled {
        tool,
        min_count,
        max_count,
        with_params,
      } => evaluate_tool_called(
        self.clone(),
        tool,
        *min_count,
        *max_count,
        with_params.as_ref(),
        context,
      ),
      Self::ToolNotCalled { tool } => evaluate_tool_not_called(self.clone(), tool, context),
      Self::StepCountBelow { max_steps } => {
        evaluate_step_count_below(self.clone(), *max_steps, context)
      }
      Self::FinalAnswerMatchesSkill {} => {
        evaluate_final_answer_matches_skill(self.clone(), context)
      }
    }
  }
}

fn evaluate_contains(
  assertion: Assertion,
  needle: &str,
  target: AssertionTarget,
  case_insensitive: bool,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let needle_norm = if case_insensitive {
    needle.to_lowercase()
  } else {
    needle.to_string()
  };
  let matcher = |hay: &str| -> bool {
    if case_insensitive {
      hay.to_lowercase().contains(&needle_norm)
    } else {
      hay.contains(&needle_norm)
    }
  };
  match locate_text_match(target, context, &matcher) {
    Some(scope) => AssertionOutcome {
      assertion,
      passed: true,
      matched_in: Some(scope),
      actual_count: None,
      reason: None,
    },
    None => AssertionOutcome {
      assertion,
      passed: false,
      matched_in: None,
      actual_count: None,
      reason: Some(format!(
        "needle '{}' not found in {}",
        needle,
        describe_target(target)
      )),
    },
  }
}

fn evaluate_regex(
  assertion: Assertion,
  pattern: &str,
  target: AssertionTarget,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let compiled = match Regex::new(pattern) {
    Ok(r) => r,
    Err(e) => {
      return AssertionOutcome {
        assertion,
        passed: false,
        matched_in: None,
        actual_count: None,
        reason: Some(format!("invalid regex '{}': {}", pattern, e)),
      };
    }
  };
  let matcher = |hay: &str| compiled.is_match(hay);
  match locate_text_match(target, context, &matcher) {
    Some(scope) => AssertionOutcome {
      assertion,
      passed: true,
      matched_in: Some(scope),
      actual_count: None,
      reason: None,
    },
    None => AssertionOutcome {
      assertion,
      passed: false,
      matched_in: None,
      actual_count: None,
      reason: Some(format!(
        "pattern '{}' did not match {}",
        pattern,
        describe_target(target)
      )),
    },
  }
}

fn evaluate_tool_called(
  assertion: Assertion,
  tool: &str,
  min_count: usize,
  max_count: usize,
  with_params: Option<&Value>,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let count = context
    .steps
    .iter()
    .filter(|step| match &step.kind {
      AgentStepKind::ToolCall { tool: name, params } => {
        name == tool && params_matches_subset(with_params, params)
      }
      _ => false,
    })
    .count();
  let passed = count >= min_count && count <= max_count;
  let reason = if passed {
    None
  } else if count < min_count {
    Some(format!(
      "tool '{}' was called {} times (min {})",
      tool, count, min_count
    ))
  } else {
    Some(format!(
      "tool '{}' was called {} times (max {})",
      tool, count, max_count
    ))
  };
  AssertionOutcome {
    assertion,
    passed,
    matched_in: None,
    actual_count: Some(count),
    reason,
  }
}

fn evaluate_tool_not_called(
  assertion: Assertion,
  tool: &str,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let mut at_step: Option<usize> = None;
  let mut count = 0usize;
  for step in context.steps.iter() {
    if let AgentStepKind::ToolCall { tool: name, .. } = &step.kind
      && name == tool
    {
      count += 1;
      at_step.get_or_insert(step.index);
    }
  }
  if count == 0 {
    AssertionOutcome {
      assertion,
      passed: true,
      matched_in: None,
      actual_count: Some(0),
      reason: None,
    }
  } else {
    AssertionOutcome {
      assertion,
      passed: false,
      matched_in: None,
      actual_count: Some(count),
      reason: Some(format!(
        "tool '{}' was called {} time(s) (first at step {})",
        tool,
        count,
        at_step.unwrap_or(0)
      )),
    }
  }
}

fn evaluate_step_count_below(
  assertion: Assertion,
  max_steps: usize,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let count = context.steps.len();
  let passed = count < max_steps;
  AssertionOutcome {
    assertion,
    passed,
    matched_in: None,
    actual_count: Some(count),
    reason: if passed {
      None
    } else {
      Some(format!(
        "run produced {} steps (expected strictly fewer than {})",
        count, max_steps
      ))
    },
  }
}

fn evaluate_final_answer_matches_skill(
  assertion: Assertion,
  context: &AssertionContext<'_>,
) -> AssertionOutcome {
  let answer = match context.final_answer {
    Some(a) => a,
    None => {
      return AssertionOutcome {
        assertion,
        passed: false,
        matched_in: None,
        actual_count: None,
        reason: Some("run produced no final answer".to_string()),
      };
    }
  };
  let validator = match context.skill_validator {
    Some(v) => v,
    None => {
      return AssertionOutcome {
        assertion,
        passed: false,
        matched_in: None,
        actual_count: None,
        reason: Some("skill declares no validator".to_string()),
      };
    }
  };
  match validator(answer) {
    SkillValidatorVerdict::Pass => AssertionOutcome {
      assertion,
      passed: true,
      matched_in: None,
      actual_count: None,
      reason: None,
    },
    SkillValidatorVerdict::Fail { reason } => AssertionOutcome {
      assertion,
      passed: false,
      matched_in: None,
      actual_count: None,
      reason: Some(reason),
    },
    SkillValidatorVerdict::Unrunnable { reason } => AssertionOutcome {
      assertion,
      passed: false,
      matched_in: None,
      actual_count: None,
      reason: Some(format!("validator unrunnable: {reason}")),
    },
  }
}

fn locate_text_match(
  target: AssertionTarget,
  context: &AssertionContext<'_>,
  matcher: &dyn Fn(&str) -> bool,
) -> Option<AssertionInScope> {
  match target {
    AssertionTarget::FinalAnswer => context
      .final_answer
      .filter(|ans| matcher(ans))
      .map(|_| AssertionInScope::FinalAnswer),
    AssertionTarget::AnyStep => context.steps.iter().find_map(|step| {
      let text = step_text(&step.kind);
      if matcher(&text) {
        Some(AssertionInScope::Step {
          index: step.index,
          kind: step_kind_label(&step.kind).to_string(),
        })
      } else {
        None
      }
    }),
    AssertionTarget::AnyToolResult => context.steps.iter().find_map(|step| match &step.kind {
      AgentStepKind::ToolResult { content, .. } if matcher(content) => {
        Some(AssertionInScope::Step {
          index: step.index,
          kind: "tool_result".to_string(),
        })
      }
      _ => None,
    }),
  }
}

fn describe_target(target: AssertionTarget) -> &'static str {
  match target {
    AssertionTarget::FinalAnswer => "final_answer",
    AssertionTarget::AnyStep => "any step body",
    AssertionTarget::AnyToolResult => "any tool result",
  }
}

fn step_text(kind: &AgentStepKind) -> String {
  match kind {
    AgentStepKind::Observe { input } => input.clone(),
    AgentStepKind::Plan { thought } => thought.clone(),
    AgentStepKind::ToolCall { tool, params } => format!("{} {}", tool, params),
    AgentStepKind::ToolResult { content, .. } => content.clone(),
    AgentStepKind::Reflect { content } => content.clone(),
    AgentStepKind::FinalAnswer { answer } => answer.clone(),
    AgentStepKind::Handoff { from, to, message } => format!("{} -> {}: {}", from, to, message),
    AgentStepKind::BlackboardOp { op, key, agent, .. } => {
      format!("{:?} {} (by {})", op, key, agent)
    }
    AgentStepKind::DebateProposal {
      round,
      agent,
      proposal,
    } => format!("round {} {}: {}", round, agent, proposal),
    AgentStepKind::DebateVerdict { rationale, .. } => rationale.clone(),
  }
}

fn step_kind_label(kind: &AgentStepKind) -> &'static str {
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

fn params_matches_subset(filter: Option<&Value>, actual: &Value) -> bool {
  let filter = match filter {
    Some(f) => f,
    None => return true,
  };
  let (Some(filter_obj), Some(actual_obj)) = (filter.as_object(), actual.as_object()) else {
    return filter == actual;
  };
  let filter_map: BTreeMap<&str, &Value> =
    filter_obj.iter().map(|(k, v)| (k.as_str(), v)).collect();
  for (k, expected) in filter_map.iter() {
    match actual_obj.get(*k) {
      Some(got) if got == *expected => continue,
      _ => return false,
    }
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::AgentStep;
  use serde_json::json;

  fn ctx_with_steps_and_answer<'a>(
    steps: &'a [AgentStep],
    final_answer: Option<&'a str>,
  ) -> AssertionContext<'a> {
    AssertionContext {
      steps,
      final_answer,
      skill_validator: None,
    }
  }

  fn observe(idx: usize, text: &str) -> AgentStep {
    AgentStep::new(
      idx,
      AgentStepKind::Observe {
        input: text.to_string(),
      },
    )
  }

  fn tool_call(idx: usize, tool: &str, params: Value) -> AgentStep {
    AgentStep::new(
      idx,
      AgentStepKind::ToolCall {
        tool: tool.to_string(),
        params,
      },
    )
  }

  fn tool_result(idx: usize, tool: &str, content: &str) -> AgentStep {
    AgentStep::new(
      idx,
      AgentStepKind::ToolResult {
        tool: tool.to_string(),
        content: content.to_string(),
        is_error: false,
        parts: Vec::new(),
      },
    )
  }

  fn final_answer_step(idx: usize, answer: &str) -> AgentStep {
    AgentStep::new(
      idx,
      AgentStepKind::FinalAnswer {
        answer: answer.to_string(),
      },
    )
  }

  // ── Contains ───────────────────────────────────────────────────────────

  #[test]
  fn contains_passes_when_final_answer_includes_needle() {
    let assertion = Assertion::Contains {
      needle: "OAuth".to_string(),
      target: AssertionTarget::FinalAnswer,
      case_insensitive: false,
    };
    let ctx = ctx_with_steps_and_answer(&[], Some("Use OAuth 2.0 with PKCE"));
    let outcome = assertion.evaluate(&ctx);
    assert!(outcome.passed);
    assert_eq!(outcome.matched_in, Some(AssertionInScope::FinalAnswer));
  }

  #[test]
  fn contains_fails_when_needle_missing() {
    let assertion = Assertion::Contains {
      needle: "OAuth".to_string(),
      target: AssertionTarget::FinalAnswer,
      case_insensitive: false,
    };
    let ctx = ctx_with_steps_and_answer(&[], Some("Use basic auth"));
    let outcome = assertion.evaluate(&ctx);
    assert!(!outcome.passed);
    assert!(outcome.reason.unwrap().contains("OAuth"));
  }

  #[test]
  fn contains_case_insensitive_normalizes_both_sides() {
    let assertion = Assertion::Contains {
      needle: "OAUTH".to_string(),
      target: AssertionTarget::FinalAnswer,
      case_insensitive: true,
    };
    let ctx = ctx_with_steps_and_answer(&[], Some("use oauth here"));
    assert!(assertion.evaluate(&ctx).passed);
  }

  #[test]
  fn contains_against_any_step_finds_match_in_plan() {
    let assertion = Assertion::Contains {
      needle: "search".to_string(),
      target: AssertionTarget::AnyStep,
      case_insensitive: false,
    };
    let steps = vec![AgentStep::new(
      1,
      AgentStepKind::Plan {
        thought: "Need to search the docs".to_string(),
      },
    )];
    let ctx = ctx_with_steps_and_answer(&steps, None);
    let outcome = assertion.evaluate(&ctx);
    assert!(outcome.passed);
    assert!(matches!(
      outcome.matched_in,
      Some(AssertionInScope::Step { kind, .. }) if kind == "plan"
    ));
  }

  #[test]
  fn contains_against_tool_result_only_searches_tool_results() {
    let assertion = Assertion::Contains {
      needle: "404".to_string(),
      target: AssertionTarget::AnyToolResult,
      case_insensitive: false,
    };
    let steps = vec![
      observe(0, "404 in observation should not match"),
      tool_result(1, "http", "server returned 404 Not Found"),
    ];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(outcome.passed);
    assert!(matches!(
      outcome.matched_in,
      Some(AssertionInScope::Step { kind, .. }) if kind == "tool_result"
    ));
  }

  // ── Regex ──────────────────────────────────────────────────────────────

  #[test]
  fn regex_matches_word_boundaries() {
    let assertion = Assertion::Regex {
      pattern: r"(?i)\bOAuth\s+token\b".to_string(),
      target: AssertionTarget::FinalAnswer,
    };
    let ctx = ctx_with_steps_and_answer(&[], Some("Use the OAuth token here"));
    assert!(assertion.evaluate(&ctx).passed);
  }

  #[test]
  fn regex_with_invalid_pattern_fails_with_compile_error() {
    let assertion = Assertion::Regex {
      pattern: "[unclosed".to_string(),
      target: AssertionTarget::FinalAnswer,
    };
    let ctx = ctx_with_steps_and_answer(&[], Some("anything"));
    let outcome = assertion.evaluate(&ctx);
    assert!(!outcome.passed);
    assert!(outcome.reason.unwrap().contains("invalid regex"));
  }

  // ── ToolCalled / ToolNotCalled ────────────────────────────────────────

  #[test]
  fn tool_called_passes_when_min_count_met() {
    let assertion = Assertion::ToolCalled {
      tool: "search".to_string(),
      min_count: 1,
      max_count: usize::MAX,
      with_params: None,
    };
    let steps = vec![
      tool_call(0, "search", json!({"q": "rust"})),
      tool_call(1, "search", json!({"q": "tokio"})),
    ];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(outcome.passed);
    assert_eq!(outcome.actual_count, Some(2));
  }

  #[test]
  fn tool_called_fails_when_count_exceeds_max() {
    let assertion = Assertion::ToolCalled {
      tool: "search".to_string(),
      min_count: 1,
      max_count: 1,
      with_params: None,
    };
    let steps = vec![
      tool_call(0, "search", json!({"q": "rust"})),
      tool_call(1, "search", json!({"q": "tokio"})),
    ];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(!outcome.passed);
    assert_eq!(outcome.actual_count, Some(2));
    assert!(outcome.reason.unwrap().contains("max 1"));
  }

  #[test]
  fn tool_called_with_params_subset_must_match() {
    let assertion = Assertion::ToolCalled {
      tool: "search".to_string(),
      min_count: 1,
      max_count: usize::MAX,
      with_params: Some(json!({"q": "rust"})),
    };
    let steps = vec![
      tool_call(0, "search", json!({"q": "tokio", "k": 1})),
      tool_call(1, "search", json!({"q": "rust", "k": 2})),
    ];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(outcome.passed);
    assert_eq!(outcome.actual_count, Some(1));
  }

  #[test]
  fn tool_not_called_passes_when_tool_absent() {
    let assertion = Assertion::ToolNotCalled {
      tool: "shell".to_string(),
    };
    let steps = vec![tool_call(0, "file", json!({"path": "/tmp"}))];
    assert!(
      assertion
        .evaluate(&ctx_with_steps_and_answer(&steps, None))
        .passed
    );
  }

  #[test]
  fn tool_not_called_fails_with_step_index_in_reason() {
    let assertion = Assertion::ToolNotCalled {
      tool: "shell".to_string(),
    };
    let steps = vec![observe(0, "x"), tool_call(4, "shell", json!({"cmd": "ls"}))];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(!outcome.passed);
    assert!(outcome.reason.unwrap().contains("first at step 4"));
  }

  // ── StepCountBelow ─────────────────────────────────────────────────────

  #[test]
  fn step_count_below_passes_when_strictly_below() {
    let assertion = Assertion::StepCountBelow { max_steps: 6 };
    let steps = vec![observe(0, "a"), observe(1, "b"), observe(2, "c")];
    assert!(
      assertion
        .evaluate(&ctx_with_steps_and_answer(&steps, None))
        .passed
    );
  }

  #[test]
  fn step_count_below_fails_at_the_boundary() {
    let assertion = Assertion::StepCountBelow { max_steps: 3 };
    let steps = vec![observe(0, "a"), observe(1, "b"), observe(2, "c")];
    let outcome = assertion.evaluate(&ctx_with_steps_and_answer(&steps, None));
    assert!(!outcome.passed);
    assert_eq!(outcome.actual_count, Some(3));
  }

  // ── FinalAnswerMatchesSkill ───────────────────────────────────────────

  #[test]
  fn final_answer_matches_skill_fails_without_validator() {
    let assertion = Assertion::FinalAnswerMatchesSkill {};
    let ctx = AssertionContext {
      steps: &[],
      final_answer: Some("anything"),
      skill_validator: None,
    };
    let outcome = assertion.evaluate(&ctx);
    assert!(!outcome.passed);
    assert!(outcome.reason.unwrap().contains("no validator"));
  }

  #[test]
  fn final_answer_matches_skill_passes_when_validator_returns_pass() {
    let validator = |ans: &str| -> SkillValidatorVerdict {
      if ans.contains("hi") {
        SkillValidatorVerdict::Pass
      } else {
        SkillValidatorVerdict::Fail {
          reason: "no hi".to_string(),
        }
      }
    };
    let assertion = Assertion::FinalAnswerMatchesSkill {};
    let ctx = AssertionContext {
      steps: &[final_answer_step(0, "hi there")],
      final_answer: Some("hi there"),
      skill_validator: Some(&validator),
    };
    let outcome = assertion.evaluate(&ctx);
    assert!(outcome.passed);
  }

  #[test]
  fn final_answer_matches_skill_fails_when_validator_returns_fail_and_surfaces_reason() {
    let validator = |_ans: &str| -> SkillValidatorVerdict {
      SkillValidatorVerdict::Fail {
        reason: "validator rejected: missing OK prefix".to_string(),
      }
    };
    let assertion = Assertion::FinalAnswerMatchesSkill {};
    let ctx = AssertionContext {
      steps: &[final_answer_step(0, "bye")],
      final_answer: Some("bye"),
      skill_validator: Some(&validator),
    };
    let outcome = assertion.evaluate(&ctx);
    assert!(!outcome.passed);
    let reason = outcome.reason.unwrap();
    assert!(
      reason.contains("missing OK prefix"),
      "reason should carry the validator's own message: {reason}"
    );
  }

  #[test]
  fn final_answer_matches_skill_surfaces_unrunnable_reason_distinctly() {
    let validator = |_ans: &str| -> SkillValidatorVerdict {
      SkillValidatorVerdict::Unrunnable {
        reason: "command exited 125: PATH broken".to_string(),
      }
    };
    let assertion = Assertion::FinalAnswerMatchesSkill {};
    let ctx = AssertionContext {
      steps: &[final_answer_step(0, "anything")],
      final_answer: Some("anything"),
      skill_validator: Some(&validator),
    };
    let outcome = assertion.evaluate(&ctx);
    assert!(!outcome.passed);
    let reason = outcome.reason.unwrap();
    assert!(
      reason.starts_with("validator unrunnable:"),
      "reason should be prefixed 'validator unrunnable:'; got: {reason}"
    );
    assert!(
      reason.contains("PATH broken"),
      "reason should carry the unrunnable detail; got: {reason}"
    );
  }

  // ── Serde round trip ───────────────────────────────────────────────────

  #[test]
  fn assertion_round_trips_through_serde_json() {
    let original = Assertion::ToolCalled {
      tool: "web_search".to_string(),
      min_count: 1,
      max_count: 3,
      with_params: Some(json!({"engine": "duckduckgo"})),
    };
    let s = serde_json::to_string(&original).unwrap();
    let back: Assertion = serde_json::from_str(&s).unwrap();
    match back {
      Assertion::ToolCalled {
        tool,
        min_count,
        max_count,
        with_params,
      } => {
        assert_eq!(tool, "web_search");
        assert_eq!(min_count, 1);
        assert_eq!(max_count, 3);
        assert_eq!(with_params, Some(json!({"engine": "duckduckgo"})));
      }
      _ => panic!("variant changed across round trip"),
    }
  }
}
