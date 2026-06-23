use agentflow_llm::{
  AgentFlow, LLMConfig, LLMConfigSourceKind,
  registry::{ModelRegistry, model_registry::ModelInfo},
};
use anyhow::{Context, Result};
use colored::*;
use serde::Serialize;
use std::collections::BTreeSet;

pub async fn execute(
  provider: Option<String>,
  detailed: bool,
  refresh_from_api: bool,
  format: String,
) -> Result<()> {
  let source = LLMConfig::resolve_default_source()?;
  for warning in &source.warnings {
    eprintln!("Warning: {warning}");
  }

  // F-A7-6: --refresh-from-api branches into the live-query path.
  // The local-listing path stays the default so existing scripts
  // and the offline case keep working unchanged.
  if refresh_from_api {
    return execute_refresh(provider, source).await;
  }

  let models = match source.kind {
    LLMConfigSourceKind::BuiltInDefault => printable_models_from_registry().await?,
    _ => {
      let config_path = source
        .path
        .as_ref()
        .context("resolved config source had no path")?;
      let config = LLMConfig::from_file(config_path)
        .await
        .with_context(|| format!("Failed to load config file '{}'", config_path.display()))?;
      printable_models_from_config(&config)
    }
  };

  if models.is_empty() {
    println!("No models found. Run 'agentflow config init' to set up your configuration.");
    return Ok(());
  }

  let filtered_models: Vec<_> = if let Some(ref provider_filter) = provider {
    models
      .into_iter()
      .filter(|model| {
        model
          .vendor
          .to_lowercase()
          .contains(&provider_filter.to_lowercase())
      })
      .collect()
  } else {
    models
  };

  if filtered_models.is_empty() {
    println!(
      "No models found for provider: {}",
      provider.unwrap_or_default()
    );
    return Ok(());
  }

  if format == "json-envelope" {
    // P3.3 migration: emit the canonical `CliJsonEnvelope` (no
    // prior JSON path existed; this is the first machine-readable
    // surface for `llm models`). `result.models[]` carries the
    // same data the detailed text view renders.
    let payload = serde_json::json!({
      "source": source.display_path(),
      "source_kind": source.kind,
      "provider_filter": provider,
      "models": &filtered_models,
      "total": filtered_models.len(),
    });
    let envelope = crate::json_envelope::CliJsonEnvelope::ok("llm models", &payload);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    return Ok(());
  }

  if detailed {
    print_detailed_models(&filtered_models);
  } else {
    print_simple_models(&filtered_models);
  }

  Ok(())
}

/// F-A7-6: query each OpenAI-compatible provider's `/v1/models`
/// endpoint and print the delta vs the local registry. Read-only;
/// doesn't write to `models.yml`. Currently supported providers:
/// openai, moonshot, stepfun, dashscope. Anthropic and Google have
/// different `/models` shapes (or none) and are reported as
/// "skipped (refresh not supported)".
///
/// Output groups per-provider:
///   - **new**: present on provider, missing locally — candidates
///     to add to `models.yml`
///   - **only_local**: in `models.yml` but not on provider — typo /
///     deprecated / private deployment
///   - **shared**: count only (full list available without
///     `--refresh-from-api`)
async fn execute_refresh(
  provider_filter: Option<String>,
  source: agentflow_llm::LLMConfigSource,
) -> Result<()> {
  let config = match source.path.as_ref() {
    Some(path) => LLMConfig::from_file(path)
      .await
      .with_context(|| format!("Failed to load config file '{}'", path.display()))?,
    None => {
      anyhow::bail!(
        "`--refresh-from-api` needs a real models.yml to diff against. \
         Run `agentflow config init` first to generate one at ~/.agentflow/models.yml."
      );
    }
  };

  println!(
    "{}",
    "Refreshing model list from provider APIs (read-only diff)"
      .bold()
      .blue()
  );
  println!();

  let mut any_provider_queried = false;
  // Sorted for deterministic output.
  let providers: std::collections::BTreeMap<_, _> = config.providers.iter().collect();

  for (provider_name, provider_cfg) in providers {
    if let Some(filter) = provider_filter.as_ref()
      && !provider_name
        .to_lowercase()
        .contains(&filter.to_lowercase())
    {
      continue;
    }
    print!("{}:", provider_name.bold().green());

    let Some(url) = refresh_url_for(provider_name, provider_cfg.base_url.as_deref()) else {
      println!(" skipped (refresh not supported for this provider yet)");
      continue;
    };

    let api_key = match std::env::var(&provider_cfg.api_key_env) {
      Ok(k) if !k.is_empty() => k,
      _ => {
        println!(
          " skipped ({} not set in environment)",
          provider_cfg.api_key_env
        );
        continue;
      }
    };

    any_provider_queried = true;
    println!();
    match fetch_models(&url, &api_key).await {
      Ok(remote_ids) => {
        let local_ids: BTreeSet<String> = config
          .models
          .iter()
          .filter(|(_, m)| m.vendor == *provider_name)
          .map(|(name, m)| m.model_id.clone().unwrap_or_else(|| name.clone()))
          .collect();
        print_diff(provider_name, &local_ids, &remote_ids);
      }
      Err(e) => {
        println!("  {} {}", "error:".red(), e);
      }
    }
    println!();
  }

  if !any_provider_queried {
    println!(
      "(No providers queried. Either no API keys are set, no providers in your config support refresh yet, or your --provider filter matched nothing.)"
    );
  }

  Ok(())
}

/// F-A7-6: provider → `/v1/models` URL. Returns None for
/// providers whose `/models` endpoint shape isn't OpenAI-compatible
/// or doesn't exist at all (Google Gemini uses `models.list` via
/// SDK; Anthropic does have `/v1/models` but the response shape
/// differs — adding it is a follow-up).
fn refresh_url_for(provider: &str, base_url: Option<&str>) -> Option<String> {
  // Provider names map to base URLs in the bundled config; we
  // hard-code the path suffix here because `/v1/models` is the
  // OpenAI-compatible convention and shouldn't be configurable.
  let fallback_base = match provider {
    "openai" => "https://api.openai.com/v1",
    "moonshot" => "https://api.moonshot.cn/v1",
    "stepfun" => "https://api.stepfun.com/v1",
    "dashscope" => "https://dashscope.aliyuncs.com/compatible-mode/v1",
    _ => return None,
  };
  // If the user configured a non-default base_url, respect it
  // (relevant for proxy / on-prem deployments).
  let base = base_url.unwrap_or(fallback_base).trim_end_matches('/');
  Some(format!("{base}/models"))
}

/// F-A7-6: fetch + parse a `{"data": [{"id": "..."}, ...]}`
/// response. Reqwest does the heavy lifting; we just project the
/// `id` field so the diff is independent of any vendor-specific
/// metadata the response may carry.
async fn fetch_models(url: &str, api_key: &str) -> Result<BTreeSet<String>> {
  #[derive(serde::Deserialize)]
  struct ModelsResponse {
    data: Vec<ModelEntry>,
  }
  #[derive(serde::Deserialize)]
  struct ModelEntry {
    id: String,
  }

  let client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(15))
    .build()
    .context("reqwest client init")?;
  let resp = client
    .get(url)
    .bearer_auth(api_key)
    .send()
    .await
    .with_context(|| format!("GET {url} failed"))?;

  let status = resp.status();
  if !status.is_success() {
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("GET {url} returned {status}: {}", truncate(&body, 200));
  }

  let parsed: ModelsResponse = resp
    .json()
    .await
    .with_context(|| format!("parsing /models response from {url}"))?;
  Ok(parsed.data.into_iter().map(|m| m.id).collect())
}

fn print_diff(provider: &str, local: &BTreeSet<String>, remote: &BTreeSet<String>) {
  let new_on_remote: Vec<_> = remote.difference(local).collect();
  let only_local: Vec<_> = local.difference(remote).collect();
  let shared = local.intersection(remote).count();

  println!(
    "  shared: {} model(s) present in both your config and the {} API",
    shared.to_string().yellow(),
    provider
  );

  if !new_on_remote.is_empty() {
    println!(
      "  {} on {} ({} candidates to add to models.yml):",
      "new".bold().green(),
      provider,
      new_on_remote.len()
    );
    for id in &new_on_remote {
      println!("    + {id}");
    }
  }

  if !only_local.is_empty() {
    println!(
      "  {} (in models.yml but NOT returned by the {} API — may be deprecated, a typo, or a private deployment):",
      "only_local".bold().yellow(),
      provider
    );
    for id in &only_local {
      println!("    - {id}");
    }
  }
}

fn truncate(s: &str, max: usize) -> String {
  // Char-safe (P-A3.5): the byte slice `&s[..max]` panics when `max` lands in
  // the middle of a multi-byte UTF-8 codepoint. `s` here is an arbitrary remote
  // HTTP response body (see the `truncate(&body, 200)` error path), so that is
  // a reachable panic. Counting / taking `chars` never splits a codepoint.
  if s.chars().count() <= max {
    s.to_string()
  } else {
    format!("{}…", s.chars().take(max).collect::<String>())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// F-A7-6: URL constructor uses the fallback base for each known
  /// provider when no override is configured. The path suffix
  /// (`/models`) is invariant.
  #[test]
  fn refresh_url_for_known_providers_uses_default_base() {
    assert_eq!(
      refresh_url_for("openai", None).as_deref(),
      Some("https://api.openai.com/v1/models")
    );
    assert_eq!(
      refresh_url_for("moonshot", None).as_deref(),
      Some("https://api.moonshot.cn/v1/models")
    );
    assert_eq!(
      refresh_url_for("stepfun", None).as_deref(),
      Some("https://api.stepfun.com/v1/models")
    );
    assert_eq!(
      refresh_url_for("dashscope", None).as_deref(),
      Some("https://dashscope.aliyuncs.com/compatible-mode/v1/models")
    );
  }

  /// F-A7-6: a user-configured `base_url` (e.g. for proxy / on-prem)
  /// overrides the fallback. Trailing slash on the override is
  /// tolerated so authors don't have to know about the implementation
  /// detail of how `/models` is appended.
  #[test]
  fn refresh_url_for_respects_user_base_url_override() {
    assert_eq!(
      refresh_url_for("moonshot", Some("https://my-proxy.example.com/v1")).as_deref(),
      Some("https://my-proxy.example.com/v1/models")
    );
    // Trailing slash on override is fine.
    assert_eq!(
      refresh_url_for("openai", Some("https://my-proxy.example.com/v1/")).as_deref(),
      Some("https://my-proxy.example.com/v1/models")
    );
  }

  /// F-A7-6: unsupported providers (Google / Anthropic, or anything
  /// agentflow-llm grows in the future before its `/models` shape
  /// gets table-mapped here) return None so the refresh path can
  /// skip them with a clear message instead of issuing malformed
  /// requests.
  #[test]
  fn refresh_url_for_unsupported_providers_returns_none() {
    assert!(refresh_url_for("google", None).is_none());
    assert!(refresh_url_for("anthropic", None).is_none());
    assert!(refresh_url_for("some-future-vendor", None).is_none());
  }

  #[test]
  fn truncate_short_string_unchanged() {
    assert_eq!(truncate("hello", 20), "hello");
  }

  #[test]
  fn truncate_long_string_appends_ellipsis() {
    let long = "x".repeat(300);
    let t = truncate(&long, 50);
    assert_eq!(t.len(), 50 + "…".len());
    assert!(t.ends_with("…"));
  }

  #[test]
  fn truncate_multibyte_does_not_panic_on_codepoint_boundary() {
    // Each '世' is 3 bytes: a byte slice `&s[..max]` would panic when `max`
    // splits one. Char-safe truncation keeps `max` whole codepoints.
    let s = "世".repeat(300);
    let t = truncate(&s, 50);
    assert_eq!(t.chars().count(), 50 + 1); // 50 kept chars + '…'
    assert!(t.ends_with("…"));
    // A boundary that lands mid-codepoint in bytes must still be fine.
    assert_eq!(truncate("a世b", 2), "a世…");
  }
}

#[derive(Debug, Clone, Serialize)]
struct PrintableModel {
  name: String,
  vendor: String,
  model_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  base_url: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  temperature: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  max_tokens: Option<u32>,
  supports_streaming: bool,
}

async fn printable_models_from_registry() -> Result<Vec<PrintableModel>> {
  AgentFlow::init_with_builtin_config().await?;

  let registry = ModelRegistry::global();
  let model_names = registry.list_models();
  let models: Result<Vec<ModelInfo>, _> = model_names
    .iter()
    .map(|name| registry.get_model_info(name))
    .collect();

  Ok(
    models?
      .into_iter()
      .map(|model| PrintableModel {
        name: model.name,
        vendor: model.vendor,
        model_id: model.model_id,
        base_url: Some(model.base_url),
        temperature: model.temperature,
        max_tokens: model.max_tokens,
        supports_streaming: model.supports_streaming,
      })
      .collect(),
  )
}

fn printable_models_from_config(config: &LLMConfig) -> Vec<PrintableModel> {
  let mut models: Vec<_> = config
    .models
    .iter()
    .map(|(name, model)| {
      let provider_base_url = config
        .providers
        .get(&model.vendor)
        .and_then(|provider| provider.base_url.clone());
      PrintableModel {
        name: name.clone(),
        vendor: model.vendor.clone(),
        model_id: model.model_id.clone().unwrap_or_else(|| name.clone()),
        base_url: model.base_url.clone().or(provider_base_url),
        temperature: model.temperature,
        max_tokens: model.max_tokens,
        supports_streaming: model.supports_streaming.unwrap_or(true),
      }
    })
    .collect();
  models.sort_by(|a, b| a.vendor.cmp(&b.vendor).then_with(|| a.name.cmp(&b.name)));
  models
}

fn print_simple_models(models: &[PrintableModel]) {
  println!("{}", "Available Models:".bold().blue());
  println!();

  let mut current_provider = String::new();
  for model in models {
    if model.vendor != current_provider {
      current_provider = model.vendor.clone();
      println!("{}", format!("{}:", current_provider).bold().green());
    }

    let model_name = if model.model_id.starts_with(&model.vendor) {
      model.model_id.clone()
    } else {
      format!("{}/{}", model.vendor, model.model_id)
    };

    println!("  • {}", model_name);
  }
}

fn print_detailed_models(models: &[PrintableModel]) {
  println!("{}", "Available Models (Detailed):".bold().blue());
  println!();

  let mut current_provider = String::new();
  for model in models {
    if model.vendor != current_provider {
      current_provider = model.vendor.clone();
      println!("{}", format!("{}:", current_provider).bold().green());
      println!();
    }

    let model_name = if model.model_id.starts_with(&model.vendor) {
      model.model_id.clone()
    } else {
      format!("{}/{}", model.vendor, model.model_id)
    };

    println!("  {}", model_name.bold());
    println!("    Name: {}", model.name);
    println!("    Vendor: {}", model.vendor);
    println!("    Model ID: {}", model.model_id);
    if let Some(base_url) = &model.base_url {
      println!("    Base URL: {}", base_url);
    }

    if let Some(temperature) = model.temperature {
      println!("    Temperature: {}", temperature.to_string().yellow());
    }

    if let Some(max_tokens) = model.max_tokens {
      println!("    Max Tokens: {}", max_tokens.to_string().yellow());
    }

    println!("    Supports Streaming: {}", model.supports_streaming);
    println!();
  }
}
