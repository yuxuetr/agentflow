use agentflow_llm::{
  registry::{model_registry::ModelInfo, ModelRegistry},
  AgentFlow, LLMConfig,
};
use anyhow::{Context, Result};
use colored::*;
use std::path::PathBuf;

pub async fn execute(provider: Option<String>, detailed: bool) -> Result<()> {
  let models = if let Some(config_path) = user_config_path()? {
    let config = LLMConfig::from_file(&config_path)
      .await
      .with_context(|| format!("Failed to load config file '{}'", config_path.display()))?;
    printable_models_from_config(&config)
  } else {
    printable_models_from_registry().await?
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

  if detailed {
    print_detailed_models(&filtered_models);
  } else {
    print_simple_models(&filtered_models);
  }

  Ok(())
}

#[derive(Debug, Clone)]
struct PrintableModel {
  name: String,
  vendor: String,
  model_id: String,
  base_url: Option<String>,
  temperature: Option<f32>,
  max_tokens: Option<u32>,
  supports_streaming: bool,
}

fn user_config_path() -> Result<Option<PathBuf>> {
  let Some(home_dir) = dirs::home_dir() else {
    return Ok(None);
  };
  let path = home_dir.join(".agentflow").join("models.yml");
  Ok(path.exists().then_some(path))
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
