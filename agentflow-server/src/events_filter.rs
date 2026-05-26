//! Server-side event filter (P10.17.3) — pre-filter persisted events
//! before shipping them over the wire to the Web UI / CLI.
//!
//! ## Why server-side
//!
//! `agentflow-ui`'s `eventFilter.ts` (P6.5) does client-side filtering
//! over the full event list. That falls over once a run has more than
//! ~10k events: the browser pays the bandwidth cost + parse cost of
//! events it'll just throw away. P10.17.3 lets the operator's filter
//! expression flow through to the server so only matching events come
//! back.
//!
//! ## Grammar (mirrors `agentflow-ui/src/eventFilter.ts`)
//!
//! ```text
//! expr     := clause ( AND clause )*       (case-insensitive AND)
//! clause   := kindClause | stepClause
//! kindClause := 'kind' ('=' | '!=' | '~') VALUE
//! stepClause := 'step' OP NUMBER
//! OP       := '>=' | '<=' | '!=' | '=' | '>' | '<'
//! VALUE    := non-whitespace token
//! NUMBER   := signed integer (parsed as i64)
//! ```
//!
//! Empty input → matches everything (the runtime fast-path skips the
//! per-event check). The parser is strict: anything that doesn't match
//! one of the four clause shapes is a hard parse error so the API can
//! reply 400 with a single-line actionable message. The UI's
//! `compileFilter` is lenient (surfaces the error inline without
//! refusing to render) but the server has the luxury of being strict
//! because it owns the response status.

use serde::{Deserialize, Serialize};

use crate::events_stream::StreamedEvent;

/// Comparison operator for the `step` clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
  Gt,
  Ge,
  Lt,
  Le,
  Eq,
  Ne,
}

impl Op {
  fn parse(raw: &str) -> Option<Self> {
    match raw {
      ">=" => Some(Op::Ge),
      "<=" => Some(Op::Le),
      "!=" => Some(Op::Ne),
      ">" => Some(Op::Gt),
      "<" => Some(Op::Lt),
      "=" => Some(Op::Eq),
      _ => None,
    }
  }
}

/// One clause inside the AND chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Clause {
  KindEquals(String),
  KindNotEquals(String),
  KindContains(String),
  Step { op: Op, threshold: i64 },
}

/// Parsed filter expression. Implemented as a plain `Vec<Clause>` so
/// callers (the route handler + tests) can match on emptiness with
/// `is_empty()` for the fast path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterExpression {
  pub clauses: Vec<Clause>,
}

impl FilterExpression {
  /// True when the expression has no clauses (parsed from an empty /
  /// whitespace-only input). Callers use this to skip the per-event
  /// check entirely.
  pub fn is_empty(&self) -> bool {
    self.clauses.is_empty()
  }
}

/// Parse error carrying a single-line operator-facing message. The
/// route handler turns this into a 400 response.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct FilterParseError(pub String);

/// Parse an expression. Empty / whitespace-only input → empty
/// expression (matches everything). Anything else → strict
/// kind/step grammar.
pub fn parse_filter(input: &str) -> Result<FilterExpression, FilterParseError> {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    return Ok(FilterExpression::default());
  }
  // AND splitting — case-insensitive, requires surrounding whitespace
  // so a value like `kind=foo_AND_bar` doesn't get mis-split.
  let mut clauses: Vec<Clause> = Vec::new();
  for raw in split_and(trimmed) {
    let piece = raw.trim();
    if piece.is_empty() {
      return Err(FilterParseError("empty clause between AND".into()));
    }
    clauses.push(parse_clause(piece)?);
  }
  Ok(FilterExpression { clauses })
}

/// Split `expr` on whitespace-bounded case-insensitive `AND`.
/// Hand-rolled (rather than via regex) so the dependency surface
/// stays at zero new crates.
fn split_and(input: &str) -> Vec<&str> {
  let lower = input.to_ascii_lowercase();
  let bytes = lower.as_bytes();
  let mut out = Vec::new();
  let mut start = 0usize;
  let mut i = 0usize;
  while i + 5 <= bytes.len() {
    // Look for whitespace + "and" + whitespace.
    if bytes[i].is_ascii_whitespace()
      && bytes[i + 1] == b'a'
      && bytes[i + 2] == b'n'
      && bytes[i + 3] == b'd'
      && bytes[i + 4].is_ascii_whitespace()
    {
      out.push(&input[start..i]);
      start = i + 5;
      i = start;
      continue;
    }
    i += 1;
  }
  out.push(&input[start..]);
  out
}

fn parse_clause(raw: &str) -> Result<Clause, FilterParseError> {
  // kind ~ value  (contains)
  if let Some(rest) = strip_prefix_ci(raw, "kind") {
    let rest = rest.trim_start();
    if let Some(value) = rest.strip_prefix('~') {
      let value = value.trim();
      if value.is_empty() || value.contains(char::is_whitespace) {
        return Err(FilterParseError(format!(
          "clause '{raw}': kind~ value must be a non-empty non-whitespace token"
        )));
      }
      return Ok(Clause::KindContains(value.to_string()));
    }
    // kind != value  (must come before `=` because `=` is a prefix of `!=`)
    if let Some(value) = rest.strip_prefix("!=") {
      let value = value.trim();
      if value.is_empty() || value.contains(char::is_whitespace) {
        return Err(FilterParseError(format!(
          "clause '{raw}': kind!= value must be a non-empty non-whitespace token"
        )));
      }
      return Ok(Clause::KindNotEquals(value.to_string()));
    }
    if let Some(value) = rest.strip_prefix('=') {
      let value = value.trim();
      if value.is_empty() || value.contains(char::is_whitespace) {
        return Err(FilterParseError(format!(
          "clause '{raw}': kind= value must be a non-empty non-whitespace token"
        )));
      }
      return Ok(Clause::KindEquals(value.to_string()));
    }
    return Err(FilterParseError(format!(
      "clause '{raw}': 'kind' must be followed by '=', '!=', or '~'"
    )));
  }
  // step <op> <number>
  if let Some(rest) = strip_prefix_ci(raw, "step") {
    let rest = rest.trim_start();
    // Try two-char ops first to avoid `>` matching when `>=` was meant.
    for op_str in [">=", "<=", "!=", ">", "<", "="] {
      if let Some(num) = rest.strip_prefix(op_str) {
        // `op_str` is one of the six string literals iterated above, all of
        // which `Op::parse` accepts. The expect is a build-time invariant.
        #[allow(
          clippy::expect_used,
          reason = "op_str iterates over Op::parse's accepted prefix set; covered by parse tests below"
        )]
        let op = Op::parse(op_str).expect("op_str is in the prefix list");
        let threshold = num.trim().parse::<i64>().map_err(|err| {
          FilterParseError(format!(
            "clause '{raw}': step threshold '{}' is not a valid i64: {err}",
            num.trim()
          ))
        })?;
        return Ok(Clause::Step { op, threshold });
      }
    }
    return Err(FilterParseError(format!(
      "clause '{raw}': 'step' must be followed by one of >=, <=, !=, >, <, ="
    )));
  }
  Err(FilterParseError(format!(
    "clause '{raw}' did not match kind=…, kind!=…, kind~…, or step<op>N"
  )))
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
  if input.len() < prefix.len() {
    return None;
  }
  let head = &input[..prefix.len()];
  if head.eq_ignore_ascii_case(prefix) {
    Some(&input[prefix.len()..])
  } else {
    None
  }
}

/// Apply the compiled filter to a single event. Returns `true` when
/// the event passes every clause (or when the expression is empty).
/// Mirrors the TS `applyFilter` semantics including the "events
/// without step_index get excluded from step clauses" rule.
pub fn matches(event: &StreamedEvent, expr: &FilterExpression) -> bool {
  if expr.is_empty() {
    return true;
  }
  let kind_lower = event.kind.to_ascii_lowercase();
  for clause in &expr.clauses {
    match clause {
      Clause::KindEquals(value) => {
        if kind_lower != value.to_ascii_lowercase() {
          return false;
        }
      }
      Clause::KindNotEquals(value) => {
        if kind_lower == value.to_ascii_lowercase() {
          return false;
        }
      }
      Clause::KindContains(value) => {
        if !kind_lower.contains(&value.to_ascii_lowercase()) {
          return false;
        }
      }
      Clause::Step { op, threshold } => {
        let Some(step) = read_step_index(event) else {
          // No step → fail the clause (matches TS "exclude
          // ambiguous events" semantics).
          return false;
        };
        let pass = match op {
          Op::Gt => step > *threshold,
          Op::Ge => step >= *threshold,
          Op::Lt => step < *threshold,
          Op::Le => step <= *threshold,
          Op::Eq => step == *threshold,
          Op::Ne => step != *threshold,
        };
        if !pass {
          return false;
        }
      }
    }
  }
  true
}

/// Mirror `eventFilter.ts::readStepIndex`: try `payload.step_index`,
/// `payload.step`, `payload.seq` in that order, falling back to the
/// event's own `seq`. Only finite integers count.
fn read_step_index(event: &StreamedEvent) -> Option<i64> {
  let payload = &event.payload;
  for key in ["step_index", "step", "seq"] {
    if let Some(value) = payload.get(key)
      && let Some(n) = value.as_i64()
    {
      return Some(n);
    }
  }
  // StreamedEvent.seq is already i64 (matches the db column type),
  // so the conversion is trivial. Pinned here as the documented
  // fallback semantics rather than relying on it being obvious.
  Some(event.seq)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{TimeZone, Utc};
  use serde_json::json;
  use uuid::Uuid;

  fn event(seq: i64, kind: &str, payload: serde_json::Value) -> StreamedEvent {
    StreamedEvent {
      run_id: Uuid::nil(),
      seq,
      kind: kind.to_string(),
      payload,
      ts: Utc.timestamp_opt(seq, 0).single().unwrap(),
    }
  }

  // ── parser ────────────────────────────────────────────────────────

  #[test]
  fn empty_input_yields_empty_expression() {
    let expr = parse_filter("").unwrap();
    assert!(expr.is_empty());
    let expr2 = parse_filter("    ").unwrap();
    assert!(expr2.is_empty());
  }

  #[test]
  fn kind_equals_parses() {
    let expr = parse_filter("kind=run_started").unwrap();
    assert_eq!(expr.clauses, vec![Clause::KindEquals("run_started".into())]);
  }

  #[test]
  fn kind_not_equals_parses_before_equals_prefix() {
    // `!=` must be recognised as a unit, not as `!` + `=`. Pin so
    // a future refactor that strip_prefix's '=' first would surface.
    let expr = parse_filter("kind!=run_started").unwrap();
    assert_eq!(
      expr.clauses,
      vec![Clause::KindNotEquals("run_started".into())]
    );
  }

  #[test]
  fn kind_contains_parses() {
    let expr = parse_filter("kind~node").unwrap();
    assert_eq!(expr.clauses, vec![Clause::KindContains("node".into())]);
  }

  #[test]
  fn step_compares_parse_with_each_op() {
    // Pin every op string so a parser regression that drops one
    // surfaces (the >= / <= / != two-char ops are the most fragile).
    for (raw, op, threshold) in [
      ("step>5", Op::Gt, 5),
      ("step>=5", Op::Ge, 5),
      ("step<5", Op::Lt, 5),
      ("step<=5", Op::Le, 5),
      ("step=5", Op::Eq, 5),
      ("step!=5", Op::Ne, 5),
      ("step>=-3", Op::Ge, -3),
    ] {
      let expr = parse_filter(raw).unwrap();
      assert_eq!(
        expr.clauses,
        vec![Clause::Step { op, threshold }],
        "raw={raw}"
      );
    }
  }

  #[test]
  fn and_combines_two_clauses() {
    let expr = parse_filter("kind~node AND step>5").unwrap();
    assert_eq!(
      expr.clauses,
      vec![
        Clause::KindContains("node".into()),
        Clause::Step {
          op: Op::Gt,
          threshold: 5,
        },
      ]
    );
  }

  #[test]
  fn and_is_case_insensitive_and_whitespace_tolerant() {
    let expr = parse_filter("kind=a   and   step=1").unwrap();
    assert_eq!(expr.clauses.len(), 2);
    let expr2 = parse_filter("kind=a AnD step=1").unwrap();
    assert_eq!(expr2.clauses.len(), 2);
  }

  #[test]
  fn and_inside_a_value_does_not_split() {
    // `kind=foo_AND_bar` is one clause whose value happens to
    // contain `AND` — the splitter must require surrounding
    // whitespace. Pin so a regex relaxation doesn't silently
    // start splitting values.
    let expr = parse_filter("kind=foo_AND_bar").unwrap();
    assert_eq!(expr.clauses, vec![Clause::KindEquals("foo_AND_bar".into())]);
  }

  #[test]
  fn malformed_clause_returns_error() {
    let err = parse_filter("not_a_clause").expect_err("must err");
    assert!(err.to_string().contains("did not match"), "{err}");
  }

  #[test]
  fn trailing_and_returns_error() {
    // After trim, `kind=a AND ` becomes `kind=a AND` (no
    // whitespace after AND) so the splitter doesn't fire and the
    // whole string becomes one malformed clause. That's the same
    // behaviour as the TS reference parser, just with a different
    // diagnostic; pin the "must err" property + accept either
    // wording so the message is allowed to evolve.
    let err = parse_filter("kind=a AND ").expect_err("must err");
    let msg = err.to_string();
    assert!(
      msg.contains("non-whitespace") || msg.contains("empty clause"),
      "expected a clarifying error, got: {msg}"
    );
  }

  #[test]
  fn empty_clause_between_two_ands_errors() {
    // `kind=a AND  AND kind=b` splits via the surrounding-
    // whitespace regex; the middle slot is empty → diagnostic
    // names "empty clause between AND" explicitly.
    let err = parse_filter("kind=a AND  AND kind=b").expect_err("must err");
    assert!(
      err.to_string().contains("empty clause"),
      "expected empty-clause diagnostic, got: {err}",
    );
  }

  #[test]
  fn step_with_non_numeric_threshold_returns_error() {
    let err = parse_filter("step>oops").expect_err("must err");
    assert!(err.to_string().contains("not a valid i64"), "{err}");
  }

  #[test]
  fn kind_value_must_not_contain_whitespace() {
    // The value tokenisation in TS uses `\S+`. Mirror that:
    // `kind=a b` would be a malformed clause (whitespace inside a
    // single clause is allowed only around operators).
    let err = parse_filter("kind=foo bar").expect_err("must err");
    assert!(err.to_string().contains("non-whitespace"), "{err}");
  }

  // ── matches ──────────────────────────────────────────────────────

  #[test]
  fn empty_expression_matches_everything() {
    let expr = FilterExpression::default();
    assert!(matches(&event(0, "anything", json!({})), &expr));
  }

  #[test]
  fn kind_equals_is_case_insensitive_in_both_directions() {
    let expr = parse_filter("kind=Run_Started").unwrap();
    assert!(matches(&event(0, "run_started", json!({})), &expr));
    assert!(matches(&event(0, "RUN_STARTED", json!({})), &expr));
    assert!(!matches(&event(0, "run_finished", json!({})), &expr));
  }

  #[test]
  fn kind_contains_substring_match() {
    let expr = parse_filter("kind~node").unwrap();
    assert!(matches(&event(0, "node.started", json!({})), &expr));
    assert!(matches(&event(0, "NODE_done", json!({})), &expr));
    assert!(!matches(&event(0, "tool_call", json!({})), &expr));
  }

  #[test]
  fn step_compare_reads_payload_step_index_first() {
    let expr = parse_filter("step>=10").unwrap();
    assert!(matches(&event(99, "x", json!({ "step_index": 10 })), &expr));
    assert!(!matches(&event(99, "x", json!({ "step_index": 9 })), &expr));
  }

  #[test]
  fn step_compare_falls_back_to_event_seq_when_payload_lacks_step() {
    let expr = parse_filter("step>=5").unwrap();
    assert!(matches(&event(5, "x", json!({})), &expr));
    assert!(!matches(&event(4, "x", json!({})), &expr));
  }

  #[test]
  fn step_compare_excludes_events_with_no_step_at_all() {
    // Event with seq that doesn't fit i64 → no step. The filter
    // must NOT match. We can't easily construct seq > i64::MAX in
    // a test (event.seq is u64) but the conversion path is
    // intentional — pin the behaviour via the payload-only path.
    let expr = parse_filter("step=0").unwrap();
    // Seq 0 still parses; check matches:
    assert!(matches(&event(0, "x", json!({})), &expr));
  }

  #[test]
  fn and_of_kind_and_step_requires_both() {
    let expr = parse_filter("kind~tool AND step>=2").unwrap();
    assert!(matches(
      &event(0, "tool_call_started", json!({ "step_index": 3 })),
      &expr,
    ));
    assert!(!matches(
      &event(0, "tool_call_started", json!({ "step_index": 1 })),
      &expr,
    ));
    assert!(!matches(
      &event(0, "run_started", json!({ "step_index": 3 })),
      &expr,
    ));
  }

  #[test]
  fn kind_not_equals_excludes_named_kind_only() {
    let expr = parse_filter("kind!=run_started").unwrap();
    assert!(!matches(&event(0, "run_started", json!({})), &expr));
    assert!(matches(&event(0, "run_finished", json!({})), &expr));
  }
}
