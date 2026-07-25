//! Result aggregation and conflict arbitration (L5.2).
//!
//! A structured primitive for combining multiple subagents' answers to the
//! *same* [`crate::delegation::DelegationSpec`] goal: dedup equivalent
//! answers, rank the survivors by how many subagents agree, and flag a
//! conflict — the TODO's own framing is "same question, opposite
//! conclusions → explicitly flag for the main agent to review," not
//! "auto-resolve." [`aggregate_answers`] never silently picks a winner; it
//! reports what happened and leaves the arbitration decision to the
//! caller (typically the parent agent, per the TODO).
//!
//! Depends on L5.1's [`crate::delegation::SchemaValidation`] convention: an
//! answer that validated against its `DelegationSpec::expected_output_schema`
//! is deduped by structural JSON equality (so `{"a":1,"b":2}` and
//! `{"b":2,"a":1}` count as the same answer); an answer with no schema (or
//! that failed validation) falls back to trimmed string equality.
//!
//! Recording an [`AggregationReport`] into a run's trace is the caller's
//! job — this module is a pure function over already-collected answers, no
//! `AgentStep`/`AgentEvent` dependency, mirroring how
//! [`crate::delegation::DelegationSpec`] itself carries no `ToolRegistry`/
//! `ReActAgent` reference.

use serde_json::Value;

use crate::delegation::SchemaValidation;

/// One subagent's answer to a shared delegation goal.
#[derive(Debug, Clone)]
pub struct SubagentAnswer {
  pub agent_name: String,
  pub answer: String,
  pub schema_validation: SchemaValidation,
}

impl SubagentAnswer {
  pub fn new(agent_name: impl Into<String>, answer: impl Into<String>) -> Self {
    Self {
      agent_name: agent_name.into(),
      answer: answer.into(),
      schema_validation: SchemaValidation::NotRequired,
    }
  }

  pub fn with_schema_validation(mut self, validation: SchemaValidation) -> Self {
    self.schema_validation = validation;
    self
  }

  /// The key equivalent answers are deduped on: the parsed JSON value when
  /// this answer validated against a schema, the trimmed raw text
  /// otherwise.
  fn dedup_key(&self) -> DedupKey {
    if matches!(self.schema_validation, SchemaValidation::Valid)
      && let Ok(value) = serde_json::from_str::<Value>(&self.answer)
    {
      return DedupKey::Json(value);
    }
    DedupKey::Text(self.answer.trim().to_string())
  }
}

#[derive(Debug, Clone, PartialEq)]
enum DedupKey {
  Json(Value),
  Text(String),
}

/// One distinct answer and every subagent that produced an equivalent one.
#[derive(Debug, Clone)]
pub struct AnswerGroup {
  /// The first-seen answer text representing this group.
  pub answer: String,
  pub agent_names: Vec<String>,
}

/// Outcome of [`aggregate_answers`].
#[derive(Debug, Clone)]
pub struct AggregationReport {
  /// Distinct answer groups, ranked by supporting-agent-count descending;
  /// ties keep first-seen order for determinism.
  pub groups: Vec<AnswerGroup>,
  /// `true` when more than one distinct answer group exists.
  pub has_conflict: bool,
}

impl AggregationReport {
  /// Agent names outside the top-ranked group — the TODO's "explicitly
  /// flagged for the main agent to review" set. Empty when there's no
  /// conflict (including the zero- or one-answer cases).
  pub fn flagged_for_review(&self) -> Vec<&str> {
    if !self.has_conflict {
      return Vec::new();
    }
    self
      .groups
      .iter()
      .skip(1)
      .flat_map(|group| group.agent_names.iter().map(String::as_str))
      .collect()
  }

  /// Human/LLM-readable summary, suitable for handing to "the main agent"
  /// for review — the literal delivery mechanism the TODO's "交主 agent
  /// 复核" (hand to the main agent for review) calls for.
  pub fn render_summary(&self) -> String {
    if self.groups.is_empty() {
      return "No subagent answers to aggregate.".to_string();
    }
    if !self.has_conflict {
      let group = &self.groups[0];
      return format!(
        "All {} subagent(s) agree: {}",
        group.agent_names.len(),
        group.answer
      );
    }
    let mut out = format!(
      "Conflict: {} distinct answers across subagents.\n",
      self.groups.len()
    );
    for (rank, group) in self.groups.iter().enumerate() {
      out.push_str(&format!(
        "  [{}] {} agent(s) ({}): {}\n",
        rank + 1,
        group.agent_names.len(),
        group.agent_names.join(", "),
        group.answer
      ));
    }
    out
  }
}

/// Aggregate multiple subagents' answers to the same delegation goal:
/// dedup equivalent answers (structural JSON equality when an answer
/// validated against its schema, trimmed text equality otherwise), rank
/// the resulting groups by supporting-agent-count descending, and flag a
/// conflict when more than one group survives.
pub fn aggregate_answers(answers: &[SubagentAnswer]) -> AggregationReport {
  let mut groups: Vec<(DedupKey, AnswerGroup)> = Vec::new();
  for answer in answers {
    let key = answer.dedup_key();
    if let Some((_, group)) = groups.iter_mut().find(|(k, _)| *k == key) {
      group.agent_names.push(answer.agent_name.clone());
    } else {
      groups.push((
        key,
        AnswerGroup {
          answer: answer.answer.clone(),
          agent_names: vec![answer.agent_name.clone()],
        },
      ));
    }
  }
  // Stable sort by descending support so first-seen order breaks ties.
  groups.sort_by_key(|(_, group)| std::cmp::Reverse(group.agent_names.len()));
  let has_conflict = groups.len() > 1;
  AggregationReport {
    groups: groups.into_iter().map(|(_, group)| group).collect(),
    has_conflict,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_answers_is_not_a_conflict() {
    let report = aggregate_answers(&[]);
    assert!(report.groups.is_empty());
    assert!(!report.has_conflict);
    assert!(report.flagged_for_review().is_empty());
  }

  #[test]
  fn identical_text_answers_dedup_into_one_group() {
    let answers = vec![
      SubagentAnswer::new("a1", "the sky is blue"),
      SubagentAnswer::new("a2", "the sky is blue"),
      SubagentAnswer::new("a3", "the sky is blue"),
    ];
    let report = aggregate_answers(&answers);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].agent_names, vec!["a1", "a2", "a3"]);
    assert!(!report.has_conflict);
  }

  #[test]
  fn text_answers_dedup_after_trimming_whitespace() {
    let answers = vec![
      SubagentAnswer::new("a1", "the sky is blue"),
      SubagentAnswer::new("a2", "  the sky is blue  "),
    ];
    let report = aggregate_answers(&answers);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].agent_names.len(), 2);
  }

  #[test]
  fn conflicting_text_answers_are_flagged() {
    let answers = vec![
      SubagentAnswer::new("a1", "yes, it is safe"),
      SubagentAnswer::new("a2", "yes, it is safe"),
      SubagentAnswer::new("a3", "no, it is not safe"),
    ];
    let report = aggregate_answers(&answers);
    assert!(report.has_conflict);
    assert_eq!(report.groups.len(), 2);
    // Majority group (2 agents) ranks first.
    assert_eq!(report.groups[0].agent_names, vec!["a1", "a2"]);
    assert_eq!(report.groups[1].agent_names, vec!["a3"]);
    assert_eq!(report.flagged_for_review(), vec!["a3"]);
  }

  #[test]
  fn schema_valid_json_answers_dedup_by_structural_equality_regardless_of_key_order() {
    let answers = vec![
      SubagentAnswer::new("a1", r#"{"a":1,"b":2}"#).with_schema_validation(SchemaValidation::Valid),
      SubagentAnswer::new("a2", r#"{"b":2,"a":1}"#).with_schema_validation(SchemaValidation::Valid),
    ];
    let report = aggregate_answers(&answers);
    assert_eq!(
      report.groups.len(),
      1,
      "key order must not create a false conflict"
    );
    assert!(!report.has_conflict);
  }

  #[test]
  fn schema_valid_json_answers_with_different_values_conflict() {
    let answers = vec![
      SubagentAnswer::new("a1", r#"{"summary":"approved"}"#)
        .with_schema_validation(SchemaValidation::Valid),
      SubagentAnswer::new("a2", r#"{"summary":"rejected"}"#)
        .with_schema_validation(SchemaValidation::Valid),
    ];
    let report = aggregate_answers(&answers);
    assert!(report.has_conflict);
    assert_eq!(report.groups.len(), 2);
  }

  #[test]
  fn invalid_schema_validation_falls_back_to_text_dedup() {
    // Even though the answer text happens to be JSON-shaped, a failed
    // validation means we don't trust it enough to parse structurally —
    // exact text comparison is the safe fallback.
    let answers = vec![
      SubagentAnswer::new("a1", r#"{"a":1,"b":2}"#).with_schema_validation(
        SchemaValidation::Invalid {
          errors: vec!["missing required field".to_string()],
        },
      ),
      SubagentAnswer::new("a2", r#"{"b":2,"a":1}"#).with_schema_validation(
        SchemaValidation::Invalid {
          errors: vec!["missing required field".to_string()],
        },
      ),
    ];
    let report = aggregate_answers(&answers);
    assert_eq!(
      report.groups.len(),
      2,
      "differently-ordered JSON text must NOT dedup once validation failed"
    );
  }

  #[test]
  fn ties_break_by_first_seen_order() {
    let answers = vec![
      SubagentAnswer::new("a1", "option A"),
      SubagentAnswer::new("a2", "option B"),
    ];
    let report = aggregate_answers(&answers);
    assert_eq!(report.groups[0].answer, "option A");
    assert_eq!(report.groups[1].answer, "option B");
  }

  #[test]
  fn render_summary_reports_unanimous_agreement() {
    let answers = vec![
      SubagentAnswer::new("a1", "42"),
      SubagentAnswer::new("a2", "42"),
    ];
    let report = aggregate_answers(&answers);
    let summary = report.render_summary();
    assert!(summary.contains("All 2 subagent"));
    assert!(summary.contains("42"));
  }

  #[test]
  fn render_summary_reports_conflict_with_every_group() {
    let answers = vec![
      SubagentAnswer::new("a1", "yes"),
      SubagentAnswer::new("a2", "no"),
    ];
    let report = aggregate_answers(&answers);
    let summary = report.render_summary();
    assert!(summary.contains("Conflict"));
    assert!(summary.contains("a1"));
    assert!(summary.contains("a2"));
  }

  #[test]
  fn render_summary_handles_empty_input() {
    let report = aggregate_answers(&[]);
    assert_eq!(report.render_summary(), "No subagent answers to aggregate.");
  }
}
