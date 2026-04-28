use agentflow_llm::LLMConfig;
use anyhow::{Context, Result};
use std::{
  collections::{BTreeSet, HashSet},
  path::{Path, PathBuf},
};

pub async fn execute() -> Result<()> {
  let config_path = default_config_path()?;
  let config = LLMConfig::from_file(&config_path)
    .await
    .with_context(|| format!("Failed to load config file '{}'", config_path.display()))?;

  let env_path = default_env_path()?;
  let configured_env = load_env_file_keys(&env_path).await?;
  let mut required_env = BTreeSet::new();

  for provider in config.providers.values() {
    required_env.insert(provider.api_key_env.clone());
  }

  for model in config.models.values() {
    if !config.providers.contains_key(&model.vendor) && !model.vendor.eq_ignore_ascii_case("mock") {
      required_env.insert(format!("{}_API_KEY", model.vendor.to_uppercase()));
    }
  }

  let mut missing_env = Vec::new();
  for key in &required_env {
    if std::env::var(key).is_err() && !configured_env.contains(key) {
      missing_env.push(key.clone());
    }
  }

  println!("Configuration: {}", config_path.display());
  println!("Models: {}", config.models.len());
  println!("Providers: {}", config.providers.len());
  println!("Required env vars: {}", required_env.len());

  if missing_env.is_empty() {
    println!("Status: valid");
  } else {
    println!("Status: valid with missing secrets");
    println!("Missing env vars:");
    for key in missing_env {
      println!("  - {key}");
    }
  }

  Ok(())
}

fn default_config_path() -> Result<PathBuf> {
  let home_dir = dirs::home_dir().context("Could not determine home directory")?;
  Ok(home_dir.join(".agentflow").join("models.yml"))
}

fn default_env_path() -> Result<PathBuf> {
  let home_dir = dirs::home_dir().context("Could not determine home directory")?;
  Ok(home_dir.join(".agentflow").join(".env"))
}

async fn load_env_file_keys(path: &Path) -> Result<HashSet<String>> {
  if !path.exists() {
    return Ok(HashSet::new());
  }

  let content = tokio::fs::read_to_string(path)
    .await
    .with_context(|| format!("Failed to read env file '{}'", path.display()))?;
  let keys = content
    .lines()
    .filter_map(parse_env_key)
    .collect::<HashSet<_>>();
  Ok(keys)
}

fn parse_env_key(line: &str) -> Option<String> {
  let trimmed = line.trim();
  if trimmed.is_empty() || trimmed.starts_with('#') {
    return None;
  }
  let (key, value) = trimmed.split_once('=')?;
  let key = key.trim();
  let value = value.trim().trim_matches('"').trim_matches('\'');
  if key.is_empty() || value.is_empty() {
    return None;
  }
  Some(key.to_string())
}

#[cfg(test)]
mod tests {
  use super::parse_env_key;

  #[test]
  fn parse_env_key_ignores_comments_and_empty_values() {
    assert_eq!(
      parse_env_key("OPENAI_API_KEY=secret").as_deref(),
      Some("OPENAI_API_KEY")
    );
    assert_eq!(
      parse_env_key("  STEP_API_KEY = \"secret\" ").as_deref(),
      Some("STEP_API_KEY")
    );
    assert_eq!(parse_env_key("# OPENAI_API_KEY=secret"), None);
    assert_eq!(parse_env_key("OPENAI_API_KEY="), None);
  }
}
