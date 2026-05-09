use crate::redaction::redact_cli_value;
use agentflow_llm::{LLMConfig, LLMConfigSourceKind};
use anyhow::{Context, Result, bail};

pub async fn execute(section: Option<String>) -> Result<()> {
  let source = LLMConfig::resolve_default_source()?;
  for warning in &source.warnings {
    eprintln!("Warning: {warning}");
  }

  let yaml_value = if let Some(config_path) = source.path.as_ref() {
    let content = tokio::fs::read_to_string(config_path)
      .await
      .with_context(|| format!("Failed to read config file '{}'", config_path.display()))?;
    serde_yaml::from_str(&content)
      .with_context(|| format!("Failed to parse config file '{}'", config_path.display()))?
  } else {
    let (config, _) = LLMConfig::from_default_source().await?;
    serde_yaml::to_value(config).context("Failed to render built-in config")?
  };

  let selected = match section.as_deref() {
    Some("models") => yaml_value.get("models").cloned(),
    Some("providers") => yaml_value.get("providers").cloned(),
    Some("defaults") => yaml_value.get("defaults").cloned(),
    Some(other) => {
      bail!("Unknown config section '{other}' (expected models, providers, or defaults)")
    }
    None => Some(yaml_value),
  }
  .with_context(|| {
    format!(
      "Config section '{}' not found in '{}'",
      section.as_deref().unwrap_or("<root>"),
      source.display_path()
    )
  })?;

  let mut json_value =
    serde_json::to_value(selected).context("Failed to convert config to JSON for redaction")?;
  redact_cli_value(&mut json_value);
  let redacted_yaml =
    serde_yaml::to_string(&json_value).context("Failed to render redacted config")?;

  println!("# source: {:?}", source.kind);
  match source.kind {
    LLMConfigSourceKind::BuiltInDefault => println!("# built-in default_models.yml"),
    _ => {
      if let Some(config_path) = source.path.as_ref() {
        println!("# {}", config_path.display());
      }
    }
  }
  print!("{redacted_yaml}");
  Ok(())
}
