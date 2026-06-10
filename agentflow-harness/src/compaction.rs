//! Harness-owned context compaction (RFC_HARNESS_LOOP_OWNERSHIP Phase 2,
//! context-engineering half).
//!
//! Phase 0 made the budget trim *truncate-not-drop*; this module closes
//! the remaining gap: when context still overflows the budget, the items
//! that would be dropped are **summarized into a single compact item**
//! instead of vanishing — and that compaction is surfaced as a
//! [`crate::HarnessEventBody::MemorySummaryAdded`] event the harness has
//! advertised since H0 but never produced.
//!
//! The summarizer is pluggable ([`ContextSummarizer`]); the default
//! [`DeterministicContextSummarizer`] is LLM-free and deterministic (so
//! trace replay stays reproducible) — it emits a bulleted index of the
//! dropped sources plus the first line of each. A caller that wants an
//! LLM-quality précis supplies its own implementation.

use async_trait::async_trait;

use crate::context::ContextItem;

/// Strategy that condenses the context items which did not fit the token
/// budget into a single short summary string. Returns `None` to skip
/// compaction (the items are simply dropped, as in Phase 0).
#[async_trait]
pub trait ContextSummarizer: Send + Sync {
  /// Stable identifier used as the `layer` of the emitted
  /// `MemorySummaryAdded` event (e.g. `deterministic`, `llm`).
  fn name(&self) -> &str;

  /// Summarize `dropped` into at most `max_tokens` worth of text.
  /// Implementations must be deterministic for a given input when they
  /// claim to be replay-safe.
  async fn summarize(&self, dropped: &[ContextItem], max_tokens: usize) -> Option<String>;
}

/// LLM-free, deterministic summarizer. Produces a compact index of the
/// dropped items: one bullet per source with its approximate token cost
/// and the first non-empty line of its body. The whole thing is
/// character-bounded by `max_tokens * CHARS_PER_TOKEN_GUESS` so it stays
/// within the reserved budget without needing a tokenizer round-trip.
#[derive(Debug, Default, Clone)]
pub struct DeterministicContextSummarizer;

/// Rough chars-per-token used only to bound the deterministic summary's
/// length; the authoritative recount happens in the runtime when the
/// summary is injected as a context item.
const CHARS_PER_TOKEN_GUESS: usize = 4;

#[async_trait]
impl ContextSummarizer for DeterministicContextSummarizer {
  fn name(&self) -> &str {
    "deterministic"
  }

  async fn summarize(&self, dropped: &[ContextItem], max_tokens: usize) -> Option<String> {
    if dropped.is_empty() {
      return None;
    }
    let char_budget = max_tokens.saturating_mul(CHARS_PER_TOKEN_GUESS).max(64);
    let mut out = format!(
      "Compacted {} lower-priority context item(s) that did not fit the budget:\n",
      dropped.len()
    );
    for item in dropped {
      let head = item
        .content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
      out.push_str(&format!(
        "- {} (~{} tok): {}\n",
        item.source, item.token_estimate, head
      ));
      if out.chars().count() >= char_budget {
        out.push_str("[...summary truncated]\n");
        break;
      }
    }
    // Hard char cap (UTF-8 safe) as a backstop.
    if out.chars().count() > char_budget {
      out = out.chars().take(char_budget).collect();
    }
    Some(out)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::context::ContextPriority;

  fn item(source: &str, content: &str, tokens: usize) -> ContextItem {
    ContextItem {
      source: source.to_owned(),
      priority: ContextPriority::Low,
      token_estimate: tokens,
      content: content.to_owned(),
      metadata: serde_json::Value::Null,
    }
  }

  #[tokio::test]
  async fn deterministic_summary_is_none_for_empty_input() {
    let s = DeterministicContextSummarizer;
    assert!(s.summarize(&[], 100).await.is_none());
  }

  #[tokio::test]
  async fn deterministic_summary_indexes_sources_and_heads() {
    let s = DeterministicContextSummarizer;
    let dropped = vec![
      item("notes_md", "first line of notes\nsecond line", 40),
      item("changelog", "\n\n  v1.2.3 released  \nmore", 30),
    ];
    let summary = s.summarize(&dropped, 200).await.expect("summary");
    assert!(summary.contains("Compacted 2 lower-priority context item(s)"));
    assert!(summary.contains("notes_md"));
    assert!(summary.contains("first line of notes"));
    assert!(summary.contains("changelog"));
    assert!(summary.contains("v1.2.3 released"));
  }

  #[tokio::test]
  async fn deterministic_summary_respects_char_budget() {
    let s = DeterministicContextSummarizer;
    let big = "x".repeat(10_000);
    let dropped: Vec<ContextItem> = (0..50)
      .map(|i| item(&format!("src{i}"), &big, 100))
      .collect();
    let summary = s.summarize(&dropped, 32).await.expect("summary");
    // 32 tok * 4 chars ≈ 128 char budget; allow the final bullet/newline
    // overshoot before the cap kicks in.
    assert!(
      summary.chars().count() <= 32 * CHARS_PER_TOKEN_GUESS,
      "summary {} chars exceeded budget",
      summary.chars().count()
    );
  }
}
