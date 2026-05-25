//! End-to-end coverage for the unified `.thinking(...)` API.
//!
//! Verifies the public surface — [`ThinkingConfig`] semantics and the
//! cross-provider mapping contract documented in
//! [`docs/LLM_PROVIDERS_MATRIX.md`]. Per-provider wire serialisation is
//! locked down by unit tests inside each `providers/*.rs` module (see
//! `build_request_body_*` tests there). This file covers what stays in
//! the public API, so callers can rely on it without reaching into
//! `pub(crate)` helpers.

use agentflow_llm::providers::ProviderRequest;
use agentflow_llm::{ThinkingConfig, ThinkingKind};
use serde_json::json;

/// Locks the canonical token budgets backing the qualitative levels so
/// callers can plan request cost with confidence: switching providers
/// for the same `.thinking(ThinkingConfig::Medium)` should not produce
/// surprising 10× swings in reasoning-token spend.
#[test]
fn qualitative_levels_have_stable_token_budgets() {
  assert_eq!(ThinkingConfig::Low.to_token_budget(), Some(1024));
  assert_eq!(ThinkingConfig::Medium.to_token_budget(), Some(4096));
  assert_eq!(ThinkingConfig::High.to_token_budget(), Some(16384));
}

/// `Auto` defers to the provider's default — no caller-side budget.
/// `Disabled` is the explicit "thinking off" override.
#[test]
fn auto_and_disabled_have_no_explicit_token_budget() {
  assert_eq!(ThinkingConfig::Auto.to_token_budget(), None);
  assert_eq!(ThinkingConfig::Disabled.to_token_budget(), None);
}

#[test]
fn qualitative_levels_map_to_canonical_openai_effort() {
  assert_eq!(ThinkingConfig::Low.to_openai_effort(), Some("low"));
  assert_eq!(ThinkingConfig::Medium.to_openai_effort(), Some("medium"));
  assert_eq!(ThinkingConfig::High.to_openai_effort(), Some("high"));
}

/// Explicit `Budget(n)` is the escape hatch for fine-tuning Anthropic /
/// Google token spend; OpenAI buckets it into the closest effort level
/// since the API doesn't accept arbitrary token budgets. The bucket
/// boundaries are tested in `thinking.rs` unit tests; here we just lock
/// the buckets that callers most often hit.
#[test]
fn explicit_budget_buckets_into_openai_effort_predictably() {
  assert_eq!(
    ThinkingConfig::Budget(500).to_openai_effort(),
    Some("low"),
    "small explicit budget → low effort"
  );
  assert_eq!(
    ThinkingConfig::Budget(4096).to_openai_effort(),
    Some("medium"),
    "medium-range budget → medium effort"
  );
  assert_eq!(
    ThinkingConfig::Budget(32_000).to_openai_effort(),
    Some("high"),
    "large budget → high effort"
  );
}

/// Explicit `Effort` (OpenAI's native shape) round-trips to a token
/// budget for Anthropic / Google. The bucket choice is documented in
/// `thinking.rs`; this lock keeps the public contract stable.
#[test]
fn explicit_effort_string_maps_to_token_budget() {
  assert_eq!(
    ThinkingConfig::Effort("minimal".into()).to_token_budget(),
    Some(512)
  );
  assert_eq!(
    ThinkingConfig::Effort("high".into()).to_token_budget(),
    Some(16384)
  );
}

/// `ThinkingKind::OutputOnly` (e.g. DeepSeek-R1) means we accept
/// `.thinking()` on the builder but emit nothing on the request — the
/// model always returns `reasoning_content` regardless. Document this
/// asymmetry so a future refactor that "cleans up" the OutputOnly
/// branch doesn't accidentally start sending DeepSeek a wire field.
#[test]
fn output_only_thinking_kind_does_not_accept_input_serialisation() {
  assert!(!ThinkingKind::OutputOnly.accepts_input());
  // The other three kinds DO accept input — provider adapters serialise
  // them onto the wire.
  assert!(ThinkingKind::Effort.accepts_input());
  assert!(ThinkingKind::BudgetTokens.accepts_input());
  assert!(ThinkingKind::ThinkingConfigBudget.accepts_input());
}

/// Building a `ProviderRequest` with `thinking: Some(...)` works because
/// the field is typed (not stuffed into the `parameters` map). This
/// locks the API shape against a refactor that would push it back
/// through `parameters` and reintroduce the Anthropic/Google
/// whitelist-drop bug.
#[test]
fn provider_request_carries_thinking_as_typed_field() {
  let req = ProviderRequest {
    model: "claude-3-7-sonnet-20250219".to_string(),
    messages: vec![json!({"role": "user", "content": "x"})],
    stream: false,
    parameters: std::collections::HashMap::new(),
    tools: None,
    tool_choice: None,
    thinking: Some(ThinkingConfig::Low),
  };
  match req.thinking {
    Some(ThinkingConfig::Low) => {}
    other => panic!("expected ThinkingConfig::Low, got {other:?}"),
  }
}
