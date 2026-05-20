//! `agentflow backup` — orchestrates `pg_dump` + filesystem `tar`
//! into a single output directory (P10.15.1).
//!
//! Closes the operator loop that `docs/SERVER_BACKUP_RESTORE.md`
//! documents: that doc describes *which* state surfaces must be
//! backed up; this command actually does it in one invocation
//! instead of leaving the operator to run `pg_dump` + 5 tars by
//! hand and reason about the order.
//!
//! Output layout (single output directory, idempotent re-runs are
//! refused unless `--force`):
//!
//! ```text
//! <output>/
//!   manifest.json          # what's in the bundle, schema version, timestamps
//!   db.dump                # pg_dump --format=custom (when DB included)
//!   run_dir.tar.gz         # tar -czf of $AGENTFLOW_RUN_DIR (when present)
//!   trace_dir.tar.gz       # tar -czf of $AGENTFLOW_TRACE_DIR (when present)
//!   marketplace_cache.tar.gz
//!   skills_dir.tar.gz
//!   plugins_dir.tar.gz
//! ```
//!
//! What this command **does not** do:
//!
//! - Restore. Pairs naturally with a future `agentflow restore
//!   --input <path>`, but P10.15.1 is scope-limited to backup.
//!   The `manifest.json` shape is the contract a future restore
//!   would consume.
//! - In-Rust tar / pg_dump. Shelling out is operationally
//!   correct: `pg_dump` is the authoritative Postgres tool, and
//!   system `tar` handles symlinks / xattrs / sparse files better
//!   than any pure-Rust port. The cost is a small one-line PATH
//!   probe up front; we surface "tool not found" with the exact
//!   apt-get / brew install line the operator needs.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::json_envelope::CliJsonEnvelope;

/// Stable schema discriminator for the backup bundle manifest.
/// Bump only on breaking changes to `manifest.json`'s shape; the
/// per-include `tarball` filename slot is *not* in scope.
pub const BUNDLE_MANIFEST_VERSION: &str = "agentflow.backup/1";

/// One include of the bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupInclude {
  /// `pg_dump --format=custom` of the configured Postgres URL.
  Db,
  /// `$AGENTFLOW_RUN_DIR` (per-run artifact root).
  RunDir,
  /// `$AGENTFLOW_TRACE_DIR` (trace persistence root).
  TraceDir,
  /// `~/.agentflow/marketplace/cache` (downloaded plugin/skill
  /// artifacts).
  MarketplaceCache,
  /// `$AGENTFLOW_SKILLS_DIR` (installed skill manifests).
  SkillsDir,
  /// `$AGENTFLOW_PLUGINS_DIR` (installed plugin binaries +
  /// manifests).
  PluginsDir,
}

impl BackupInclude {
  pub const ALL: [BackupInclude; 6] = [
    BackupInclude::Db,
    BackupInclude::RunDir,
    BackupInclude::TraceDir,
    BackupInclude::MarketplaceCache,
    BackupInclude::SkillsDir,
    BackupInclude::PluginsDir,
  ];

  pub fn parse(s: &str) -> Option<Self> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
      "db" | "database" | "postgres" => BackupInclude::Db,
      "run_dir" | "run-dir" | "runs" => BackupInclude::RunDir,
      "trace_dir" | "trace-dir" | "traces" => BackupInclude::TraceDir,
      "marketplace_cache" | "marketplace-cache" | "marketplace" => BackupInclude::MarketplaceCache,
      "skills_dir" | "skills-dir" | "skills" => BackupInclude::SkillsDir,
      "plugins_dir" | "plugins-dir" | "plugins" => BackupInclude::PluginsDir,
      _ => return None,
    })
  }

  /// Per-include output filename inside the bundle directory.
  pub fn artifact_name(self) -> &'static str {
    match self {
      BackupInclude::Db => "db.dump",
      BackupInclude::RunDir => "run_dir.tar.gz",
      BackupInclude::TraceDir => "trace_dir.tar.gz",
      BackupInclude::MarketplaceCache => "marketplace_cache.tar.gz",
      BackupInclude::SkillsDir => "skills_dir.tar.gz",
      BackupInclude::PluginsDir => "plugins_dir.tar.gz",
    }
  }

  /// Stable serialization tag for the manifest / log output.
  pub fn tag(self) -> &'static str {
    match self {
      BackupInclude::Db => "db",
      BackupInclude::RunDir => "run_dir",
      BackupInclude::TraceDir => "trace_dir",
      BackupInclude::MarketplaceCache => "marketplace_cache",
      BackupInclude::SkillsDir => "skills_dir",
      BackupInclude::PluginsDir => "plugins_dir",
    }
  }
}

/// Parsed CLI arguments for `agentflow backup`.
#[derive(Debug, Clone)]
pub struct BackupArgs {
  /// Output directory. Must be empty (or absent — we create it),
  /// unless `--force` is supplied.
  pub output: PathBuf,
  /// Postgres URL. Falls back to `DATABASE_URL` env. Used only
  /// when `BackupInclude::Db` is in the include set.
  pub database_url: Option<String>,
  /// Plan-only mode — emit the manifest *as it would be* + the
  /// shell commands we would run, but mutate nothing.
  pub dry_run: bool,
  /// Overwrite `output` if non-empty. Without this flag a
  /// non-empty `output` errors out before any work happens.
  pub force: bool,
  /// Explicit include set. Empty means "all" (the documented
  /// default).
  pub includes: Vec<BackupInclude>,
  /// Output format for the report. `text` is the human-readable
  /// default; `json` emits the raw `BackupReport` (compat with
  /// other commands that pre-date the envelope); `json-envelope`
  /// emits the canonical `agentflow.cli/1` envelope.
  pub format: String,
}

/// Per-include execution row in the run report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStepReport {
  /// Stable tag — matches the manifest. Operators can grep on
  /// this when wiring up monitoring.
  pub include: String,
  /// Resolved source path on the host (or "<postgres>" for the
  /// DB step — no single FS path).
  pub source: String,
  /// Artifact filename inside the bundle directory.
  pub artifact: String,
  /// `"executed"`, `"skipped"`, `"dry_run"`, or `"failed"`.
  pub status: String,
  /// Bytes written. `0` for dry-run and skipped steps.
  pub bytes: u64,
  /// Wall-clock duration of the underlying command, in ms.
  pub duration_ms: u64,
  /// One-line reason. Required for `skipped` / `failed`,
  /// optional otherwise.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

/// Full report of one `agentflow backup` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupReport {
  /// Manifest schema version — pinned to
  /// [`BUNDLE_MANIFEST_VERSION`].
  pub manifest_version: String,
  /// Resolved output directory (absolute when possible).
  pub output: String,
  /// Whether this was a `--dry-run` invocation.
  pub dry_run: bool,
  /// UTC start of the backup.
  pub started_at: DateTime<Utc>,
  /// UTC end of the backup (set even when one step failed).
  pub finished_at: DateTime<Utc>,
  /// Per-include rows in the order they ran.
  pub steps: Vec<BackupStepReport>,
  /// Aggregate counts derived from `steps` for at-a-glance reading.
  pub total_executed: usize,
  pub total_skipped: usize,
  pub total_failed: usize,
  /// `true` when every requested include succeeded (or was
  /// intentionally skipped). `false` means at least one `failed`.
  pub ok: bool,
}

/// What goes on disk as `<output>/manifest.json`. Strict subset
/// of [`BackupReport`] — only the fields a restore needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
  pub manifest_version: String,
  pub created_at: DateTime<Utc>,
  pub artifacts: Vec<BundleManifestArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifestArtifact {
  pub include: String,
  pub source: String,
  pub artifact: String,
  pub bytes: u64,
}

/// Entry point invoked by `main.rs`.
pub async fn execute(args: BackupArgs) -> Result<()> {
  let report = run_backup(&args).await?;
  emit_report(&report, &args.format)?;
  if !report.ok {
    std::process::exit(2);
  }
  Ok(())
}

async fn run_backup(args: &BackupArgs) -> Result<BackupReport> {
  let started_at = Utc::now();
  let output = canonicalize_output(&args.output)?;
  prepare_output_dir(&output, args.force, args.dry_run)?;

  let includes = if args.includes.is_empty() {
    BackupInclude::ALL.to_vec()
  } else {
    args.includes.clone()
  };

  let mut steps = Vec::with_capacity(includes.len());
  for include in &includes {
    let step = run_one(*include, &output, args).await;
    steps.push(step);
  }

  let total_executed = steps
    .iter()
    .filter(|s| s.status == "executed" || s.status == "dry_run")
    .count();
  let total_skipped = steps.iter().filter(|s| s.status == "skipped").count();
  let total_failed = steps.iter().filter(|s| s.status == "failed").count();

  let finished_at = Utc::now();
  let manifest = BundleManifest {
    manifest_version: BUNDLE_MANIFEST_VERSION.to_string(),
    created_at: started_at,
    artifacts: steps
      .iter()
      .filter(|s| s.status == "executed" || s.status == "dry_run")
      .map(|s| BundleManifestArtifact {
        include: s.include.clone(),
        source: s.source.clone(),
        artifact: s.artifact.clone(),
        bytes: s.bytes,
      })
      .collect(),
  };
  // The manifest is the source of truth for a future restore. We
  // write it even on partial failure so operators can diff the
  // "what we got" set against the "what we wanted" set; restore
  // is then free to error early if a required artifact is absent.
  if !args.dry_run {
    let manifest_path = output.join("manifest.json");
    let serialized =
      serde_json::to_string_pretty(&manifest).context("serialize bundle manifest")?;
    std::fs::write(&manifest_path, serialized)
      .with_context(|| format!("write manifest to {}", manifest_path.display()))?;
  }

  Ok(BackupReport {
    manifest_version: BUNDLE_MANIFEST_VERSION.to_string(),
    output: output.display().to_string(),
    dry_run: args.dry_run,
    started_at,
    finished_at,
    steps,
    total_executed,
    total_skipped,
    total_failed,
    ok: total_failed == 0,
  })
}

async fn run_one(include: BackupInclude, output: &Path, args: &BackupArgs) -> BackupStepReport {
  let started = std::time::Instant::now();
  match include {
    BackupInclude::Db => run_pg_dump(output, args, started).await,
    other => run_tar_dir(other, output, args, started).await,
  }
}

async fn run_pg_dump(
  output: &Path,
  args: &BackupArgs,
  started: std::time::Instant,
) -> BackupStepReport {
  let artifact = output.join(BackupInclude::Db.artifact_name());
  let url = args
    .database_url
    .clone()
    .or_else(|| std::env::var("DATABASE_URL").ok());

  let Some(url) = url else {
    return BackupStepReport {
      include: BackupInclude::Db.tag().to_string(),
      source: "<postgres>".to_string(),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "skipped".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(
        "no DATABASE_URL provided and `--database-url` not set; skipping DB dump".to_string(),
      ),
    };
  };

  if args.dry_run {
    return BackupStepReport {
      include: BackupInclude::Db.tag().to_string(),
      source: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "dry_run".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: None,
    };
  }

  if which_in_path("pg_dump").is_none() {
    return BackupStepReport {
      include: BackupInclude::Db.tag().to_string(),
      source: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(
        "pg_dump not found on PATH (install postgresql-client or postgresql via your package \
         manager)"
          .to_string(),
      ),
    };
  }

  let mut cmd = Command::new("pg_dump");
  // `--format=custom` is the only format `pg_restore` can take
  // selectively — never use `--format=plain` for prod backups.
  cmd
    .arg("--format=custom")
    .arg("--no-owner")
    .arg("--no-acl")
    .arg("--file")
    .arg(&artifact)
    .arg(&url);

  let outcome = cmd.output().await;
  let elapsed_ms = started.elapsed().as_millis() as u64;
  match outcome {
    Err(err) => BackupStepReport {
      include: BackupInclude::Db.tag().to_string(),
      source: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: elapsed_ms,
      reason: Some(format!("failed to spawn pg_dump: {err}")),
    },
    Ok(out) if !out.status.success() => BackupStepReport {
      include: BackupInclude::Db.tag().to_string(),
      source: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: elapsed_ms,
      reason: Some(format!(
        "pg_dump exited {}: {}",
        exit_label(&out.status),
        String::from_utf8_lossy(&out.stderr).trim()
      )),
    },
    Ok(_) => {
      let bytes = std::fs::metadata(&artifact).map(|m| m.len()).unwrap_or(0);
      BackupStepReport {
        include: BackupInclude::Db.tag().to_string(),
        source: redact_url(&url),
        artifact: BackupInclude::Db.artifact_name().to_string(),
        status: "executed".to_string(),
        bytes,
        duration_ms: elapsed_ms,
        reason: None,
      }
    }
  }
}

async fn run_tar_dir(
  include: BackupInclude,
  output: &Path,
  args: &BackupArgs,
  started: std::time::Instant,
) -> BackupStepReport {
  let source = match resolve_include_dir(include) {
    Some(p) => p,
    None => {
      return BackupStepReport {
        include: include.tag().to_string(),
        source: "<unresolved>".to_string(),
        artifact: include.artifact_name().to_string(),
        status: "skipped".to_string(),
        bytes: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        reason: Some(format!(
          "could not resolve {} (no env override and no home directory)",
          include.tag()
        )),
      };
    }
  };

  let artifact = output.join(include.artifact_name());
  let source_display = source.display().to_string();

  if !source.is_dir() {
    return BackupStepReport {
      include: include.tag().to_string(),
      source: source_display,
      artifact: include.artifact_name().to_string(),
      status: "skipped".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(format!(
        "source directory does not exist; nothing to back up for {}",
        include.tag()
      )),
    };
  }

  if args.dry_run {
    return BackupStepReport {
      include: include.tag().to_string(),
      source: source_display,
      artifact: include.artifact_name().to_string(),
      status: "dry_run".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: None,
    };
  }

  if which_in_path("tar").is_none() {
    return BackupStepReport {
      include: include.tag().to_string(),
      source: source_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some("tar not found on PATH".to_string()),
    };
  }

  let (parent, basename) = match (source.parent(), source.file_name()) {
    (Some(p), Some(name)) => (p.to_path_buf(), name.to_owned()),
    _ => {
      // Source has no parent (e.g. "/"): unusual enough that we
      // refuse rather than guess. Operators with weird overrides
      // can re-mount under a saner path.
      return BackupStepReport {
        include: include.tag().to_string(),
        source: source_display,
        artifact: include.artifact_name().to_string(),
        status: "failed".to_string(),
        bytes: 0,
        duration_ms: started.elapsed().as_millis() as u64,
        reason: Some(format!(
          "refusing to tar a top-level path: {}",
          source.display()
        )),
      };
    }
  };

  // tar -C <parent> -czf <artifact> <basename>
  // Anchoring under <parent> keeps the archive prefix relative to
  // the directory name, which is what a restore needs.
  let mut cmd = Command::new("tar");
  cmd
    .arg("-C")
    .arg(&parent)
    .arg("-czf")
    .arg(&artifact)
    .arg(&basename);

  let outcome = cmd.output().await;
  let elapsed_ms = started.elapsed().as_millis() as u64;
  match outcome {
    Err(err) => BackupStepReport {
      include: include.tag().to_string(),
      source: source_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: elapsed_ms,
      reason: Some(format!("failed to spawn tar: {err}")),
    },
    Ok(out) if !out.status.success() => BackupStepReport {
      include: include.tag().to_string(),
      source: source_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      bytes: 0,
      duration_ms: elapsed_ms,
      reason: Some(format!(
        "tar exited {}: {}",
        exit_label(&out.status),
        String::from_utf8_lossy(&out.stderr).trim()
      )),
    },
    Ok(_) => {
      let bytes = std::fs::metadata(&artifact).map(|m| m.len()).unwrap_or(0);
      BackupStepReport {
        include: include.tag().to_string(),
        source: source_display,
        artifact: include.artifact_name().to_string(),
        status: "executed".to_string(),
        bytes,
        duration_ms: elapsed_ms,
        reason: None,
      }
    }
  }
}

fn canonicalize_output(p: &Path) -> Result<PathBuf> {
  if let Ok(abs) = std::fs::canonicalize(p) {
    return Ok(abs);
  }
  // `canonicalize` fails for paths that don't exist yet — fall
  // back to a manual resolve against CWD.
  let absolute = if p.is_absolute() {
    p.to_path_buf()
  } else {
    std::env::current_dir().context("read current dir")?.join(p)
  };
  Ok(absolute)
}

fn prepare_output_dir(output: &Path, force: bool, dry_run: bool) -> Result<()> {
  if dry_run {
    // Plan mode never writes; don't create or check the dir, but
    // still refuse if it's a non-empty dir without --force so the
    // dry-run reflects what a real run would do.
    if output.exists() {
      let mut entries =
        std::fs::read_dir(output).with_context(|| format!("read {}", output.display()))?;
      if entries.next().is_some() && !force {
        anyhow::bail!(
          "{} is non-empty; pass --force to overwrite or pick a fresh directory",
          output.display()
        );
      }
    }
    return Ok(());
  }

  if !output.exists() {
    std::fs::create_dir_all(output)
      .with_context(|| format!("create output directory {}", output.display()))?;
    return Ok(());
  }
  if !output.is_dir() {
    anyhow::bail!("{} exists and is not a directory", output.display());
  }
  let non_empty = std::fs::read_dir(output)
    .with_context(|| format!("read {}", output.display()))?
    .next()
    .is_some();
  if non_empty && !force {
    anyhow::bail!(
      "{} is non-empty; pass --force to overwrite or pick a fresh directory",
      output.display()
    );
  }
  Ok(())
}

fn resolve_include_dir(include: BackupInclude) -> Option<PathBuf> {
  let home = dirs::home_dir();
  match include {
    BackupInclude::Db => None,
    BackupInclude::RunDir => {
      resolve_env_or_default(home.as_deref(), "AGENTFLOW_RUN_DIR", &["runs"])
    }
    BackupInclude::TraceDir => {
      resolve_env_or_default(home.as_deref(), "AGENTFLOW_TRACE_DIR", &["traces"])
    }
    BackupInclude::MarketplaceCache => resolve_env_or_default(
      home.as_deref(),
      "AGENTFLOW_MARKETPLACE_CACHE",
      &["marketplace", "cache"],
    ),
    BackupInclude::SkillsDir => {
      resolve_env_or_default(home.as_deref(), "AGENTFLOW_SKILLS_DIR", &["skills"])
    }
    BackupInclude::PluginsDir => {
      resolve_env_or_default(home.as_deref(), "AGENTFLOW_PLUGINS_DIR", &["plugins"])
    }
  }
}

fn resolve_env_or_default(home: Option<&Path>, env_var: &str, tail: &[&str]) -> Option<PathBuf> {
  if let Some(value) = std::env::var(env_var).ok().filter(|v| !v.trim().is_empty()) {
    return Some(PathBuf::from(value));
  }
  let home = home?;
  let mut p = home.join(".agentflow");
  for segment in tail {
    p.push(segment);
  }
  Some(p)
}

/// Mask the password component of a Postgres URL so log lines and
/// the report don't leak credentials. Anything we don't
/// confidently parse falls through to the unredacted string —
/// callers don't depend on this for security, just operator
/// hygiene.
fn redact_url(url: &str) -> String {
  // postgres://user:pass@host:5432/db
  if let Some(scheme_end) = url.find("://") {
    let scheme = &url[..scheme_end + 3];
    let rest = &url[scheme_end + 3..];
    if let Some(at) = rest.find('@')
      && let Some(colon) = rest[..at].find(':')
    {
      let user = &rest[..colon];
      let after_at = &rest[at..];
      return format!("{}{}:***{}", scheme, user, after_at);
    }
  }
  url.to_string()
}

fn which_in_path(exe: &str) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  for entry in std::env::split_paths(&path) {
    let candidate = entry.join(exe);
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  None
}

fn exit_label(status: &ExitStatus) -> String {
  if let Some(code) = status.code() {
    format!("with code {code}")
  } else {
    format!("by signal: {status}")
  }
}

fn emit_report(report: &BackupReport, format: &str) -> Result<()> {
  match format {
    "text" => emit_text(report),
    "json" => {
      let body = serde_json::to_string_pretty(report)?;
      println!("{body}");
      Ok(())
    }
    "json-envelope" => {
      let envelope = if report.ok {
        CliJsonEnvelope::ok("backup", report.clone())
      } else {
        let errors = report
          .steps
          .iter()
          .filter(|s| s.status == "failed")
          .map(|s| {
            format!(
              "{}: {}",
              s.include,
              s.reason
                .clone()
                .unwrap_or_else(|| "unspecified failure".into())
            )
          })
          .collect();
        CliJsonEnvelope::with_errors("backup", report.clone(), errors)
      };
      let body = serde_json::to_string_pretty(&envelope)?;
      println!("{body}");
      Ok(())
    }
    other => {
      anyhow::bail!("unknown --format value: {other}");
    }
  }
}

fn emit_text(report: &BackupReport) -> Result<()> {
  println!("agentflow backup");
  println!("  output:        {}", report.output);
  println!("  dry_run:       {}", report.dry_run);
  println!(
    "  duration:      {} ms",
    (report.finished_at - report.started_at).num_milliseconds()
  );
  println!();
  for step in &report.steps {
    println!(
      "  {:18} {:8} {:>12} bytes   {} ms",
      step.include, step.status, step.bytes, step.duration_ms
    );
    if let Some(reason) = &step.reason {
      println!("    └─ {reason}");
    }
  }
  println!();
  println!(
    "  summary: {} executed, {} skipped, {} failed   {}",
    report.total_executed,
    report.total_skipped,
    report.total_failed,
    if report.ok { "OK" } else { "FAILURES PRESENT" }
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_accepts_canonical_and_alias_forms() {
    assert_eq!(BackupInclude::parse("db"), Some(BackupInclude::Db));
    assert_eq!(BackupInclude::parse("DATABASE"), Some(BackupInclude::Db));
    assert_eq!(BackupInclude::parse("run-dir"), Some(BackupInclude::RunDir));
    assert_eq!(BackupInclude::parse("runs"), Some(BackupInclude::RunDir));
    assert_eq!(
      BackupInclude::parse("marketplace_cache"),
      Some(BackupInclude::MarketplaceCache)
    );
    assert_eq!(BackupInclude::parse("nope"), None);
  }

  #[test]
  fn tag_and_artifact_name_match_documented_layout() {
    assert_eq!(BackupInclude::Db.artifact_name(), "db.dump");
    assert_eq!(BackupInclude::Db.tag(), "db");
    assert_eq!(BackupInclude::RunDir.artifact_name(), "run_dir.tar.gz");
    assert_eq!(BackupInclude::SkillsDir.tag(), "skills_dir");
  }

  #[test]
  fn redact_url_masks_password() {
    assert_eq!(
      redact_url("postgres://alice:s3cret@db.example.com:5432/agentflow"),
      "postgres://alice:***@db.example.com:5432/agentflow"
    );
  }

  #[test]
  fn redact_url_leaves_passwordless_url_alone() {
    assert_eq!(
      redact_url("postgres://alice@localhost/agentflow"),
      "postgres://alice@localhost/agentflow"
    );
  }

  #[test]
  fn redact_url_leaves_non_postgres_strings_alone() {
    assert_eq!(redact_url("not a url"), "not a url");
  }

  #[test]
  fn prepare_output_dir_refuses_non_empty_without_force() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("preexisting"), "data").unwrap();
    let err = prepare_output_dir(tmp.path(), false, false).unwrap_err();
    let msg = err.to_string();
    assert!(
      msg.contains("non-empty"),
      "expected non-empty refusal, got: {msg}"
    );
  }

  #[test]
  fn prepare_output_dir_accepts_non_empty_with_force() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("preexisting"), "data").unwrap();
    let ok = prepare_output_dir(tmp.path(), true, false);
    assert!(ok.is_ok(), "force should accept non-empty: {ok:?}");
  }

  #[test]
  fn prepare_output_dir_creates_missing_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let new_dir = tmp.path().join("brand_new");
    prepare_output_dir(&new_dir, false, false).unwrap();
    assert!(new_dir.is_dir(), "missing dir should be created");
  }

  #[test]
  fn dry_run_does_not_create_missing_output_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let new_dir = tmp.path().join("plan_only");
    prepare_output_dir(&new_dir, false, true).unwrap();
    assert!(
      !new_dir.is_dir(),
      "dry-run must not create the output directory"
    );
  }

  #[tokio::test]
  async fn dry_run_includes_all_when_includes_is_empty() {
    let tmp = tempfile::TempDir::new().unwrap();
    let args = BackupArgs {
      output: tmp.path().join("plan"),
      database_url: Some("postgres://u:p@localhost/x".into()),
      dry_run: true,
      force: false,
      includes: vec![],
      format: "json".into(),
    };
    let report = run_backup(&args).await.unwrap();
    assert_eq!(report.steps.len(), BackupInclude::ALL.len());
    // Every dry-run step is `dry_run` or `skipped` — never executed
    // or failed.
    for step in &report.steps {
      assert!(
        matches!(step.status.as_str(), "dry_run" | "skipped"),
        "unexpected status {} for {}: {:?}",
        step.status,
        step.include,
        step.reason
      );
    }
    assert!(report.ok, "dry-run with no failures must report ok");
  }

  #[tokio::test]
  async fn dry_run_with_explicit_include_skips_others() {
    let tmp = tempfile::TempDir::new().unwrap();
    let args = BackupArgs {
      output: tmp.path().join("plan_subset"),
      database_url: None,
      dry_run: true,
      force: false,
      includes: vec![BackupInclude::SkillsDir],
      format: "json".into(),
    };
    let report = run_backup(&args).await.unwrap();
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].include, "skills_dir");
  }

  #[tokio::test]
  async fn db_step_skips_cleanly_without_database_url() {
    let tmp = tempfile::TempDir::new().unwrap();
    let args = BackupArgs {
      output: tmp.path().join("no_db"),
      database_url: None,
      dry_run: false,
      force: false,
      includes: vec![BackupInclude::Db],
      format: "json".into(),
    };
    // SAFETY: the test mutates DATABASE_URL only within its own
    // scope; no other tests in this binary depend on it.
    let saved = std::env::var("DATABASE_URL").ok();
    // SAFETY: remove_var/set_var are unsafe in 2024 edition because
    // they mutate process-global state; this test deliberately
    // isolates the env access.
    unsafe { std::env::remove_var("DATABASE_URL") };
    let report = run_backup(&args).await.unwrap();
    if let Some(value) = saved {
      unsafe { std::env::set_var("DATABASE_URL", value) };
    }
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].status, "skipped");
    assert!(report.ok, "skipped step is not a failure");
  }
}
