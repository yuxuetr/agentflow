//! # Thinking / extended-reasoning configuration
//!
//! Unified abstraction over the model-side "slow think before acting"
//! knob that each provider exposes in a different shape:
//!
//! - **Anthropic** (`claude-3.7-*`, `claude-sonnet-4-*`, `claude-opus-4-*`):
//!   `thinking: { type: "enabled", budget_tokens: N }` on the request body.
//! - **OpenAI o-series** (`o1-*`, `o3-*`, `o4-*`): `reasoning_effort:
//!   "minimal" | "low" | "medium" | "high"` on the request body.
//! - **Google Gemini 2.5+**: `generationConfig.thinkingConfig.thinkingBudget:
//!   N` (a token budget in the model's reasoning space).
//! - **DeepSeek-R1** (OpenAI-compat): no input knob; the model returns
//!   `reasoning_content` alongside `content`. AgentFlow surfaces that via
//!   [`crate::providers::ProviderResponse::thinking`] /
//!   [`crate::LLMResponse::thinking`].
//!
//! The high-level intent (`Auto` / `Low` / `Medium` / `High` / `Disabled`)
//! is mapped per provider into the native wire shape. When a caller needs
//! fine-grained control — e.g. an explicit token budget for Anthropic or a
//! provider-specific effort string — the `Budget(u32)` and
//! `Effort(String)` variants are the escape hatch.

use serde::{Deserialize, Serialize};

/// Unified thinking/reasoning configuration for an LLM request.
///
/// Each provider adapter maps this to its native wire shape. Callers should
/// prefer the qualitative variants (`Auto` / `Low` / `Medium` / `High` /
/// `Disabled`) for cross-provider portability; reach for `Budget` or
/// `Effort` only when they need provider-specific tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingConfig {
  /// Enable thinking with the provider's default budget. For Anthropic this
  /// resolves to the model's default `budget_tokens`; for OpenAI it maps to
  /// `reasoning_effort: "medium"`; for Google it omits the explicit
  /// `thinkingBudget` and lets the model pick.
  Auto,
  /// Lightweight thinking. Cross-provider mapping:
  ///   - Anthropic: `budget_tokens: 1024`
  ///   - OpenAI: `reasoning_effort: "low"`
  ///   - Google: `thinkingBudget: 1024`
  Low,
  /// Standard thinking. Cross-provider mapping:
  ///   - Anthropic: `budget_tokens: 4096`
  ///   - OpenAI: `reasoning_effort: "medium"`
  ///   - Google: `thinkingBudget: 4096`
  Medium,
  /// Deep thinking. Cross-provider mapping:
  ///   - Anthropic: `budget_tokens: 16384`
  ///   - OpenAI: `reasoning_effort: "high"`
  ///   - Google: `thinkingBudget: 16384`
  High,
  /// Explicit token budget for providers that accept one (Anthropic, Google).
  /// On OpenAI o-series this is bucketed into the closest `reasoning_effort`
  /// level (≤ 2048 → low, ≤ 8192 → medium, > 8192 → high).
  Budget(u32),
  /// Explicit effort string for providers that accept one (OpenAI o-series:
  /// `"minimal"`, `"low"`, `"medium"`, `"high"`). On Anthropic / Google this
  /// is bucketed into the closest token budget (low → 1024, medium → 4096,
  /// high → 16384; "minimal" → 512).
  Effort(String),
  /// Explicit "thinking off" — used to override a registry-side default
  /// that would otherwise enable thinking.
  Disabled,
}

impl ThinkingConfig {
  /// Whether this config represents an explicit "thinking off" intent.
  pub fn is_disabled(&self) -> bool {
    matches!(self, ThinkingConfig::Disabled)
  }

  /// Resolve to a token budget for providers that accept one
  /// (Anthropic, Google). Returns `None` for `Auto` (provider default
  /// wins) and `Disabled`.
  pub fn to_token_budget(&self) -> Option<u32> {
    match self {
      ThinkingConfig::Auto => None,
      ThinkingConfig::Low => Some(1024),
      ThinkingConfig::Medium => Some(4096),
      ThinkingConfig::High => Some(16384),
      ThinkingConfig::Budget(n) => Some(*n),
      ThinkingConfig::Effort(s) => Some(effort_to_budget(s)),
      ThinkingConfig::Disabled => None,
    }
  }

  /// Resolve to an OpenAI `reasoning_effort` string.
  pub fn to_openai_effort(&self) -> Option<&'static str> {
    match self {
      ThinkingConfig::Auto | ThinkingConfig::Medium => Some("medium"),
      ThinkingConfig::Low => Some("low"),
      ThinkingConfig::High => Some("high"),
      ThinkingConfig::Budget(n) => Some(budget_to_effort(*n)),
      ThinkingConfig::Effort(s) => Some(normalise_effort(s)),
      ThinkingConfig::Disabled => None,
    }
  }
}

/// Map an arbitrary effort string to its closest token budget.
fn effort_to_budget(s: &str) -> u32 {
  match s.to_ascii_lowercase().as_str() {
    "minimal" => 512,
    "low" => 1024,
    "medium" => 4096,
    "high" => 16384,
    _ => 4096,
  }
}

/// Bucket an explicit token budget into the closest OpenAI effort level.
fn budget_to_effort(n: u32) -> &'static str {
  if n <= 1024 {
    "low"
  } else if n <= 8192 {
    "medium"
  } else {
    "high"
  }
}

/// Normalise a caller-supplied effort string to one of the four valid
/// OpenAI values. Unknown strings fall back to `"medium"` — providers reject
/// arbitrary values and a silent silent-fall-through is worse than a
/// predictable bucket.
fn normalise_effort(s: &str) -> &'static str {
  match s.to_ascii_lowercase().as_str() {
    "minimal" => "minimal",
    "low" => "low",
    "medium" => "medium",
    "high" => "high",
    _ => "medium",
  }
}

/// How a model accepts the thinking/reasoning knob, declared in the model
/// registry. Drives which serialisation branch each provider takes.
///
/// Models lacking thinking support should leave `ModelConfig::thinking_kind`
/// unset — setting `.thinking(...)` on such a model errors with
/// [`crate::LLMError::UnsupportedFeature`] before any HTTP call is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingKind {
  /// OpenAI o-series: emits `reasoning_effort` string on the chat completion body.
  Effort,
  /// Anthropic claude-3.7+ / 4.x: emits `thinking: { type: "enabled",
  /// budget_tokens: N }` block.
  BudgetTokens,
  /// Google Gemini 2.5+: emits `generationConfig.thinkingConfig.thinkingBudget`.
  ThinkingConfigBudget,
  /// DeepSeek-R1 and similar: no input knob; the model always returns
  /// `reasoning_content` and we surface it on the response. `.thinking()`
  /// is accepted (and silently produces no wire-level config) so callers
  /// can express "I want the reasoning text" portably.
  OutputOnly,
}

impl ThinkingKind {
  /// Whether this kind accepts caller-supplied input configuration. Used
  /// by the response-side paths to know whether to silently skip wire-level
  /// serialisation (`OutputOnly` returns `false` here, the other three
  /// return `true`).
  pub fn accepts_input(&self) -> bool {
    !matches!(self, ThinkingKind::OutputOnly)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn qualitative_levels_map_to_canonical_budgets() {
    assert_eq!(ThinkingConfig::Low.to_token_budget(), Some(1024));
    assert_eq!(ThinkingConfig::Medium.to_token_budget(), Some(4096));
    assert_eq!(ThinkingConfig::High.to_token_budget(), Some(16384));
    assert_eq!(ThinkingConfig::Auto.to_token_budget(), None);
    assert_eq!(ThinkingConfig::Disabled.to_token_budget(), None);
  }

  #[test]
  fn qualitative_levels_map_to_openai_effort() {
    assert_eq!(ThinkingConfig::Low.to_openai_effort(), Some("low"));
    assert_eq!(ThinkingConfig::Medium.to_openai_effort(), Some("medium"));
    assert_eq!(ThinkingConfig::High.to_openai_effort(), Some("high"));
    assert_eq!(ThinkingConfig::Auto.to_openai_effort(), Some("medium"));
    assert_eq!(ThinkingConfig::Disabled.to_openai_effort(), None);
  }

  #[test]
  fn explicit_budget_buckets_into_correct_openai_effort() {
    assert_eq!(
      ThinkingConfig::Budget(500).to_openai_effort(),
      Some("low"),
      "500 tokens falls into low (≤ 1024)"
    );
    assert_eq!(
      ThinkingConfig::Budget(4096).to_openai_effort(),
      Some("medium"),
      "4096 tokens falls into medium (≤ 8192)"
    );
    assert_eq!(
      ThinkingConfig::Budget(20_000).to_openai_effort(),
      Some("high"),
      "20000 tokens falls into high (> 8192)"
    );
  }

  #[test]
  fn explicit_effort_strings_normalise() {
    assert_eq!(
      ThinkingConfig::Effort("LOW".into()).to_openai_effort(),
      Some("low")
    );
    assert_eq!(
      ThinkingConfig::Effort("minimal".into()).to_openai_effort(),
      Some("minimal")
    );
    // Unknown values fall back to "medium" rather than emitting garbage.
    assert_eq!(
      ThinkingConfig::Effort("intense".into()).to_openai_effort(),
      Some("medium")
    );
  }

  #[test]
  fn explicit_effort_string_maps_to_budget() {
    assert_eq!(
      ThinkingConfig::Effort("minimal".into()).to_token_budget(),
      Some(512)
    );
    assert_eq!(
      ThinkingConfig::Effort("high".into()).to_token_budget(),
      Some(16384)
    );
  }

  #[test]
  fn output_only_kind_does_not_accept_input() {
    assert!(!ThinkingKind::OutputOnly.accepts_input());
    assert!(ThinkingKind::Effort.accepts_input());
    assert!(ThinkingKind::BudgetTokens.accepts_input());
    assert!(ThinkingKind::ThinkingConfigBudget.accepts_input());
  }
}
