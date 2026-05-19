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
use std::sync::OnceLock;
use std::time::Duration;

use agentflow_llm::providers::stepfun::{
  ASRRequest, StepFunSpecializedClient, TTSBuilder, Text2ImageBuilder,
};
use agentflow_llm::providers::{
  AnthropicProvider, ContentType, GoogleProvider, LLMProvider, MoonshotProvider, OpenAIProvider,
  ProviderRequest, StepFunProvider, TokenUsage,
};
use agentflow_llm::tool_calling::{StopReason, ToolChoice, ToolSpec};
use agentflow_llm::{AgentFlow, LLMConfig};
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
    capabilities: &[
      LiveCapability::Llm,
      LiveCapability::Multimodal,
      LiveCapability::Audio,
    ],
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
  LiveProviderProfile {
    name: "glm",
    capabilities: &[LiveCapability::Llm, LiveCapability::Multimodal],
  },
  LiveProviderProfile {
    name: "dashscope",
    capabilities: &[LiveCapability::Llm],
  },
  LiveProviderProfile {
    name: "deepseek",
    capabilities: &[LiveCapability::Llm],
  },
];

fn provider_supports_capability(provider_name: &str, capability: LiveCapability) -> bool {
  LIVE_PROVIDER_PROFILES
    .iter()
    .find(|profile| profile.name == provider_name)
    .is_some_and(|profile| profile.capabilities.contains(&capability))
}

fn no_proxy_client() -> reqwest::Client {
  reqwest::Client::builder()
    .no_proxy()
    .build()
    .expect("no-proxy reqwest client")
}

fn glm_live_lock() -> &'static tokio::sync::Mutex<()> {
  static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn dashscope_live_lock() -> &'static tokio::sync::Mutex<()> {
  static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Resolve the DashScope OpenAI-compatible endpoint. Honors the bundled
/// `default_models.yml` provider entry (which points at
/// `https://dashscope.aliyuncs.com/compatible-mode/v1`); falls back to the
/// same hard-coded URL when the config is unreadable so the test is robust
/// against `~/.agentflow/models.yml` corruption.
async fn dashscope_base_url() -> Option<String> {
  let Ok((config, _source)) = LLMConfig::from_default_source().await else {
    return Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string());
  };

  config
    .get_provider("dashscope")
    .and_then(|provider| provider.base_url.clone())
    .or_else(|| Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string()))
}

async fn dashscope_live_context(capability: LiveCapability) -> Option<(String, Option<String>)> {
  if !prepare_live_provider("dashscope", capability).await {
    return None;
  }

  let Some((env_used, api_key)) = pick_api_key(&["DASHSCOPE_API_KEY"]) else {
    eprintln!("[live] dashscope: skipped (DASHSCOPE_API_KEY not set; live gate is on)");
    return None;
  };
  eprintln!("[live] dashscope: using API key from {env_used}");

  Some((api_key, dashscope_base_url().await))
}

fn deepseek_live_lock() -> &'static tokio::sync::Mutex<()> {
  static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Resolve the DeepSeek OpenAI-compatible endpoint. DeepSeek explicitly
/// supports both `https://api.deepseek.com/v1/chat/completions` and the
/// non-versioned `https://api.deepseek.com/chat/completions`; we use the
/// `/v1` form to match how `OpenAIProvider` appends `/chat/completions`
/// to the base URL. There's no `default_models.yml` provider entry yet,
/// so this is the unconditional fallback.
async fn deepseek_base_url() -> Option<String> {
  Some("https://api.deepseek.com/v1".to_string())
}

async fn deepseek_live_context(capability: LiveCapability) -> Option<(String, Option<String>)> {
  if !prepare_live_provider("deepseek", capability).await {
    return None;
  }

  let Some((env_used, api_key)) = pick_api_key(&["DEEPSEEK_API_KEY"]) else {
    eprintln!("[live] deepseek: skipped (DEEPSEEK_API_KEY not set; live gate is on)");
    return None;
  };
  eprintln!("[live] deepseek: using API key from {env_used}");

  Some((api_key, deepseek_base_url().await))
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

  // Live tests construct providers directly via `<Provider>::with_client`
  // and resolve API keys with `pick_api_key()`; the only registry use is the
  // hermetic-seed `whisper_via_modality_dispatcher_transcribes_audio` test
  // below, which seeds its own one-row config. Calling `AgentFlow::init()`
  // here would force-validate every model in the bundled `default_models.yml`
  // (which includes `dashscope` / `alibaba` entries unrelated to the
  // 6-provider live matrix), so a single missing unrelated key would fail-
  // close the entire suite. We instead load `~/.agentflow/.env` only — that's
  // the part that helps devs running the suite outside CI; in CI the workflow
  // sets every needed env var via the `env:` block directly.
  if let Some(home_dir) = dirs::home_dir() {
    let user_env = home_dir.join(".agentflow").join(".env");
    if user_env.exists() {
      dotenvy::from_path(&user_env).ok();
    }
  }
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
      // 16-token budget is enough for "ok" on the classic chat models
      // (openai / anthropic / moonshot / stepfun / glm) but Gemini 2.5
      // Flash burns the entire budget on internal thinking tokens before
      // emitting any output, surfacing as StopReason::Length. 256 is
      // still a tiny cap (~$0.0001/call) but gives reasoning models
      // enough headroom to actually finish.
      ("max_tokens".to_string(), json!(256)),
    ]),
    tools: None,
    tool_choice: None,
  }
}

fn provider_streaming_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    stream: true,
    ..provider_request(model)
  }
}

fn glm_provider_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": "Reply with exactly one word: ok"
    })],
    stream: false,
    parameters: HashMap::from([
      ("do_sample".to_string(), json!(false)),
      ("max_tokens".to_string(), json!(64)),
      ("thinking".to_string(), json!({ "type": "disabled" })),
    ]),
    tools: None,
    tool_choice: None,
  }
}

fn glm_provider_streaming_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    stream: true,
    ..glm_provider_request(model)
  }
}

fn provider_tool_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": "Use the get_weather tool to look up the weather in Tokyo. Do not answer directly."
    })],
    stream: false,
    parameters: HashMap::from([
      ("temperature".to_string(), json!(0.0)),
      ("max_tokens".to_string(), json!(64)),
    ]),
    tools: Some(vec![ToolSpec::new(
      "get_weather",
      "Get current weather for a city.",
      json!({
        "type": "object",
        "properties": {
          "city": {
            "type": "string",
            "description": "City name"
          }
        },
        "required": ["city"]
      }),
    )]),
    tool_choice: Some(ToolChoice::Tool {
      name: "get_weather".to_string(),
    }),
  }
}

fn glm_provider_tool_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": "Use the get_weather tool to look up the weather in Tokyo. Do not answer directly."
    })],
    stream: false,
    parameters: HashMap::from([
      ("do_sample".to_string(), json!(false)),
      ("max_tokens".to_string(), json!(128)),
      ("thinking".to_string(), json!({ "type": "disabled" })),
    ]),
    tools: Some(vec![ToolSpec::new(
      "get_weather",
      "Get current weather for a city.",
      json!({
        "type": "object",
        "properties": {
          "city": {
            "type": "string",
            "description": "City name"
          }
        },
        "required": ["city"]
      }),
    )]),
    tool_choice: Some(ToolChoice::Auto),
  }
}

fn provider_vision_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "Reply with one word naming the dominant color in this image."
        },
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR42mP8z8BQDwAFgwJ/l1u37wAAAABJRU5ErkJggg=="
          }
        }
      ]
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

fn glm_provider_vision_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    messages: vec![json!({
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "Reply with one word naming the dominant color in this image."
        },
        {
          "type": "image_url",
          "image_url": {
            "url": "https://www.gstatic.com/webp/gallery/1.jpg"
          }
        }
      ]
    })],
    parameters: HashMap::from([
      ("do_sample".to_string(), json!(false)),
      ("max_tokens".to_string(), json!(64)),
      ("thinking".to_string(), json!({ "type": "disabled" })),
    ]),
    ..provider_vision_request(model)
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

async fn assert_live_stream_non_empty(mut stream: Box<dyn agentflow_llm::StreamingResponse>) {
  let mut text = String::new();
  let mut saw_final = false;

  while let Some(chunk) = stream
    .next_chunk()
    .await
    .expect("live streaming chunk must parse")
  {
    if !chunk.content.is_empty() {
      text.push_str(&chunk.content);
    }
    if chunk.is_final {
      saw_final = true;
    }
  }

  assert!(
    !text.trim().is_empty(),
    "expected non-empty text from live streaming response"
  );
  assert!(
    saw_final,
    "expected live streaming response to include a final chunk"
  );
}

/// Minimum-cost model per provider for live smoke tests.
///
/// Override via the matching env var when a provider deprecates a model. The
/// defaults are picked for low cost-per-call and broad availability rather
/// than capability.
fn live_text_model(provider: &str, default: &str) -> String {
  live_model(provider, "TEXT", default)
}

fn live_model_override(provider: &str, capability: &str) -> Option<String> {
  let provider_upper = provider.to_ascii_uppercase();
  let specific = format!("AGENTFLOW_LIVE_{provider_upper}_{capability}_MODEL");
  let legacy = format!("AGENTFLOW_LIVE_{provider_upper}_MODEL");
  for env_var in [&specific, &legacy] {
    if let Ok(value) = std::env::var(env_var) {
      let trimmed = value.trim();
      if !trimmed.is_empty() {
        eprintln!("[live] {provider}: using {capability} model override from {env_var}");
        return Some(trimmed.to_string());
      }
    }
  }
  None
}

fn live_model(provider: &str, capability: &str, default: &str) -> String {
  if let Some(model) = live_model_override(provider, capability) {
    return model;
  }
  let provider_upper = provider.to_ascii_uppercase();
  let specific = format!("AGENTFLOW_LIVE_{provider_upper}_{capability}_MODEL");
  eprintln!(
    "[live] {provider}: using default {capability} model `{default}` (set {specific} to override)"
  );
  default.to_string()
}

fn model_id_from_config(alias: &str, model: &agentflow_llm::ModelConfig) -> String {
  model
    .model_id
    .clone()
    .or_else(|| {
      model
        .additional_params
        .get("model")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
    })
    .unwrap_or_else(|| alias.to_string())
}

async fn stepfun_configured_model<F>(
  capability: &str,
  preferred_models: &[&str],
  accepts: F,
) -> Option<String>
where
  F: Fn(&agentflow_llm::ModelConfig) -> bool,
{
  let Ok((config, _source)) = LLMConfig::from_default_source().await else {
    return None;
  };

  let mut candidates = config
    .models
    .iter()
    .filter(|(_, model)| {
      matches!(
        model.vendor.to_ascii_lowercase().as_str(),
        "stepfun" | "step"
      ) && accepts(model)
    })
    .map(|(alias, model)| (alias.as_str(), model_id_from_config(alias, model)))
    .collect::<Vec<_>>();

  candidates.sort_by(|(left_alias, left_model), (right_alias, right_model)| {
    let left_rank = preferred_models
      .iter()
      .position(|preferred| preferred == left_alias || preferred == left_model)
      .unwrap_or(usize::MAX);
    let right_rank = preferred_models
      .iter()
      .position(|preferred| preferred == right_alias || preferred == right_model)
      .unwrap_or(usize::MAX);
    left_rank
      .cmp(&right_rank)
      .then_with(|| left_alias.cmp(right_alias))
  });

  let selected = candidates.into_iter().next().map(|(_alias, model)| model);

  if let Some(model) = &selected {
    eprintln!("[live] stepfun: using {capability} model `{model}` from AgentFlow models config");
  }
  selected
}

async fn stepfun_live_model<F>(
  capability: &str,
  default: &str,
  preferred_models: &[&str],
  accepts: F,
) -> String
where
  F: Fn(&agentflow_llm::ModelConfig) -> bool,
{
  if let Some(env_model) = live_model_override("stepfun", capability) {
    return env_model;
  }

  stepfun_configured_model(capability, preferred_models, accepts)
    .await
    .unwrap_or_else(|| {
      eprintln!("[live] stepfun: using default {capability} model `{default}`");
      default.to_string()
    })
}

async fn stepfun_base_url() -> Option<String> {
  let Ok((config, _source)) = LLMConfig::from_default_source().await else {
    return None;
  };

  config
    .get_provider("stepfun")
    .or_else(|| config.get_provider("step"))
    .and_then(|provider| provider.base_url.clone())
}

async fn stepfun_live_context(capability: LiveCapability) -> Option<(String, Option<String>)> {
  if !prepare_live_provider("stepfun", capability).await {
    return None;
  }

  let Some((env_used, api_key)) = pick_api_key(&["STEPFUN_API_KEY", "STEP_API_KEY"]) else {
    eprintln!(
      "[live] stepfun: skipped (none of STEPFUN_API_KEY / STEP_API_KEY set; live gate is on)"
    );
    return None;
  };
  eprintln!("[live] stepfun: using API key from {env_used}");

  Some((api_key, stepfun_base_url().await))
}

async fn glm_configured_model<F>(
  capability: &str,
  preferred_models: &[&str],
  accepts: F,
) -> Option<String>
where
  F: Fn(&agentflow_llm::ModelConfig) -> bool,
{
  let Ok((config, _source)) = LLMConfig::from_default_source().await else {
    return None;
  };

  let mut candidates = config
    .models
    .iter()
    .filter(|(_, model)| {
      matches!(
        model.vendor.to_ascii_lowercase().as_str(),
        "glm" | "bigmodel" | "zhipu"
      ) && accepts(model)
    })
    .map(|(alias, model)| (alias.as_str(), model_id_from_config(alias, model)))
    .collect::<Vec<_>>();

  candidates.sort_by(|(left_alias, left_model), (right_alias, right_model)| {
    let left_rank = preferred_models
      .iter()
      .position(|preferred| preferred == left_alias || preferred == left_model)
      .unwrap_or(usize::MAX);
    let right_rank = preferred_models
      .iter()
      .position(|preferred| preferred == right_alias || preferred == right_model)
      .unwrap_or(usize::MAX);
    left_rank
      .cmp(&right_rank)
      .then_with(|| left_alias.cmp(right_alias))
  });

  let selected = candidates.into_iter().next().map(|(_alias, model)| model);

  if let Some(model) = &selected {
    eprintln!("[live] glm: using {capability} model `{model}` from AgentFlow models config");
  }
  selected
}

async fn glm_live_model<F>(
  capability: &str,
  default: &str,
  preferred_models: &[&str],
  accepts: F,
) -> String
where
  F: Fn(&agentflow_llm::ModelConfig) -> bool,
{
  if let Some(env_model) = live_model_override("glm", capability) {
    return env_model;
  }

  glm_configured_model(capability, preferred_models, accepts)
    .await
    .unwrap_or_else(|| {
      eprintln!("[live] glm: using default {capability} model `{default}`");
      default.to_string()
    })
}

async fn glm_base_url() -> Option<String> {
  let Ok((config, _source)) = LLMConfig::from_default_source().await else {
    return Some("https://open.bigmodel.cn/api/paas/v4".to_string());
  };

  config
    .get_provider("glm")
    .or_else(|| config.get_provider("bigmodel"))
    .or_else(|| config.get_provider("zhipu"))
    .and_then(|provider| provider.base_url.clone())
    .or_else(|| Some("https://open.bigmodel.cn/api/paas/v4".to_string()))
}

async fn glm_live_context(capability: LiveCapability) -> Option<(String, Option<String>)> {
  if !prepare_live_provider("glm", capability).await {
    return None;
  }

  let Some((env_used, api_key)) =
    pick_api_key(&["GLM_API_KEY", "BIGMODEL_API_KEY", "ZHIPU_API_KEY"])
  else {
    eprintln!(
      "[live] glm: skipped (none of GLM_API_KEY / BIGMODEL_API_KEY / ZHIPU_API_KEY set; live gate is on)"
    );
    return None;
  };
  eprintln!("[live] glm: using API key from {env_used}");

  Some((api_key, glm_base_url().await))
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
    // `claude-3-5-haiku-latest` alias and the dated `-20241022` revision
    // both 404 against current Anthropic tiers; `claude-haiku-4-5` is the
    // current cheapest model per CLAUDE.md.
    "anthropic" => live_text_model(provider_name, "claude-haiku-4-5"),
    // `gemini-1.5-flash` was retired from v1beta; `gemini-2.0-flash` is
    // "no longer available to new users" per the API; `gemini-2.5-flash`
    // is the current cheap-tier model.
    "google" => live_text_model(provider_name, "gemini-2.5-flash"),
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
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    StepFunProvider::with_client(no_proxy_client(), &api_key, base_url).expect("stepfun provider");
  let model = stepfun_live_model(
    "TEXT",
    "step-1-8k",
    &[
      "step-2-16k-202411",
      "step-2-16k",
      "step-2-mini",
      "step-1-8k",
    ],
    |model| model.model_type() == "text",
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live text request timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live text request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, "stepfun");
  assert_eq!(
    response.stop_reason,
    Some(StopReason::Stop),
    "stepfun: expected StopReason::Stop on a single-turn text completion"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_streaming_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    StepFunProvider::with_client(no_proxy_client(), &api_key, base_url).expect("stepfun provider");
  let model = stepfun_live_model(
    "TEXT",
    "step-1-8k",
    &[
      "step-2-16k-202411",
      "step-2-16k",
      "step-2-mini",
      "step-1-8k",
    ],
    |model| model.model_type() == "text" && model.supports_streaming_capability(),
  )
  .await;

  let stream = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute_streaming(&provider_streaming_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live streaming request timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live streaming request failed: {e}"));

  assert_live_stream_non_empty(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_tool_calling_or_fallback_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    StepFunProvider::with_client(no_proxy_client(), &api_key, base_url).expect("stepfun provider");
  let model = stepfun_live_model(
    "TOOLS",
    "step-1-8k",
    &[
      "step-2-16k-202411",
      "step-2-16k",
      "step-2-mini",
      "step-1-8k",
    ],
    |model| model.model_type() == "text" && model.supports_tools_capability(),
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_tool_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live tool request timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live tool request failed: {e}"));

  if let Some(tool_call) = response.tool_calls.first() {
    assert_eq!(tool_call.name, "get_weather");
    assert!(
      tool_call.arguments.get("city").is_some(),
      "stepfun: tool call must include city argument"
    );
    assert_eq!(
      response.stop_reason,
      Some(StopReason::ToolCalls),
      "stepfun: native tool calls should map to StopReason::ToolCalls"
    );
  } else {
    assert_text_non_empty(&response.content);
    eprintln!("[live] stepfun: model returned text fallback instead of native tool_calls");
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_vision_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Multimodal).await else {
    return;
  };
  let provider =
    StepFunProvider::with_client(no_proxy_client(), &api_key, base_url).expect("stepfun provider");
  let model = stepfun_live_model(
    "VISION",
    "step-1o-turbo-vision",
    &[
      "step-1o-turbo-vision",
      "step-1v-8k",
      "step-1o-vision-32k",
      "step-1v-32k",
      "step-3",
    ],
    |model| matches!(model.model_type(), "imageunderstand" | "multimodal") || model.is_multimodal(),
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_vision_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live vision request timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live vision request failed: {e}"));

  assert_text_non_empty(&response.content);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_image_generation_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Image).await else {
    return;
  };
  let client = StepFunSpecializedClient::with_client(no_proxy_client(), &api_key, base_url)
    .expect("stepfun specialized client");
  let model = stepfun_live_model(
    "IMAGE",
    "step-1x-medium",
    &["step-1x-medium", "step-2x-large"],
    |model| {
      matches!(model.model_type(), "generateimage" | "text2image" | "image")
        || model.is_image_model()
    },
  )
  .await;

  let request = Text2ImageBuilder::new(&model, "a tiny red square icon on a white background")
    .size("512x512")
    .response_format("b64_json")
    .steps(1)
    .build();

  let response = tokio::time::timeout(Duration::from_secs(60), client.text_to_image(request))
    .await
    .unwrap_or_else(|_| panic!("stepfun: live image generation timed out after 60s"))
    .unwrap_or_else(|e| panic!("stepfun: live image generation failed: {e}"));

  let first = response
    .data
    .first()
    .expect("stepfun: image generation response must contain at least one image");
  assert!(
    first.image.as_deref().is_some_and(|v| !v.is_empty())
      || first.b64_json.as_deref().is_some_and(|v| !v.is_empty())
      || first.url.as_deref().is_some_and(|v| !v.is_empty()),
    "stepfun: image generation response must contain image, b64_json, or url"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_tts_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Audio).await else {
    return;
  };
  let client = StepFunSpecializedClient::with_client(no_proxy_client(), &api_key, base_url)
    .expect("stepfun specialized client");
  let model = stepfun_live_model(
    "TTS",
    "step-tts-mini",
    &["step-tts-mini", "step-tts-vivid"],
    |model| model.model_type() == "tts",
  )
  .await;
  let voice = std::env::var("AGENTFLOW_LIVE_STEPFUN_TTS_VOICE")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "cixingnansheng".to_string());

  let request = TTSBuilder::new(&model, "ok", &voice)
    .response_format("mp3")
    .build();

  let audio = tokio::time::timeout(Duration::from_secs(30), client.text_to_speech(request))
    .await
    .unwrap_or_else(|_| panic!("stepfun: live TTS request timed out after 30s"))
    .unwrap_or_else(|e| panic!("stepfun: live TTS request failed: {e}"));

  assert!(
    audio.len() > 128,
    "stepfun: TTS response should contain audio bytes"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_live_asr_path() {
  let Some((api_key, base_url)) = stepfun_live_context(LiveCapability::Audio).await else {
    return;
  };
  let client = StepFunSpecializedClient::with_client(no_proxy_client(), &api_key, base_url)
    .expect("stepfun specialized client");
  let tts_model = stepfun_live_model(
    "TTS",
    "step-tts-mini",
    &["step-tts-mini", "step-tts-vivid"],
    |model| model.model_type() == "tts",
  )
  .await;
  let asr_model = stepfun_live_model("ASR", "step-asr", &["step-asr"], |model| {
    model.model_type() == "asr"
  })
  .await;
  let voice = std::env::var("AGENTFLOW_LIVE_STEPFUN_TTS_VOICE")
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "cixingnansheng".to_string());

  let speech = tokio::time::timeout(
    Duration::from_secs(30),
    client.text_to_speech(
      TTSBuilder::new(&tts_model, "agentflow live test", &voice)
        .response_format("mp3")
        .build(),
    ),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live ASR fixture TTS timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live ASR fixture TTS failed: {e}"));

  let transcript = tokio::time::timeout(
    Duration::from_secs(30),
    client.speech_to_text(ASRRequest {
      model: asr_model,
      response_format: "text".to_string(),
      audio_data: speech,
      filename: "agentflow-live-test.mp3".to_string(),
    }),
  )
  .await
  .unwrap_or_else(|_| panic!("stepfun: live ASR request timed out after 30s"))
  .unwrap_or_else(|e| panic!("stepfun: live ASR request failed: {e}"));

  assert!(
    !transcript.trim().is_empty(),
    "stepfun: ASR transcript must be non-empty"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glm_live_text_path() {
  let _guard = glm_live_lock().lock().await;
  let Some((api_key, base_url)) = glm_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("glm provider");
  let model = glm_live_model(
    "TEXT",
    "glm-4.5-flash",
    &["glm-4.5-flash", "glm-4-flash-250414", "glm-5.1"],
    |model| model.model_type() == "text",
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&glm_provider_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("glm: live text request timed out after 30s"))
  .unwrap_or_else(|e| panic!("glm: live text request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, "glm");
  assert_eq!(
    response.stop_reason,
    Some(StopReason::Stop),
    "glm: expected StopReason::Stop on a single-turn text completion"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glm_live_openai_compatible_chat_path() {
  let _guard = glm_live_lock().lock().await;
  let Some((api_key, base_url)) = glm_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("glm provider");
  let model = glm_live_model(
    "TEXT",
    "glm-4.5-flash",
    &["glm-4.5-flash", "glm-4-flash-250414", "glm-5.1"],
    |model| model.model_type() == "text",
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&ProviderRequest {
      messages: vec![json!({
        "role": "user",
        "content": "Reply with exactly these two lowercase letters: ok"
      })],
      ..glm_provider_request(&model)
    }),
  )
  .await
  .unwrap_or_else(|_| panic!("glm: OpenAI-compatible chat request timed out after 30s"))
  .unwrap_or_else(|e| panic!("glm: OpenAI-compatible chat request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, "glm");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glm_live_streaming_path() {
  let _guard = glm_live_lock().lock().await;
  let Some((api_key, base_url)) = glm_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("glm provider");
  let model = glm_live_model(
    "TEXT",
    "glm-4.5-flash",
    &["glm-4.5-flash", "glm-4-flash-250414", "glm-5.1"],
    |model| model.model_type() == "text" && model.supports_streaming_capability(),
  )
  .await;

  let stream = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute_streaming(&glm_provider_streaming_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("glm: live streaming request timed out after 30s"))
  .unwrap_or_else(|e| panic!("glm: live streaming request failed: {e}"));

  assert_live_stream_non_empty(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glm_live_tool_calling_or_fallback_path() {
  let _guard = glm_live_lock().lock().await;
  let Some((api_key, base_url)) = glm_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("glm provider");
  let model = glm_live_model(
    "TOOLS",
    "glm-4.5-flash",
    &["glm-4.5-flash", "glm-5.1", "glm-4.7-flash"],
    |model| model.model_type() == "text" && model.supports_tools_capability(),
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&glm_provider_tool_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("glm: live tool request timed out after 30s"))
  .unwrap_or_else(|e| panic!("glm: live tool request failed: {e}"));

  if let Some(tool_call) = response.tool_calls.first() {
    assert_eq!(tool_call.name, "get_weather");
    assert!(
      tool_call.arguments.get("city").is_some(),
      "glm: tool call must include city argument"
    );
    assert_eq!(
      response.stop_reason,
      Some(StopReason::ToolCalls),
      "glm: native tool calls should map to StopReason::ToolCalls"
    );
  } else {
    assert_text_non_empty(&response.content);
    eprintln!("[live] glm: model returned text fallback instead of native tool_calls");
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn glm_live_vision_path() {
  let _guard = glm_live_lock().lock().await;
  let Some((api_key, base_url)) = glm_live_context(LiveCapability::Multimodal).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("glm provider");
  let model = glm_live_model(
    "VISION",
    "glm-4.5v",
    &["glm-4.5v", "glm-5v-turbo", "glm-4.1v-thinking"],
    |model| matches!(model.model_type(), "imageunderstand" | "multimodal") || model.is_multimodal(),
  )
  .await;

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&glm_provider_vision_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("glm: live vision request timed out after 30s"))
  .unwrap_or_else(|e| panic!("glm: live vision request failed: {e}"));

  assert_text_non_empty(&response.content);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dashscope_live_text_path() {
  // DashScope (Alibaba Bailian) exposes an OpenAI-compatible chat completions
  // endpoint at `<base>/compatible-mode/v1`, so we drive it through
  // `OpenAIProvider::with_client` like GLM. `qwen-plus` is a long-standing
  // stable alias that Alibaba maintains across model generations — it
  // currently points at the latest qwen-plus revision and 91 dashscope
  // entries in `default_models.yml` use it. Override at workflow level
  // via `AGENTFLOW_LIVE_DASHSCOPE_TEXT_MODEL` if the alias ever decays.
  let _guard = dashscope_live_lock().lock().await;
  let Some((api_key, base_url)) = dashscope_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("dashscope provider");
  let model = live_text_model("dashscope", "qwen-plus");

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("dashscope: live text request timed out after 30s"))
  .unwrap_or_else(|e| panic!("dashscope: live text request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, "dashscope");
  assert_eq!(
    response.stop_reason,
    Some(StopReason::Stop),
    "dashscope: expected StopReason::Stop on a single-turn text completion"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deepseek_live_text_path() {
  // DeepSeek (https://api.deepseek.com) is OpenAI-compatible. `deepseek-chat`
  // is the long-standing stable alias (V3 family today); per their docs it
  // remains the canonical entry point through at least 2026-07-24. If a
  // future model rolls and breaks the alias, override at workflow level via
  // `AGENTFLOW_LIVE_DEEPSEEK_TEXT_MODEL`.
  let _guard = deepseek_live_lock().lock().await;
  let Some((api_key, base_url)) = deepseek_live_context(LiveCapability::Llm).await else {
    return;
  };
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), &api_key, base_url).expect("deepseek provider");
  let model = live_text_model("deepseek", "deepseek-chat");

  let response = tokio::time::timeout(
    Duration::from_secs(30),
    provider.execute(&provider_request(&model)),
  )
  .await
  .unwrap_or_else(|_| panic!("deepseek: live text request timed out after 30s"))
  .unwrap_or_else(|e| panic!("deepseek: live text request failed: {e}"));

  assert_text_non_empty(&response.content);
  assert_usage_populated(&response.usage, "deepseek");
  assert_eq!(
    response.stop_reason,
    Some(StopReason::Stop),
    "deepseek: expected StopReason::Stop on a single-turn text completion"
  );
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
  assert!(provider_supports_capability("glm", LiveCapability::Llm));
  assert!(provider_supports_capability(
    "glm",
    LiveCapability::Multimodal
  ));
  assert!(!provider_supports_capability("glm", LiveCapability::Audio));
  assert!(!provider_supports_capability(
    "unknown",
    LiveCapability::Llm
  ));
}

// ============================================================================
// P-LLM.5 — OpenAI Whisper via the modality dispatcher
//
// Validates that `AgentFlow::asr("whisper-1")` resolves through the registry
// to the OpenAI vendor, builds an `OpenAIAsrProvider`, and successfully
// transcribes a tiny audio clip. Gated on `AGENTFLOW_LIVE_AUDIO_TESTS=1` AND
// `OPENAI_API_KEY` so the workspace stays hermetic without the key.
//
// Uses StepFun TTS to produce the audio fixture (live), then runs the
// transcript through Whisper. If StepFun isn't reachable the audio
// fixture falls back to a small WAV header so the test still validates
// the dispatcher / multipart / response-parsing path even when only
// OPENAI_API_KEY is configured.
// ============================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn whisper_via_modality_dispatcher_transcribes_audio() {
  if !live_gate_enabled(LiveCapability::Audio) {
    eprintln!("[live] whisper: skipped (AGENTFLOW_LIVE_AUDIO_TESTS not enabled)");
    return;
  }
  let api_key = match std::env::var("OPENAI_API_KEY") {
    Ok(v) if !v.trim().is_empty() => v,
    _ => {
      eprintln!("[live] whisper: skipped (OPENAI_API_KEY missing)");
      return;
    }
  };

  // Ensure the test process has the OpenAI key registered before
  // dispatcher key resolution.
  // SAFETY: tests run with their own env scope.
  unsafe {
    std::env::set_var("OPENAI_API_KEY", api_key);
  }

  // Generate the audio fixture FIRST — `stepfun_live_context` triggers
  // `AgentFlow::init()` which reloads the user's `~/.agentflow/models.yml`,
  // so any registry mutation must happen after this point.
  let audio: Vec<u8> =
    if let Some((sf_key, sf_base)) = stepfun_live_context(LiveCapability::Audio).await {
      let client = StepFunSpecializedClient::with_client(no_proxy_client(), &sf_key, sf_base)
        .expect("stepfun specialized client");
      let req = TTSBuilder::new("step-tts-mini", "agentflow whisper test", "cixingnansheng")
        .response_format("mp3")
        .build();
      match tokio::time::timeout(Duration::from_secs(30), client.text_to_speech(req)).await {
        Ok(Ok(bytes)) => bytes,
        _ => silent_wav_one_second(),
      }
    } else {
      silent_wav_one_second()
    };

  // Now seed the hermetic whisper-1 entry. This wins over the user's
  // ~/.agentflow/models.yml (which may predate the P-LLM.5 registry
  // additions). The bundled `default_models.yml` already carries
  // whisper-1; this seed just makes the test independent of the host's
  // local registry choices.
  let hermetic_registry_yaml = r#"
models:
  whisper-1:
    vendor: openai
    type: asr
    accepts: [audio]
"#;
  agentflow_llm::ModelRegistry::global()
    .load_config_from_yaml(hermetic_registry_yaml)
    .await
    .expect("seed hermetic whisper-1 entry");

  let provider = AgentFlow::asr("whisper-1")
    .await
    .expect("whisper-1 resolves through dispatcher");
  assert_eq!(provider.name(), "openai", "whisper-1 must route to openai");

  let request = agentflow_llm::AsrRequest {
    model: "whisper-1".to_string(),
    audio_data: audio,
    filename: "agentflow-whisper-test.mp3".to_string(),
    response_format: "json".to_string(),
    language: Some("en".to_string()),
    temperature: Some(0.0),
    prompt: Some("agentflow, whisper, modality dispatcher".to_string()),
  };
  let response = tokio::time::timeout(Duration::from_secs(60), provider.transcribe(request))
    .await
    .unwrap_or_else(|_| panic!("whisper transcription timed out after 60s"))
    .unwrap_or_else(|e| panic!("whisper transcription failed: {e}"));

  // Silent fallback may produce empty transcript; only assert that the
  // call succeeded and the metadata round-tripped. For the StepFun-
  // produced fixture the transcript is usually non-empty.
  assert!(
    response.metadata.is_some(),
    "json response_format must carry metadata"
  );
}

fn silent_wav_one_second() -> Vec<u8> {
  // Minimal 1-second mono 8kHz silent WAV. Whisper happily accepts
  // empty audio; the transcript is usually an empty string.
  let mut wav = Vec::with_capacity(44 + 8000 * 2);
  wav.extend_from_slice(b"RIFF");
  wav.extend_from_slice(&(36u32 + 8000u32 * 2).to_le_bytes());
  wav.extend_from_slice(b"WAVE");
  wav.extend_from_slice(b"fmt ");
  wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
  wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
  wav.extend_from_slice(&1u16.to_le_bytes()); // mono
  wav.extend_from_slice(&8000u32.to_le_bytes()); // sample rate
  wav.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
  wav.extend_from_slice(&2u16.to_le_bytes()); // block align
  wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
  wav.extend_from_slice(b"data");
  wav.extend_from_slice(&(8000u32 * 2).to_le_bytes());
  wav.extend(std::iter::repeat_n(0u8, 8000 * 2));
  wav
}
