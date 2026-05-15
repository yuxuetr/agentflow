use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use agentflow_llm::{LLMConfig, LLMConfigSource, MODELS_CONFIG_ENV};
use agentflow_tools::sandbox::{SandboxEnforcement, default_backend};
use agentflow_tools::{SECURITY_PROFILE_ENV, SecurityProfile, SecurityProfileDefaults};

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
  profile: DoctorProfile,
  features: FeatureReport,
  paths: PathReport,
  config: ConfigReport,
  security: SecurityReport,
  sandbox: SandboxReport,
  environment: EnvironmentReport,
  disk: DiskReport,
  #[serde(skip_serializing_if = "Option::is_none")]
  server: Option<ServerReport>,
  #[serde(skip_serializing_if = "Option::is_none")]
  backup_check: Option<BackupCheckReport>,
  status: DoctorStatus,
}

/// Backup-readiness report populated only when `--backup-check` is supplied.
/// Walks every workspace state directory an operator would need to back up
/// or restore and probes that each is present and writable. Extends the
/// existing `DiskReport` (which only covers run/trace/marketplace) with the
/// skills and plugins install dirs. See `docs/SERVER_BACKUP_RESTORE.md`
/// for the rationale behind the dir set.
#[derive(Debug, Serialize)]
struct BackupCheckReport {
  run_dir: DirCheck,
  trace_dir: DirCheck,
  marketplace_cache: DirCheck,
  skills_dir: DirCheck,
  plugins_dir: DirCheck,
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
  legacy_models_config: Option<PathBuf>,
  env_file: Option<PathBuf>,
  skills_dir: Option<PathBuf>,
  plugins_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ConfigReport {
  models_config_source: String,
  models_config_path: String,
  models_config_exists: bool,
  models_config_loadable: bool,
  models: usize,
  providers: usize,
  missing_env_vars: Vec<String>,
  warnings: Vec<String>,
  error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SecurityReport {
  env_var: &'static str,
  profile: SecurityProfile,
  defaults: SecurityProfileDefaults,
  warning: Option<String>,
}

#[derive(Debug, Serialize)]
struct SandboxReport {
  backend: &'static str,
  /// Tri-state enforcement (`enforcing` / `permissive` / `disabled`). Operators
  /// reading the JSON output can distinguish "no platform backend on this OS"
  /// (`disabled`) from "backend exists but cannot enforce right now"
  /// (`permissive`, e.g. missing `sandbox-exec`, unsupported arch).
  enforcement: SandboxEnforcement,
  /// Kept for backwards-compatible JSON consumers. Equivalent to
  /// `enforcement == "enforcing"`.
  enforcing: bool,
  capabilities: Vec<&'static str>,
  warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EnvironmentReport {
  agentflow_run_dir: Option<String>,
  agentflow_trace_dir: Option<String>,
  agentflow_api_token_set: bool,
  agentflow_skills_index: Option<String>,
}

/// Filesystem reachability report for the workspace state dirs that
/// AgentFlow writes during execution. The byte-cost check is
/// deliberately coarse: we look up "is the directory present, and is
/// it writable" rather than running platform-specific `statvfs`. The
/// 80 % case for operators is "did I forget to mount the run-dir
/// volume" — that case is fully covered without a new dependency.
#[derive(Debug, Serialize)]
struct DiskReport {
  run_dir: DirCheck,
  trace_dir: DirCheck,
  marketplace_cache: DirCheck,
}

#[derive(Debug, Clone, Serialize)]
struct DirCheck {
  /// Resolved path (override → env → default).
  path: String,
  /// Stable identifier of the source (`override`, `env`, `default`).
  source: &'static str,
  /// `true` when the directory exists and is a directory.
  exists: bool,
  /// `true` when a probe-file create + remove succeeded under the dir.
  writable: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
}

/// Server reachability report populated only when `--server <url>` is
/// supplied. Issues a `GET <url>/health` with a 3 s timeout and
/// records the HTTP status code.
#[derive(Debug, Serialize)]
struct ServerReport {
  url: String,
  reachable: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  status_code: Option<u16>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<String>,
}

/// Tri-state doctor verdict. `agentflow doctor` exits with the
/// corresponding [`Self::exit_code`] so CI gates can branch on it
/// (`P3.4`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DoctorStatus {
  Ok,
  Warning,
  Fail,
}

impl DoctorStatus {
  fn exit_code(self) -> i32 {
    match self {
      Self::Ok => 0,
      Self::Warning => 1,
      Self::Fail => 2,
    }
  }

  fn promote(&mut self, other: Self) {
    if other.rank() > self.rank() {
      *self = other;
    }
  }

  fn rank(self) -> u8 {
    match self {
      Self::Ok => 0,
      Self::Warning => 1,
      Self::Fail => 2,
    }
  }
}

/// Pass/fail threshold profile chosen via `--profile`. Sticks to
/// `local` by default so the legacy behaviour (warn but exit 0… now
/// warn = exit 1) stays close to what existing users expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorProfile {
  /// Most lenient. Missing models config and missing API keys stay
  /// at `Warning` and never escalate to `Fail`.
  Dev,
  /// Default. Matches the security model `local` profile.
  Local,
  /// Strictest. Missing API keys, missing API token, missing
  /// marketplace cache, and missing run/trace dirs escalate to
  /// `Fail`.
  Production,
}

impl DoctorProfile {
  pub fn parse(value: &str) -> Result<Self> {
    match value {
      "dev" => Ok(Self::Dev),
      "local" => Ok(Self::Local),
      "production" => Ok(Self::Production),
      other => Err(anyhow::anyhow!(
        "unsupported --profile '{other}', expected dev | local | production"
      )),
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Dev => "dev",
      Self::Local => "local",
      Self::Production => "production",
    }
  }
}

/// Execute the doctor command. The CLI passes through `--profile` and
/// optional `--server` URL; both default to None so legacy invocations
/// keep working.
pub async fn execute(
  format: OutputFormat,
  profile: DoctorProfile,
  server: Option<String>,
  backup_check: bool,
) -> Result<()> {
  let report = build_report(profile, server.as_deref(), backup_check).await;
  match format {
    OutputFormat::Json => {
      println!("{}", serde_json::to_string_pretty(&report)?);
    }
    OutputFormat::Text => print_text_report(&report),
  }
  std::process::exit(report.status.exit_code());
}

async fn build_report(
  profile: DoctorProfile,
  server: Option<&str>,
  backup_check: bool,
) -> DoctorReport {
  let home = dirs::home_dir();
  let config_dir = home.as_ref().map(|p| p.join(".agentflow"));
  let resolved_source = LLMConfig::resolve_default_source().ok();
  let models_config = resolved_source
    .as_ref()
    .and_then(|source| source.path.clone());
  let legacy_models_config = config_dir.as_ref().map(|p| p.join("models.yaml"));
  let env_file = config_dir.as_ref().map(|p| p.join(".env"));
  let skills_dir = home.as_ref().map(|p| p.join(".agentflow").join("skills"));
  let plugins_dir = home.as_ref().map(|p| p.join(".agentflow").join("plugins"));

  let config = match resolved_source.as_ref() {
    Some(source) => inspect_config(source, env_file.as_deref()).await,
    None => ConfigReport {
      models_config_source: "unknown".to_string(),
      models_config_path: "unknown".to_string(),
      models_config_exists: false,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      warnings: Vec::new(),
      error: Some("could not determine home directory".to_string()),
    },
  };

  let sandbox_backend = default_backend();
  let enforcement = sandbox_backend.enforcement_level();
  let sandbox = SandboxReport {
    backend: sandbox_backend.name(),
    enforcement,
    enforcing: enforcement.is_enforcing(),
    capabilities: sandbox_capabilities(enforcement.is_enforcing()),
    warnings: sandbox_warnings(sandbox_backend.name(), enforcement),
  };

  let security = security_report();
  let disk = disk_report(home.as_deref());
  let server_report = match server {
    Some(url) => Some(probe_server(url).await),
    None => None,
  };

  let mut status = DoctorStatus::Ok;

  // Config / API keys.
  if config.error.is_some() {
    status.promote(DoctorStatus::Warning);
  }
  if !config.missing_env_vars.is_empty() {
    status.promote(match profile {
      DoctorProfile::Production => DoctorStatus::Fail,
      _ => DoctorStatus::Warning,
    });
  }

  // Sandbox.
  if !sandbox.enforcing {
    let level = if matches!(profile, DoctorProfile::Production) {
      DoctorStatus::Fail
    } else {
      DoctorStatus::Warning
    };
    status.promote(level);
  }

  // Security warnings.
  if security.warning.is_some() {
    status.promote(DoctorStatus::Warning);
  }

  // Disk reachability.
  for check in [&disk.run_dir, &disk.trace_dir, &disk.marketplace_cache] {
    if !check.exists {
      status.promote(DoctorStatus::Warning);
    } else if !check.writable {
      status.promote(match profile {
        DoctorProfile::Production => DoctorStatus::Fail,
        _ => DoctorStatus::Warning,
      });
    }
  }

  // Server reachability (only when explicitly probed).
  if let Some(report) = server_report.as_ref()
    && !report.reachable
  {
    status.promote(DoctorStatus::Fail);
  }

  // Backup-readiness section (only populated when --backup-check is set).
  let backup_check_report = if backup_check {
    let skills = resolve_dir(home.as_deref(), "AGENTFLOW_SKILLS_DIR", &["skills"]);
    let plugins = resolve_dir(home.as_deref(), "AGENTFLOW_PLUGINS_DIR", &["plugins"]);
    let report = BackupCheckReport {
      run_dir: disk.run_dir.clone(),
      trace_dir: disk.trace_dir.clone(),
      marketplace_cache: disk.marketplace_cache.clone(),
      skills_dir: skills,
      plugins_dir: plugins,
    };
    for check in [
      &report.run_dir,
      &report.trace_dir,
      &report.marketplace_cache,
      &report.skills_dir,
      &report.plugins_dir,
    ] {
      if !check.exists {
        status.promote(match profile {
          DoctorProfile::Production => DoctorStatus::Fail,
          _ => DoctorStatus::Warning,
        });
      } else if !check.writable {
        status.promote(DoctorStatus::Fail);
      }
    }
    Some(report)
  } else {
    None
  };

  DoctorReport {
    version: env!("CARGO_PKG_VERSION"),
    profile,
    features: FeatureReport {
      rag: cfg!(feature = "rag"),
      plugin: cfg!(feature = "plugin"),
      mcp_workflow_nodes: cfg!(feature = "mcp"),
    },
    paths: PathReport {
      home,
      config_dir,
      models_config,
      legacy_models_config,
      env_file,
      skills_dir,
      plugins_dir,
    },
    config,
    security,
    sandbox,
    environment: EnvironmentReport {
      agentflow_run_dir: std::env::var("AGENTFLOW_RUN_DIR").ok(),
      agentflow_trace_dir: std::env::var("AGENTFLOW_TRACE_DIR").ok(),
      agentflow_api_token_set: std::env::var("AGENTFLOW_API_TOKEN").is_ok(),
      agentflow_skills_index: std::env::var("AGENTFLOW_SKILLS_INDEX").ok(),
    },
    disk,
    server: server_report,
    backup_check: backup_check_report,
    status,
  }
}

fn disk_report(home: Option<&Path>) -> DiskReport {
  let run_dir = resolve_dir(home, "AGENTFLOW_RUN_DIR", &["runs"]);
  let trace_dir = resolve_dir(home, "AGENTFLOW_TRACE_DIR", &["traces"]);
  let marketplace_cache = resolve_dir(
    home,
    "AGENTFLOW_MARKETPLACE_CACHE",
    &["marketplace", "cache"],
  );
  DiskReport {
    run_dir,
    trace_dir,
    marketplace_cache,
  }
}

fn resolve_dir(home: Option<&Path>, env_var: &str, default_tail: &[&str]) -> DirCheck {
  let (path, source) =
    if let Some(value) = std::env::var(env_var).ok().filter(|v| !v.trim().is_empty()) {
      (PathBuf::from(value), "env")
    } else if let Some(home) = home {
      let mut p = home.join(".agentflow");
      for segment in default_tail {
        p.push(segment);
      }
      (p, "default")
    } else {
      (PathBuf::from("<unknown>"), "default")
    };

  let exists = path.is_dir();
  let writable = if exists { probe_writable(&path) } else { false };
  let error = if !exists {
    Some("directory does not exist; will be created on first write".to_string())
  } else if !writable {
    Some("directory exists but write probe failed".to_string())
  } else {
    None
  };
  DirCheck {
    path: path.display().to_string(),
    source,
    exists,
    writable,
    error,
  }
}

fn probe_writable(dir: &Path) -> bool {
  let probe = dir.join(format!(".agentflow-doctor-probe-{}", std::process::id()));
  match std::fs::write(&probe, b"probe") {
    Ok(()) => {
      let _ = std::fs::remove_file(&probe);
      true
    }
    Err(_) => false,
  }
}

async fn probe_server(url: &str) -> ServerReport {
  let trimmed = url.trim_end_matches('/');
  let health = format!("{trimmed}/health");
  let client = match reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(3))
    .build()
  {
    Ok(client) => client,
    Err(err) => {
      return ServerReport {
        url: url.to_string(),
        reachable: false,
        status_code: None,
        error: Some(format!("http client init failed: {err}")),
      };
    }
  };
  match client.get(&health).send().await {
    Ok(response) => {
      let code = response.status().as_u16();
      let ok = response.status().is_success();
      ServerReport {
        url: url.to_string(),
        reachable: ok,
        status_code: Some(code),
        error: if ok {
          None
        } else {
          Some(format!("non-success HTTP status {code}"))
        },
      }
    }
    Err(err) => ServerReport {
      url: url.to_string(),
      reachable: false,
      status_code: None,
      error: Some(err.to_string()),
    },
  }
}

fn security_report() -> SecurityReport {
  match SecurityProfile::from_env() {
    Ok(profile) => SecurityReport {
      env_var: SECURITY_PROFILE_ENV,
      profile,
      defaults: profile.defaults(),
      warning: None,
    },
    Err(err) => {
      let profile = SecurityProfile::default();
      SecurityReport {
        env_var: SECURITY_PROFILE_ENV,
        profile,
        defaults: profile.defaults(),
        warning: Some(format!(
          "{SECURITY_PROFILE_ENV} is invalid ({err}); falling back to '{profile}' for diagnostics"
        )),
      }
    }
  }
}

async fn inspect_config(source: &LLMConfigSource, env_path: Option<&Path>) -> ConfigReport {
  let source_name = format!("{:?}", source.kind);
  let source_path = source.display_path();
  let Some(path) = source.path.as_ref() else {
    return match LLMConfig::from_default_source().await {
      Ok((config, _)) => ConfigReport {
        models_config_source: source_name,
        models_config_path: source_path,
        models_config_exists: true,
        models_config_loadable: true,
        models: config.models.len(),
        providers: config.providers.len(),
        missing_env_vars: Vec::new(),
        warnings: source.warnings.clone(),
        error: None,
      },
      Err(e) => ConfigReport {
        models_config_source: source_name,
        models_config_path: source_path,
        models_config_exists: false,
        models_config_loadable: false,
        models: 0,
        providers: 0,
        missing_env_vars: Vec::new(),
        warnings: source.warnings.clone(),
        error: Some(e.to_string()),
      },
    };
  };

  if !path.exists() {
    return ConfigReport {
      models_config_source: source_name,
      models_config_path: source_path,
      models_config_exists: false,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      warnings: source.warnings.clone(),
      error: Some(format!(
        "{} not found; run `agentflow config init` or set {MODELS_CONFIG_ENV}",
        path.display()
      )),
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
        models_config_source: source_name,
        models_config_path: source_path,
        models_config_exists: true,
        models_config_loadable: true,
        models: config.models.len(),
        providers: config.providers.len(),
        missing_env_vars,
        warnings: source.warnings.clone(),
        error: None,
      }
    }
    Err(e) => ConfigReport {
      models_config_source: source_name,
      models_config_path: source_path,
      models_config_exists: true,
      models_config_loadable: false,
      models: 0,
      providers: 0,
      missing_env_vars: Vec::new(),
      warnings: source.warnings.clone(),
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
    "  legacy config: {}",
    optional_path(report.paths.legacy_models_config.as_deref())
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
  println!("  source: {}", report.config.models_config_source);
  println!("  path: {}", report.config.models_config_path);
  println!(
    "  models config: {}",
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
  for warning in &report.config.warnings {
    println!("  warning: {warning}");
  }
  println!();

  println!("Security:");
  println!("  profile: {}", report.security.profile);
  println!("  env var: {}", report.security.env_var);
  println!(
    "  auth token required: {}",
    enabled_label(report.security.defaults.auth.require_api_token)
  );
  println!("  cors: {:?}", report.security.defaults.cors.mode);
  println!(
    "  max request body: {} bytes",
    report
      .security
      .defaults
      .request_limits
      .max_request_body_bytes
  );
  println!(
    "  os sandbox required: {}",
    enabled_label(report.security.defaults.sandboxing.require_os_sandbox)
  );
  println!(
    "  subprocess plugins: {}",
    enabled_label(report.security.defaults.plugins.allow_subprocess_plugins)
  );
  println!(
    "  marketplace signatures: {}",
    enabled_label(
      report
        .security
        .defaults
        .marketplace
        .require_signature_verification
    )
  );
  if let Some(warning) = &report.security.warning {
    println!("  warning: {warning}");
  }
  println!();

  println!("Sandbox:");
  println!("  backend: {}", report.sandbox.backend);
  println!("  enforcement: {}", report.sandbox.enforcement.as_str());
  println!("  enforcing: {}", enabled_label(report.sandbox.enforcing));
  println!(
    "  capabilities: {}",
    if report.sandbox.capabilities.is_empty() {
      "none".to_string()
    } else {
      report.sandbox.capabilities.join(", ")
    }
  );
  for warning in &report.sandbox.warnings {
    println!("  warning: {warning}");
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
  println!();

  println!("Disk:");
  print_dir_check("run dir", &report.disk.run_dir);
  print_dir_check("trace dir", &report.disk.trace_dir);
  print_dir_check("marketplace cache", &report.disk.marketplace_cache);
  println!();

  if let Some(server) = &report.server {
    println!("Server:");
    println!("  url: {}", server.url);
    println!("  reachable: {}", enabled_label(server.reachable));
    if let Some(code) = server.status_code {
      println!("  status: {code}");
    }
    if let Some(err) = &server.error {
      println!("  error: {err}");
    }
    println!();
  }

  if let Some(backup) = &report.backup_check {
    println!("Backup check:");
    print_dir_check("run dir", &backup.run_dir);
    print_dir_check("trace dir", &backup.trace_dir);
    print_dir_check("marketplace cache", &backup.marketplace_cache);
    print_dir_check("skills dir", &backup.skills_dir);
    print_dir_check("plugins dir", &backup.plugins_dir);
    println!();
  }

  println!("Profile: {}", report.profile.as_str());
}

fn print_dir_check(label: &str, check: &DirCheck) {
  let status = match (check.exists, check.writable) {
    (true, true) => "ok",
    (true, false) => "read-only",
    (false, _) => "missing",
  };
  println!("  {label}: {status} ({}) [{}]", check.path, check.source);
  if let Some(err) = &check.error {
    println!("    note: {err}");
  }
}

fn enabled_label(value: bool) -> &'static str {
  if value { "yes" } else { "no" }
}

fn status_label(status: &DoctorStatus) -> &'static str {
  match status {
    DoctorStatus::Ok => "ok",
    DoctorStatus::Warning => "warning",
    DoctorStatus::Fail => "fail",
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

fn sandbox_capabilities(enforcing: bool) -> Vec<&'static str> {
  if enforcing {
    vec!["process", "filesystem", "network"]
  } else {
    Vec::new()
  }
}

fn sandbox_warnings(backend: &str, enforcement: SandboxEnforcement) -> Vec<String> {
  match enforcement {
    SandboxEnforcement::Enforcing => Vec::new(),
    SandboxEnforcement::Permissive => vec![format!(
      "sandbox backend '{backend}' is installed but not enforcing in this environment; shell, script, and plugin runs rely only on in-process policy checks"
    )],
    SandboxEnforcement::Disabled => vec![format!(
      "no enforcing sandbox backend is available (running with backend '{backend}'); shell, script, and plugin runs rely only on in-process policy checks"
    )],
  }
}

#[cfg(test)]
mod tests {
  use super::{OutputFormat, SandboxEnforcement, parse_env_key, sandbox_warnings};

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

  #[test]
  fn sandbox_warnings_explain_disabled_state() {
    let warnings = sandbox_warnings("noop", SandboxEnforcement::Disabled);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("no enforcing sandbox backend"));
    assert!(warnings[0].contains("in-process policy"));
  }

  #[test]
  fn sandbox_warnings_distinguish_permissive_from_disabled() {
    // Operators need to tell "platform has no backend at all" apart from
    // "platform backend exists but cannot enforce right now". The two
    // warnings must therefore be different strings.
    let disabled = sandbox_warnings("noop", SandboxEnforcement::Disabled);
    let permissive = sandbox_warnings("sandbox-exec", SandboxEnforcement::Permissive);
    assert_ne!(disabled, permissive);
    assert!(permissive[0].contains("installed but not enforcing"));
  }

  #[test]
  fn sandbox_warnings_empty_when_enforcing() {
    let warnings = sandbox_warnings("seccomp", SandboxEnforcement::Enforcing);
    assert!(warnings.is_empty());
  }
}
