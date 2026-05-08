use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use agentflow_llm::LLMConfig;
use agentflow_tools::sandbox::default_backend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
  Text,
  Json,
}

impl OutputFormat {
  pub fn parse(value: &str) -> Result<Self> {
    match value {
      "text" => Ok(Self::Text),
      "json" => Ok(Self::Json),
      other => Err(anyhow::anyhow!(
        "unsupported doctor output format '{other}', expected 'text' or 'json'"
      )),
    }
  }
}

#[derive(Debug, Serialize)]
struct DoctorReport {
  version: &'static str,
  features: FeatureReport,
  paths: PathReport,
  config: ConfigReport,
  sandbox: SandboxReport,
  environment: EnvironmentReport,
  status: DoctorStatus,
}

#[derive(Debug, Serialize)]
struct FeatureReport {
  rag: bool,
  plugin: bool,
  mcp_workflow_nodes: bool,
}

#[derive(Debug, Serialize)]
struct PathReport {
  home: Option<PathBuf>,
  config_dir: Option<PathBuf>,
  models_config: Option<PathBuf>,
  env_file: Option<PathBuf>,
  skills_dir: Option<PathBuf>,
  plugins_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ConfigReport {
  models_config_exists: bool,
  models_config_loadable: bool,
  models: usize,
  providers: usize,
  missing_env_vars: Vec<String>,
  error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SandboxReport {
  backend: &'static str,
  enforcing: bool,
}

#[derive(Debug, Serialize)]
struct EnvironmentReport {
  agentflow_run_dir: Option<String>,
  agentflow_trace_dir: Option<String>,
  agentflow_api_token_set: bool,
  agentflow_skills_index: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum DoctorStatus {
  Ok,
  Warning,
}

pub async fn execute(format: OutputFormat) -> Result<()> {
  let report = build_report().await;
  match format {
    OutputFormat::Json => {
      println!("{}", serde_json::to_string_pretty(&report)?);
    }
    OutputFormat::Text => print_text_report(&report),
  }
  Ok(())
}

async fn build_report() -> DoctorReport {
  let home = dirs::home_dir();
  let config_dir = home.as_ref().map(|p| p.join(".agentflow"));
  let models_config = config_dir.as_ref().map(|p| p.join("models.yml"));
  let env_file = config_dir.as_ref().map(|p| p.join(".env"));
  let skills_dir = home.as_ref().map(|p| p.join(".agentflow").join("skills"));
  let plugins_dir = home.as_ref().map(|p| p.join(".agentflow").join("plugins"));

  let config = match models_config.as_ref() {
    Some(path) => inspect_config(path, env_file.as_deref()).await,
    None => ConfigReport {
      models_config_exists: false,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      error: Some("could not determine home directory".to_string()),
    },
  };

  let sandbox_backend = default_backend();
  let sandbox = SandboxReport {
    backend: sandbox_backend.name(),
    enforcing: sandbox_backend.is_enforcing(),
  };

  let status =
    if config.error.is_some() || !config.missing_env_vars.is_empty() || !sandbox.enforcing {
      DoctorStatus::Warning
    } else {
      DoctorStatus::Ok
    };

  DoctorReport {
    version: env!("CARGO_PKG_VERSION"),
    features: FeatureReport {
      rag: cfg!(feature = "rag"),
      plugin: cfg!(feature = "plugin"),
      mcp_workflow_nodes: cfg!(feature = "mcp"),
    },
    paths: PathReport {
      home,
      config_dir,
      models_config,
      env_file,
      skills_dir,
      plugins_dir,
    },
    config,
    sandbox,
    environment: EnvironmentReport {
      agentflow_run_dir: std::env::var("AGENTFLOW_RUN_DIR").ok(),
      agentflow_trace_dir: std::env::var("AGENTFLOW_TRACE_DIR").ok(),
      agentflow_api_token_set: std::env::var("AGENTFLOW_API_TOKEN").is_ok(),
      agentflow_skills_index: std::env::var("AGENTFLOW_SKILLS_INDEX").ok(),
    },
    status,
  }
}

async fn inspect_config(path: &Path, env_path: Option<&Path>) -> ConfigReport {
  if !path.exists() {
    return ConfigReport {
      models_config_exists: false,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      error: Some("models.yml not found; run `agentflow config init`".to_string()),
    };
  }

  match LLMConfig::from_file(path).await {
    Ok(config) => {
      let configured_env = env_path
        .map(load_env_file_keys)
        .transpose()
        .unwrap_or_default()
        .unwrap_or_default();
      let mut missing_env_vars = Vec::new();
      for provider in config.providers.values() {
        if std::env::var(&provider.api_key_env).is_err()
          && !configured_env.contains(&provider.api_key_env)
        {
          missing_env_vars.push(provider.api_key_env.clone());
        }
      }
      missing_env_vars.sort();
      missing_env_vars.dedup();

      ConfigReport {
        models_config_exists: true,
        models_config_loadable: true,
        models: config.models.len(),
        providers: config.providers.len(),
        missing_env_vars,
        error: None,
      }
    }
    Err(e) => ConfigReport {
      models_config_exists: true,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      error: Some(e.to_string()),
    },
  }
}

fn load_env_file_keys(path: &Path) -> Result<std::collections::BTreeSet<String>> {
  if !path.exists() {
    return Ok(std::collections::BTreeSet::new());
  }
  let content = std::fs::read_to_string(path)
    .with_context(|| format!("failed to read env file '{}'", path.display()))?;
  Ok(content.lines().filter_map(parse_env_key).collect())
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

fn print_text_report(report: &DoctorReport) {
  println!("AgentFlow doctor");
  println!("Version: {}", report.version);
  println!("Status: {}", status_label(&report.status));
  println!();

  println!("Features:");
  println!("  rag: {}", enabled_label(report.features.rag));
  println!("  plugin: {}", enabled_label(report.features.plugin));
  println!(
    "  mcp workflow nodes: {}",
    enabled_label(report.features.mcp_workflow_nodes)
  );
  println!();

  println!("Paths:");
  println!("  home: {}", optional_path(report.paths.home.as_deref()));
  println!(
    "  config: {}",
    optional_path(report.paths.models_config.as_deref())
  );
  println!(
    "  skills: {}",
    optional_path(report.paths.skills_dir.as_deref())
  );
  println!(
    "  plugins: {}",
    optional_path(report.paths.plugins_dir.as_deref())
  );
  println!();

  println!("Config:");
  println!(
    "  models.yml: {}",
    if report.config.models_config_exists {
      "found"
    } else {
      "missing"
    }
  );
  println!(
    "  loadable: {}",
    enabled_label(report.config.models_config_loadable)
  );
  println!("  models: {}", report.config.models);
  println!("  providers: {}", report.config.providers);
  if report.config.missing_env_vars.is_empty() {
    println!("  missing env vars: none");
  } else {
    println!(
      "  missing env vars: {}",
      report.config.missing_env_vars.join(", ")
    );
  }
  if let Some(error) = &report.config.error {
    println!("  warning: {error}");
  }
  println!();

  println!("Sandbox:");
  println!("  backend: {}", report.sandbox.backend);
  println!("  enforcing: {}", enabled_label(report.sandbox.enforcing));
  if !report.sandbox.enforcing {
    println!("  warning: this platform has no enforcing OS sandbox backend");
  }
  println!();

  println!("Environment:");
  println!(
    "  AGENTFLOW_RUN_DIR: {}",
    optional_env(report.environment.agentflow_run_dir.as_deref())
  );
  println!(
    "  AGENTFLOW_TRACE_DIR: {}",
    optional_env(report.environment.agentflow_trace_dir.as_deref())
  );
  println!(
    "  AGENTFLOW_API_TOKEN: {}",
    if report.environment.agentflow_api_token_set {
      "set"
    } else {
      "unset"
    }
  );
  println!(
    "  AGENTFLOW_SKILLS_INDEX: {}",
    optional_env(report.environment.agentflow_skills_index.as_deref())
  );
}

fn enabled_label(value: bool) -> &'static str {
  if value { "yes" } else { "no" }
}

fn status_label(status: &DoctorStatus) -> &'static str {
  match status {
    DoctorStatus::Ok => "ok",
    DoctorStatus::Warning => "warning",
  }
}

fn optional_path(path: Option<&Path>) -> String {
  path
    .map(|p| p.display().to_string())
    .unwrap_or_else(|| "unknown".to_string())
}

fn optional_env(value: Option<&str>) -> &str {
  value.unwrap_or("unset")
}

#[cfg(test)]
mod tests {
  use super::{OutputFormat, parse_env_key};

  #[test]
  fn output_format_rejects_unknown_values() {
    assert!(OutputFormat::parse("yaml").is_err());
    assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
    assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
  }

  #[test]
  fn parse_env_key_ignores_empty_and_comments() {
    assert_eq!(
      parse_env_key("OPENAI_API_KEY=secret").as_deref(),
      Some("OPENAI_API_KEY")
    );
    assert_eq!(parse_env_key("# OPENAI_API_KEY=secret"), None);
    assert_eq!(parse_env_key("OPENAI_API_KEY="), None);
  }
}
