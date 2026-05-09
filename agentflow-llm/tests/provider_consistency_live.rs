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

use agentflow_llm::AgentFlow;
use agentflow_llm::providers::{
  AnthropicProvider, ContentType, GoogleProvider, LLMProvider, MoonshotProvider, OpenAIProvider,
  ProviderRequest, StepFunProvider, TokenUsage,
};
use agentflow_llm::tool_calling::StopReason;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveCapability {
  Llm,
  Multimodal,
  Image,
  Audio,
  Video,
}

impl LiveCapability {
  fn gate_env(self) -> &'static str {
    match self {
      Self::Llm => "AGENTFLOW_LIVE_LLM_TESTS",
      Self::Multimodal => "AGENTFLOW_LIVE_MULTIMODAL_TESTS",
      Self::Image => "AGENTFLOW_LIVE_IMAGE_TESTS",
      Self::Audio => "AGENTFLOW_LIVE_AUDIO_TESTS",
      Self::Video => "AGENTFLOW_LIVE_VIDEO_TESTS",
    }
  }
}

#[derive(Debug, Clone, Copy)]
struct LiveProviderProfile {
  name: &'static str,
  capabilities: &'static [LiveCapability],
}

const LIVE_PROVIDER_PROFILES: &[LiveProviderProfile] = &[
  LiveProviderProfile {
    name: "openai",
    capabilities: &[LiveCapability::Llm, LiveCapability::Multimodal],
  },
  LiveProviderProfile {
    name: "anthropic",
    capabilities: &[LiveCapability::Llm, LiveCapability::Multimodal],
  },
  LiveProviderProfile {
    name: "google",
    capabilities: &[LiveCapability::Llm, LiveCapability::Multimodal],
  },
  LiveProviderProfile {
    name: "moonshot",
    capabilities: &[LiveCapability::Llm],
  },
  LiveProviderProfile {
    name: "stepfun",
    capabilities: &[
      LiveCapability::Llm,
      LiveCapability::Multimodal,
      LiveCapability::Image,
      LiveCapability::Audio,
    ],
  },
];

fn provider_supports_capability(provider_name: &str, capability: LiveCapability) -> bool {
  LIVE_PROVIDER_PROFILES
    .iter()
    .find(|profile| profile.name == provider_name)
    .is_some_and(|profile| profile.capabilities.contains(&capability))
}

fn env_truthy(value: &str) -> bool {
  let value = value.trim().to_ascii_lowercase();
  matches!(value.as_str(), "1" | "true" | "yes" | "on")
}

fn live_gate_enabled(capability: LiveCapability) -> bool {
  std::env::var(capability.gate_env())
    .ok()
    .map(|v| env_truthy(&v))
    .unwrap_or(false)
}

async fn prepare_live_provider(provider_name: &str, capability: LiveCapability) -> bool {
  if !provider_supports_capability(provider_name, capability) {
    eprintln!("[live] {provider_name}: skipped ({capability:?} is not marked supported)");
    return false;
  }

  let gate_env = capability.gate_env();
  if !live_gate_enabled(capability) {
    eprintln!("[live] {provider_name}: skipped ({gate_env} not set)");
    return false;
  }

  AgentFlow::init()
    .await
    .unwrap_or_else(|err| panic!("{provider_name}: failed to initialize AgentFlow config: {err}"));
  true
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
fn live_text_model(provider: &str, default: &str) -> String {
  let provider_upper = provider.to_ascii_uppercase();
  let specific = format!("AGENTFLOW_LIVE_{provider_upper}_TEXT_MODEL");
  let legacy = format!("AGENTFLOW_LIVE_{provider_upper}_MODEL");
  for env_var in [&specific, &legacy] {
    if let Ok(value) = std::env::var(env_var) {
      let trimmed = value.trim();
      if !trimmed.is_empty() {
        eprintln!("[live] {provider}: using text model override from {env_var}");
        return trimmed.to_string();
      }
    }
  }

  eprintln!("[live] {provider}: using default text model `{default}` (set {specific} to override)");
  default.to_string()
}

async fn run_text_path<P, F>(provider_name: &str, key_envs: &[&str], build: F)
where
  P: LLMProvider + 'static,
  F: FnOnce(&str) -> P,
{
  if !prepare_live_provider(provider_name, LiveCapability::Llm).await {
    return;
  }
  let Some((env_used, api_key)) = pick_api_key(key_envs) else {
    eprintln!("[live] {provider_name}: skipped (none of {key_envs:?} set; live gate is on)");
    return;
  };
  eprintln!("[live] {provider_name}: using API key from {env_used}");

  let provider = build(&api_key);
  let model = match provider_name {
    "openai" => live_text_model(provider_name, "gpt-4o-mini"),
    "anthropic" => live_text_model(provider_name, "claude-3-5-haiku-20241022"),
    "google" => live_text_model(provider_name, "gemini-1.5-flash"),
    "moonshot" => live_text_model(provider_name, "moonshot-v1-8k"),
    "stepfun" => live_text_model(provider_name, "step-1-8k"),
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
  if std::env::var(LiveCapability::Llm.gate_env()).is_ok() {
    eprintln!("[live] gate is set; skipping the gate-default-off invariant test");
    return;
  }
  assert!(
    !live_gate_enabled(LiveCapability::Llm),
    "live_gate_enabled() must default to false when AGENTFLOW_LIVE_LLM_TESTS is unset"
  );
}

#[test]
fn live_capability_gate_names_are_stable() {
  assert_eq!(LiveCapability::Llm.gate_env(), "AGENTFLOW_LIVE_LLM_TESTS");
  assert_eq!(
    LiveCapability::Multimodal.gate_env(),
    "AGENTFLOW_LIVE_MULTIMODAL_TESTS"
  );
  assert_eq!(
    LiveCapability::Image.gate_env(),
    "AGENTFLOW_LIVE_IMAGE_TESTS"
  );
  assert_eq!(
    LiveCapability::Audio.gate_env(),
    "AGENTFLOW_LIVE_AUDIO_TESTS"
  );
  assert_eq!(
    LiveCapability::Video.gate_env(),
    "AGENTFLOW_LIVE_VIDEO_TESTS"
  );
}

#[test]
fn live_gate_truthy_values_are_explicit() {
  for value in ["1", "true", "TRUE", "yes", "on"] {
    assert!(env_truthy(value), "{value} should enable live tests");
  }
  for value in ["", "0", "false", "no", "off", "please"] {
    assert!(!env_truthy(value), "{value} should not enable live tests");
  }
}

#[test]
fn live_provider_profiles_probe_supported_capabilities() {
  assert!(provider_supports_capability(
    "stepfun",
    LiveCapability::Image
  ));
  assert!(provider_supports_capability(
    "stepfun",
    LiveCapability::Audio
  ));
  assert!(provider_supports_capability(
    "openai",
    LiveCapability::Multimodal
  ));
  assert!(!provider_supports_capability(
    "openai",
    LiveCapability::Video
  ));
  assert!(!provider_supports_capability(
    "unknown",
    LiveCapability::Llm
  ));
}
