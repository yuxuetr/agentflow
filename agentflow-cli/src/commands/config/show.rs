use crate::redaction::redact_cli_value;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

pub async fn execute(section: Option<String>) -> Result<()> {
  let config_path = default_config_path()?;
  let content = tokio::fs::read_to_string(&config_path)
    .await
    .with_context(|| format!("Failed to read config file '{}'", config_path.display()))?;
  let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)
    .with_context(|| format!("Failed to parse config file '{}'", config_path.display()))?;

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
      config_path.display()
    )
  })?;

  let mut json_value =
    serde_json::to_value(selected).context("Failed to convert config to JSON for redaction")?;
  redact_cli_value(&mut json_value);
  let redacted_yaml =
    serde_yaml::to_string(&json_value).context("Failed to render redacted config")?;

  println!("# {}", config_path.display());
  print!("{redacted_yaml}");
  Ok(())
}

fn default_config_path() -> Result<PathBuf> {
  let home_dir = dirs::home_dir().context("Could not determine home directory")?;
  Ok(home_dir.join(".agentflow").join("models.yml"))
}
