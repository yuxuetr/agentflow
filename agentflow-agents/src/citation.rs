//! Citation consistency verification (L4.4).
//!
//! Checks whether a final answer's `[n]`-style citation markers — the
//! numbering `agentflow_rag::tool::RagSearchTool::render()` bakes into a
//! `rag_search` tool result's content, e.g. `"[1] (source: docs/a.md,
//! score: 0.812)\n<passage>"` — are actually supported by the passage they
//! point to. Unlike `agentflow-rag`'s L4.2 `RelevanceScorer` / L4.3
//! `QueryRewriter` (deliberately LLM-free crates), `agentflow-agents`
//! already depends on `agentflow-llm`, so [`LlmCitationChecker`] here is a
//! real, usable "lightweight LLM-as-judge" implementation, not an
//! injection point deferred to a higher layer.
//!
//! [`verify_citations`] is the entry point: given a completed run's
//! [`AgentStep`]s and a candidate final answer, it locates the most recent
//! `rag_search` tool result, parses its citations, keeps only the markers
//! the answer actually references, and runs them through a
//! [`CitationChecker`]. [`downgrade_answer`] strips citation markers from
//! an answer that failed the check — the TODO's literal "不通过降级为无
//! 引用回答" (on failure, downgrade to a no-citation answer): once even one
//! citation is judged unsupported, the whole marker numbering can no
//! longer be trusted by a reader, so every referenced marker is stripped,
//! not just the failing ones.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use serde_json::json;

use crate::runtime::{AgentStep, AgentStepKind};

/// One citation marker (`[n]`) parsed out of a `rag_search` tool result,
/// paired with the passage it points to.
#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
  /// The literal marker text as it appears in the answer, e.g. `"[1]"`.
  pub marker: String,
  pub source: Option<String>,
  pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CitationVerdict {
  Supported,
  Unsupported { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum CitationCheckError {
  #[error("Citation checker failed: {message}")]
  Failed { message: String },
}

/// Judges whether cited passages support the claims an answer makes near
/// their markers. Returns one verdict per input citation, same order.
#[async_trait]
pub trait CitationChecker: Send + Sync {
  async fn check(
    &self,
    answer: &str,
    citations: &[Citation],
  ) -> Result<Vec<CitationVerdict>, CitationCheckError>;

  fn name(&self) -> &str;
}

#[allow(
  clippy::expect_used,
  reason = "compile-time regex literal; never fails at runtime on any input"
)]
fn citation_marker_regex() -> &'static Regex {
  static RE: OnceLock<Regex> = OnceLock::new();
  RE.get_or_init(|| {
    Regex::new(r"^\[(\d+)\] \(source: (.*), score: [-\d.]+\)$")
      .expect("valid citation marker regex")
  })
}

/// Parses the `[n] (source: ..., score: ...)\n<content>` blocks
/// `RagSearchTool::render()` produces back into structured [`Citation`]s.
/// Best-effort: text that doesn't match the marker pattern is treated as
/// part of the preceding citation's content (or ignored if no marker has
/// been seen yet).
pub fn parse_citations_from_tool_result(content: &str) -> Vec<Citation> {
  let marker_re = citation_marker_regex();
  let mut citations = Vec::new();
  let mut current: Option<(String, Option<String>, Vec<String>)> = None;

  for line in content.lines() {
    if let Some(caps) = marker_re.captures(line) {
      if let Some((marker, source, lines)) = current.take() {
        citations.push(Citation {
          marker,
          source,
          content: lines.join("\n").trim().to_string(),
        });
      }
      let marker = format!("[{}]", &caps[1]);
      let source_text = caps[2].trim();
      let source = if source_text.is_empty() {
        None
      } else {
        Some(source_text.to_string())
      };
      current = Some((marker, source, Vec::new()));
    } else if let Some((_, _, lines)) = current.as_mut() {
      lines.push(line.to_string());
    }
  }
  if let Some((marker, source, lines)) = current {
    citations.push(Citation {
      marker,
      source,
      content: lines.join("\n").trim().to_string(),
    });
  }
  citations
}

/// Keeps only the citations whose marker literally appears in `answer`.
pub fn citations_referenced_in_answer(answer: &str, citations: &[Citation]) -> Vec<Citation> {
  citations
    .iter()
    .filter(|c| answer.contains(&c.marker))
    .cloned()
    .collect()
}

/// Verdicts for every citation an answer referenced.
#[derive(Debug, Clone)]
pub struct CitationReport {
  pub verdicts: Vec<(Citation, CitationVerdict)>,
}

impl CitationReport {
  pub fn all_supported(&self) -> bool {
    self
      .verdicts
      .iter()
      .all(|(_, v)| matches!(v, CitationVerdict::Supported))
  }

  /// Markers of every referenced citation — the full set to strip on
  /// downgrade, not just the unsupported ones (see module docs for why).
  pub fn markers(&self) -> Vec<&str> {
    self
      .verdicts
      .iter()
      .map(|(c, _)| c.marker.as_str())
      .collect()
  }
}

/// Locates the most recent non-error `rag_search` tool result in `steps`,
/// parses its citations, filters to the ones `answer` actually references,
/// and runs them through `checker`.
///
/// Returns `Ok(None)` when there's no `rag_search` result to check against,
/// or the answer references none of its citations — both "nothing to
/// verify," not a failure.
pub async fn verify_citations(
  steps: &[AgentStep],
  answer: &str,
  checker: &dyn CitationChecker,
) -> Result<Option<CitationReport>, CitationCheckError> {
  let Some(rag_result) = steps.iter().rev().find_map(|step| match &step.kind {
    AgentStepKind::ToolResult {
      tool,
      content,
      is_error,
      ..
    } if tool == "rag_search" && !is_error => Some(content.as_str()),
    _ => None,
  }) else {
    return Ok(None);
  };

  let all_citations = parse_citations_from_tool_result(rag_result);
  let referenced = citations_referenced_in_answer(answer, &all_citations);
  if referenced.is_empty() {
    return Ok(None);
  }

  let verdicts = checker.check(answer, &referenced).await?;
  if verdicts.len() != referenced.len() {
    return Err(CitationCheckError::Failed {
      message: format!(
        "CitationChecker `{}` returned {} verdicts for {} citations",
        checker.name(),
        verdicts.len(),
        referenced.len()
      ),
    });
  }

  Ok(Some(CitationReport {
    verdicts: referenced.into_iter().zip(verdicts).collect(),
  }))
}

/// Strips every marker in `markers` from `answer`, then collapses the
/// whitespace the removal leaves behind. Produces the "no-citation answer"
/// an unsupported [`CitationReport`] downgrades to.
pub fn downgrade_answer(answer: &str, markers: &[&str]) -> String {
  let mut out = answer.to_string();
  for marker in markers {
    out = out.replace(marker, "");
  }
  out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Deterministic, dependency-free checker: judges a citation `Supported`
/// when its content shares at least `min_overlap_ratio` of its
/// (lowercased, length > 3) words with the answer. A cheap proxy for
/// "does this passage actually relate to what the answer says" — not a
/// semantic judge, but a real, testable first line of defense, and the
/// default this module's tests use.
pub struct KeywordOverlapCitationChecker {
  pub min_overlap_ratio: f32,
}

impl Default for KeywordOverlapCitationChecker {
  fn default() -> Self {
    Self {
      min_overlap_ratio: 0.15,
    }
  }
}

fn tokenize(text: &str) -> HashSet<String> {
  text
    .to_ascii_lowercase()
    .split(|c: char| !c.is_alphanumeric())
    .filter(|w| w.len() > 3)
    .map(|w| w.to_string())
    .collect()
}

#[async_trait]
impl CitationChecker for KeywordOverlapCitationChecker {
  async fn check(
    &self,
    answer: &str,
    citations: &[Citation],
  ) -> Result<Vec<CitationVerdict>, CitationCheckError> {
    let answer_words = tokenize(answer);
    Ok(
      citations
        .iter()
        .map(|c| {
          let content_words = tokenize(&c.content);
          if content_words.is_empty() {
            return CitationVerdict::Unsupported {
              reason: "cited passage is empty".to_string(),
            };
          }
          let overlap = content_words.intersection(&answer_words).count();
          let ratio = overlap as f32 / content_words.len() as f32;
          if ratio >= self.min_overlap_ratio {
            CitationVerdict::Supported
          } else {
            CitationVerdict::Unsupported {
              reason: format!(
                "keyword overlap {ratio:.2} below threshold {:.2}",
                self.min_overlap_ratio
              ),
            }
          }
        })
        .collect(),
    )
  }

  fn name(&self) -> &str {
    "keyword_overlap"
  }
}

#[derive(Deserialize)]
struct JudgeResponse {
  verdicts: Vec<JudgeVerdict>,
}

#[derive(Deserialize)]
struct JudgeVerdict {
  marker: String,
  supported: bool,
  reason: String,
}

/// The "lightweight LLM-as-judge" the TODO calls for: one structured-output
/// call per [`check`](CitationChecker::check) invocation, judging every
/// referenced citation against the answer text in one round trip.
pub struct LlmCitationChecker {
  model: String,
}

impl LlmCitationChecker {
  pub fn new(model: impl Into<String>) -> Self {
    Self {
      model: model.into(),
    }
  }
}

#[async_trait]
impl CitationChecker for LlmCitationChecker {
  async fn check(
    &self,
    answer: &str,
    citations: &[Citation],
  ) -> Result<Vec<CitationVerdict>, CitationCheckError> {
    if citations.is_empty() {
      return Ok(Vec::new());
    }
    let citations_block = citations
      .iter()
      .map(|c| {
        format!(
          "{} (source: {}):\n{}",
          c.marker,
          c.source.as_deref().unwrap_or("unknown"),
          c.content
        )
      })
      .collect::<Vec<_>>()
      .join("\n\n");
    let prompt = format!(
      "Answer:\n{answer}\n\nCited passages:\n{citations_block}\n\nFor each citation marker, judge \
       whether the cited passage actually supports the claim the answer makes near that marker. \
       Be strict: unrelated or only-tangentially-related passages are NOT supported."
    );
    let schema = json!({
      "type": "object",
      "properties": {
        "verdicts": {
          "type": "array",
          "items": {
            "type": "object",
            "properties": {
              "marker": { "type": "string" },
              "supported": { "type": "boolean" },
              "reason": { "type": "string" }
            },
            "required": ["marker", "supported", "reason"]
          }
        }
      },
      "required": ["verdicts"]
    });

    let raw = AgentFlow::model(&self.model)
      .prompt(&prompt)
      .json_schema("citation_verdicts", schema)
      .execute()
      .await
      .map_err(|e| CitationCheckError::Failed {
        message: e.to_string(),
      })?;
    let parsed: JudgeResponse =
      serde_json::from_str(&raw).map_err(|e| CitationCheckError::Failed {
        message: format!("failed to parse citation judge response: {e}"),
      })?;

    // Fail closed: a marker the judge didn't return a verdict for is
    // Unsupported, not silently passed.
    let by_marker: HashMap<String, JudgeVerdict> = parsed
      .verdicts
      .into_iter()
      .map(|v| (v.marker.clone(), v))
      .collect();
    Ok(
      citations
        .iter()
        .map(|c| match by_marker.get(&c.marker) {
          Some(v) if v.supported => CitationVerdict::Supported,
          Some(v) => CitationVerdict::Unsupported {
            reason: v.reason.clone(),
          },
          None => CitationVerdict::Unsupported {
            reason: "judge returned no verdict for this citation".to_string(),
          },
        })
        .collect(),
    )
  }

  fn name(&self) -> &str {
    "llm_judge"
  }
}

/// Aggregate "Citation Accuracy" metric (L4.4): the fraction of checked
/// citations judged [`CitationVerdict::Supported`] across however many
/// [`CitationReport`]s the caller [`record`](Self::record)s — e.g. every
/// citation check a `rag eval`-style harness run performs over a batch of
/// (query, generated-answer) cases.
#[derive(Debug, Clone, Default)]
pub struct CitationAccuracyReport {
  pub total: usize,
  pub supported: usize,
}

impl CitationAccuracyReport {
  pub fn record(&mut self, report: &CitationReport) {
    for (_, verdict) in &report.verdicts {
      self.total += 1;
      if matches!(verdict, CitationVerdict::Supported) {
        self.supported += 1;
      }
    }
  }

  /// `supported / total`, or `1.0` when nothing was checked (vacuously
  /// accurate — matches how `EvalReport`'s macro-averages treat an empty
  /// denominator elsewhere in this workspace's eval code).
  pub fn accuracy(&self) -> f64 {
    if self.total == 0 {
      1.0
    } else {
      self.supported as f64 / self.total as f64
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;

  fn tool_result_step(index: usize, content: &str) -> AgentStep {
    AgentStep {
      index,
      kind: AgentStepKind::ToolResult {
        tool: "rag_search".to_string(),
        content: content.to_string(),
        is_error: false,
        parts: vec![],
      },
      timestamp: Utc::now(),
      duration_ms: None,
    }
  }

  const RENDERED: &str = "[1] (source: docs/rust.md, score: 0.812)\nRust ownership moves values on assignment.\n\n[2] (source: docs/python.md, score: 0.400)\nPython uses reference counting for memory management.";

  #[test]
  fn parses_citations_from_rendered_tool_result() {
    let citations = parse_citations_from_tool_result(RENDERED);
    assert_eq!(citations.len(), 2);
    assert_eq!(citations[0].marker, "[1]");
    assert_eq!(citations[0].source.as_deref(), Some("docs/rust.md"));
    assert_eq!(
      citations[0].content,
      "Rust ownership moves values on assignment."
    );
    assert_eq!(citations[1].marker, "[2]");
  }

  #[test]
  fn filters_to_markers_the_answer_actually_uses() {
    let citations = parse_citations_from_tool_result(RENDERED);
    let referenced = citations_referenced_in_answer("Ownership moves values [1].", &citations);
    assert_eq!(referenced.len(), 1);
    assert_eq!(referenced[0].marker, "[1]");
  }

  #[tokio::test]
  async fn keyword_overlap_checker_supports_relevant_citation() {
    let checker = KeywordOverlapCitationChecker::default();
    let citations = vec![Citation {
      marker: "[1]".to_string(),
      source: Some("docs/rust.md".to_string()),
      content: "Rust ownership moves values on assignment".to_string(),
    }];
    let verdicts = checker
      .check("Rust ownership moves values [1].", &citations)
      .await
      .expect("check ok");
    assert_eq!(verdicts, vec![CitationVerdict::Supported]);
  }

  #[tokio::test]
  async fn keyword_overlap_checker_rejects_unrelated_citation() {
    let checker = KeywordOverlapCitationChecker::default();
    let citations = vec![Citation {
      marker: "[1]".to_string(),
      source: Some("docs/python.md".to_string()),
      content: "Python uses reference counting for memory management".to_string(),
    }];
    let verdicts = checker
      .check("Rust ownership moves values on assignment [1].", &citations)
      .await
      .expect("check ok");
    assert!(matches!(verdicts[0], CitationVerdict::Unsupported { .. }));
  }

  #[tokio::test]
  async fn verify_citations_finds_the_most_recent_rag_search_result() {
    let steps = vec![
      tool_result_step(
        0,
        "[1] (source: stale.md, score: 0.5)\nstale unrelated content",
      ),
      tool_result_step(1, RENDERED),
    ];
    let checker = KeywordOverlapCitationChecker::default();
    let report = verify_citations(&steps, "Rust ownership moves values [1].", &checker)
      .await
      .expect("verify ok")
      .expect("a report, since the answer references a citation");
    assert_eq!(report.verdicts.len(), 1);
    assert!(report.all_supported());
  }

  #[tokio::test]
  async fn verify_citations_returns_none_when_answer_cites_nothing() {
    let steps = vec![tool_result_step(0, RENDERED)];
    let checker = KeywordOverlapCitationChecker::default();
    let report = verify_citations(&steps, "An answer with no markers at all.", &checker)
      .await
      .expect("verify ok");
    assert!(report.is_none());
  }

  #[tokio::test]
  async fn verify_citations_returns_none_when_no_rag_search_step_exists() {
    let steps = vec![AgentStep {
      index: 0,
      kind: AgentStepKind::ToolResult {
        tool: "shell".to_string(),
        content: "some output".to_string(),
        is_error: false,
        parts: vec![],
      },
      timestamp: Utc::now(),
      duration_ms: None,
    }];
    let checker = KeywordOverlapCitationChecker::default();
    let report = verify_citations(&steps, "answer [1]", &checker)
      .await
      .expect("verify ok");
    assert!(report.is_none());
  }

  #[tokio::test]
  async fn verify_citations_ignores_a_failed_rag_search_step() {
    let mut failed = tool_result_step(0, RENDERED);
    failed.kind = AgentStepKind::ToolResult {
      tool: "rag_search".to_string(),
      content: RENDERED.to_string(),
      is_error: true,
      parts: vec![],
    };
    let checker = KeywordOverlapCitationChecker::default();
    let report = verify_citations(&[failed], "Rust ownership [1]", &checker)
      .await
      .expect("verify ok");
    assert!(
      report.is_none(),
      "an error tool result must not be treated as citable"
    );
  }

  #[test]
  fn downgrade_strips_all_referenced_markers_not_just_unsupported() {
    let downgraded = downgrade_answer(
      "Rust ownership moves values [1]. Python uses refcounting [2].",
      &["[1]", "[2]"],
    );
    assert!(!downgraded.contains('['));
    assert!(downgraded.contains("Rust ownership"));
    assert!(downgraded.contains("Python uses refcounting"));
  }

  #[tokio::test]
  async fn citation_report_all_supported_is_false_with_one_unsupported() {
    let checker = KeywordOverlapCitationChecker::default();
    let citations = vec![
      Citation {
        marker: "[1]".to_string(),
        source: None,
        content: "Rust ownership moves values on assignment".to_string(),
      },
      Citation {
        marker: "[2]".to_string(),
        source: None,
        content: "totally unrelated passage about gardening".to_string(),
      },
    ];
    let verdicts = checker
      .check(
        "Rust ownership moves values [1] and something else [2].",
        &citations,
      )
      .await
      .expect("check ok");
    let report = CitationReport {
      verdicts: citations.into_iter().zip(verdicts).collect(),
    };
    assert!(!report.all_supported());
    assert_eq!(
      report.markers().len(),
      2,
      "markers() returns ALL referenced, not just unsupported"
    );
  }

  #[tokio::test]
  async fn citation_accuracy_report_aggregates_across_multiple_reports() {
    let mut accuracy = CitationAccuracyReport::default();
    accuracy.record(&CitationReport {
      verdicts: vec![
        (
          Citation {
            marker: "[1]".to_string(),
            source: None,
            content: String::new(),
          },
          CitationVerdict::Supported,
        ),
        (
          Citation {
            marker: "[2]".to_string(),
            source: None,
            content: String::new(),
          },
          CitationVerdict::Unsupported {
            reason: "x".to_string(),
          },
        ),
      ],
    });
    accuracy.record(&CitationReport {
      verdicts: vec![(
        Citation {
          marker: "[1]".to_string(),
          source: None,
          content: String::new(),
        },
        CitationVerdict::Supported,
      )],
    });
    assert_eq!(accuracy.total, 3);
    assert_eq!(accuracy.supported, 2);
    assert!((accuracy.accuracy() - (2.0 / 3.0)).abs() < 1e-9);
  }

  #[test]
  fn citation_accuracy_report_is_vacuously_1_0_when_empty() {
    let accuracy = CitationAccuracyReport::default();
    assert_eq!(accuracy.accuracy(), 1.0);
  }

  /// A scorer that returns the wrong number of verdicts must surface a
  /// loud error from `verify_citations`, not silently misalign.
  #[tokio::test]
  async fn mismatched_verdict_count_is_a_loud_error() {
    struct WrongCountChecker;
    #[async_trait]
    impl CitationChecker for WrongCountChecker {
      async fn check(
        &self,
        _answer: &str,
        _citations: &[Citation],
      ) -> Result<Vec<CitationVerdict>, CitationCheckError> {
        Ok(vec![CitationVerdict::Supported])
      }
      fn name(&self) -> &str {
        "wrong_count"
      }
    }
    let steps = vec![tool_result_step(0, RENDERED)];
    let err = verify_citations(
      &steps,
      "Rust ownership [1] and Python refcounting [2].",
      &WrongCountChecker,
    )
    .await
    .expect_err("mismatched verdict count must error");
    assert!(matches!(err, CitationCheckError::Failed { .. }));
  }
}
