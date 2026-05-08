//! Live LLM cross-provider consistency suite.
//!
//! Companion to `provider_consistency.rs`. The offline file uses hand-rolled
//! TCP mocks to verify wire-format parsing for each provider; this file makes
//! a single real API call per provider with a minimum-cost model and asserts
//! the same cross-provider contract holds against production endpoints:
//!
//!   * `ContentType::Text` is returned and non-empty.
//!   * `TokenUsage` is populated (`prompt_tokens` / `completion_tokens` /
//!     `total_tokens` all present).
//!   * `StopReason::Stop` is reported on a successful single-turn completion.
//!
//! Tests skip cleanly (no failure) when the global gate env var
//! `AGENTFLOW_LIVE_LLM_TESTS` is not set to a truthy value, and per provider
//! when its API key env var is unset. This means default `cargo test` runs
//! never hit the network, while nightly CI can opt in by setting the env vars
//! from secrets. See `docs/LLM_PROVIDERS_MATRIX.md` for the full design.

use std::collections::HashMap;
use std::time::Duration;

use agentflow_llm::providers::{
  AnthropicProvider, ContentType, GoogleProvider, LLMProvider, MoonshotProvider, OpenAIProvider,
  ProviderRequest, StepFunProvider, TokenUsage,
};
use agentflow_llm::tool_calling::StopReason;
use serde_json::json;

const GATE_ENV: &str = "AGENTFLOW_LIVE_LLM_TESTS";

fn live_gate_enabled() -> bool {
  std::env::var(GATE_ENV)
    .ok()
    .map(|v| {
      let v = v.trim().to_ascii_lowercase();
      matches!(v.as_str(), "1" | "true" | "yes" | "on")
    })
    .unwrap_or(false)
}

fn pick_api_key(candidates: &[&str]) -> Option<(String, String)> {
  for name in candidates {
    if let Ok(value) = std::env::var(name) {
      let trimmed = value.trim();
      if !trimmed.is_empty() {
        return Some(((*name).to_string(), trimmed.to_string()));
      }
    }
  }
  None
}

fn provider_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": "Reply with exactly one word: ok"
    })],
    stream: false,
    parameters: HashMap::from([
      ("temperature".to_string(), json!(0.0)),
      ("max_tokens".to_string(), json!(16)),
    ]),
    tools: None,
    tool_choice: None,
  }
}

fn assert_text_non_empty(content: &ContentType) {
  match content {
    ContentType::Text(t) => {
      assert!(
        !t.trim().is_empty(),
        "expected non-empty text content from live provider"
      );
    }
    other => panic!("expected ContentType::Text, got {other:?}"),
  }
}

fn assert_usage_populated(usage: &Option<TokenUsage>, provider: &str) {
  let Some(usage) = usage.as_ref() else {
    panic!("{provider}: expected populated TokenUsage, got None");
  };
  assert!(
    usage.prompt_tokens.is_some(),
    "{provider}: prompt_tokens must be populated"
  );
  assert!(
    usage.completion_tokens.is_some(),
    "{provider}: completion_tokens must be populated"
  );
  assert!(
    usage.total_tokens.is_some(),
    "{provider}: total_tokens must be populated"
  );
}

/// Minimum-cost model per provider for live smoke tests.
///
/// Override via the matching env var when a provider deprecates a model. The
/// defaults are picked for low cost-per-call and broad availability rather
/// than capability.
fn live_model(provider: &str, env_var: &str, default: &str) -> String {
  std::env::var(env_var)
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| {
      eprintln!("[live] {provider}: using default model `{default}` (set {env_var} to override)");
      default.to_string()
    })
}

async fn run_text_path<P, F>(provider_name: &str, key_envs: &[&str], build: F)
where
  P: LLMProvider + 'static,
  F: FnOnce(&str) -> P,
{
  if !live_gate_enabled() {
    eprintln!("[live] {provider_name}: skipped ({GATE_ENV} not set)");
    return;
  }
  let Some((env_used, api_key)) = pick_api_key(key_envs) else {
    eprintln!("[live] {provider_name}: skipped (none of {key_envs:?} set; live gate is on)");
    return;
  };
  eprintln!("[live] {provider_name}: using API key from {env_used}");

  let provider = build(&api_key);
  let model = match provider_name {
    "openai" => live_model(provider_name, "AGENTFLOW_LIVE_OPENAI_MODEL", "gpt-4o-mini"),
    "anthropic" => live_model(
      provider_name,
      "AGENTFLOW_LIVE_ANTHROPIC_MODEL",
      "claude-3-5-haiku-20241022",
    ),
    "google" => live_model(
      provider_name,
      "AGENTFLOW_LIVE_GOOGLE_MODEL",
      "gemini-1.5-flash",
    ),
    "moonshot" => live_model(
      provider_name,
      "AGENTFLOW_LIVE_MOONSHOT_MODEL",
      "moonshot-v1-8k",
    ),
    "stepfun" => live_model(provider_name, "AGENTFLOW_LIVE_STEPFUN_MODEL", "step-1-8k"),
    other => panic!("unknown provider in live harness: {other}"),
  };

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("{provider_name}: live request timed out after 30s"))
  .unwrap_or_else(|e| panic!("{provider_name}: live request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, provider_name);
  assert_eq!(
    response.stop_reason,
    Some(StopReason::Stop),
    "{provider_name}: expected StopReason::Stop on a single-turn text completion"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_live_text_path() {
  run_text_path::<OpenAIProvider, _>("openai", &["OPENAI_API_KEY", "OPENAI_KEY"], |key| {
    OpenAIProvider::new(key, None).expect("openai provider")
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_live_text_path() {
  run_text_path::<AnthropicProvider, _>(
    "anthropic",
    &["ANTHROPIC_API_KEY", "ANTHROPIC_KEY", "CLAUDE_API_KEY"],
    |key| AnthropicProvider::new(key, None).expect("anthropic provider"),
  )
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_live_text_path() {
  run_text_path::<GoogleProvider, _>(
    "google",
    &["GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_AI_KEY"],
    |key| GoogleProvider::new(key, None).expect("google provider"),
  )
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_live_text_path() {
  run_text_path::<MoonshotProvider, _>("moonshot", &["MOONSHOT_API_KEY", "MOONSHOT_KEY"], |key| {
    MoonshotProvider::new(key, None).expect("moonshot provider")
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_text_path() {
  run_text_path::<StepFunProvider, _>("stepfun", &["STEPFUN_API_KEY", "STEP_API_KEY"], |key| {
    StepFunProvider::new(key, None).expect("stepfun provider")
  })
  .await;
}

#[tokio::test]
async fn gate_disabled_by_default() {
  // Belt-and-suspenders: confirm that without the gate env var set, all
  // per-provider tests above would short-circuit before issuing any HTTP
  // request. This guards the offline `cargo test --all-targets` invariant.
  if std::env::var(GATE_ENV).is_ok() {
    eprintln!("[live] gate is set; skipping the gate-default-off invariant test");
    return;
  }
  assert!(
    !live_gate_enabled(),
    "live_gate_enabled() must default to false when {GATE_ENV} is unset"
  );
}
