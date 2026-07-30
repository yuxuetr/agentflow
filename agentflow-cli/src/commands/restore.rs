//! `agentflow restore` — consumes an `agentflow backup` bundle's
//! `manifest.json` and reverses each artifact (T2.2): `pg_restore` for
//! `db.dump`, `tar -xzf` for the directory tarballs, back into the
//! same paths `agentflow backup` read them from (via the same
//! env-var/default resolution — see `commands::backup::resolve_include_dir`).
//!
//! Restore order is fixed regardless of `--include` order or the
//! manifest's artifact order, mirroring `docs/SERVER_BACKUP_RESTORE.md`'s
//! documented sequencing rationale: Postgres first (its rows reference
//! run_dir/trace_dir paths), independent caches next, then trace_dir,
//! then run_dir last (restoring it before Postgres produces orphan
//! trees the next cleanup sweep would delete).
//!
//! What this command **does not** do:
//!
//! - Verify the restored Postgres schema version matches what
//!   `agentflow serve` expects beyond what `pg_restore` itself
//!   guarantees (the dump carries `_sqlx_migrations`, so a restore of
//!   a fully-migrated backup lands at that same schema version — see
//!   `docs/SERVER_BACKUP_RESTORE.md`).
//! - In-Rust tar / pg_restore, for the same reasons `agentflow backup`
//!   shells out rather than reimplementing either tool.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::commands::backup::{
  BUNDLE_MANIFEST_VERSION, BackupInclude, BundleManifest, BundleManifestArtifact, exit_label,
  redact_url, resolve_include_dir, sha256_file, which_in_path,
};
use crate::json_envelope::CliJsonEnvelope;

/// Fixed restore order — see the module doc for the rationale. Does
/// NOT depend on `--include` order or the manifest's own artifact
/// order.
const RESTORE_ORDER: [BackupInclude; 6] = [
  BackupInclude::Db,
  BackupInclude::MarketplaceCache,
  BackupInclude::SkillsDir,
  BackupInclude::PluginsDir,
  BackupInclude::TraceDir,
  BackupInclude::RunDir,
];

/// Parsed CLI arguments for `agentflow restore`.
#[derive(Debug, Clone)]
pub struct RestoreArgs {
  /// Bundle directory produced by a prior `agentflow backup --output <path>`.
  pub input: PathBuf,
  /// Postgres URL to restore into. Falls back to `DATABASE_URL` env.
  /// Only consulted when the `db` include is in the active set and
  /// the manifest has a `db` artifact.
  pub database_url: Option<String>,
  /// Plan-only mode — parse the manifest and print what would run,
  /// mutate nothing.
  pub dry_run: bool,
  /// Overwrite an existing target directory for a filesystem include
  /// by removing it first. Without this flag, a target directory that
  /// already exists fails that step rather than merging tar contents
  /// into stale files.
  pub force: bool,
  /// Explicit include set. Empty means "every include present in the
  /// manifest" (the documented default, mirroring `agentflow backup`).
  pub includes: Vec<BackupInclude>,
  /// Output format for the report — same three values as `agentflow backup`.
  pub format: String,
  /// Restore an artifact even when its recomputed SHA-256 does not
  /// match the manifest's recorded hash (U0.2). Off by default —
  /// a mismatch fails that step rather than silently feeding a
  /// possibly corrupted/tampered dump into `pg_restore --clean` or a
  /// possibly path-traversing archive into `tar`.
  pub skip_integrity_check: bool,
}

/// Per-include execution row in the restore report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreStepReport {
  pub include: String,
  /// Resolved destination path on the host (or `"<postgres>"` for the
  /// DB step).
  pub target: String,
  pub artifact: String,
  /// `"executed"`, `"skipped"`, `"dry_run"`, or `"failed"`.
  pub status: String,
  pub duration_ms: u64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

/// Full report of one `agentflow restore` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreReport {
  pub manifest_version: String,
  pub input: String,
  pub dry_run: bool,
  pub started_at: DateTime<Utc>,
  pub finished_at: DateTime<Utc>,
  pub steps: Vec<RestoreStepReport>,
  pub total_executed: usize,
  pub total_skipped: usize,
  pub total_failed: usize,
  /// `true` when every requested include succeeded (or was
  /// intentionally skipped). `false` means at least one `failed`.
  pub ok: bool,
}

/// Entry point invoked by `main.rs`.
pub async fn execute(args: RestoreArgs) -> Result<()> {
  let report = run_restore(&args).await?;
  emit_report(&report, &args.format)?;
  if !report.ok {
    std::process::exit(2);
  }
  Ok(())
}

async fn run_restore(args: &RestoreArgs) -> Result<RestoreReport> {
  let started_at = Utc::now();
  let input = canonicalize_input(&args.input)?;
  let manifest = load_manifest(&input)?;

  if manifest.manifest_version != BUNDLE_MANIFEST_VERSION {
    anyhow::bail!(
      "bundle manifest_version '{}' does not match this binary's supported version '{}' \
       — restore with a matching `agentflow` build",
      manifest.manifest_version,
      BUNDLE_MANIFEST_VERSION
    );
  }

  let requested: Vec<BackupInclude> = if args.includes.is_empty() {
    BackupInclude::ALL.to_vec()
  } else {
    args.includes.clone()
  };

  let mut steps = Vec::new();
  for include in RESTORE_ORDER {
    if !requested.contains(&include) {
      continue;
    }
    let Some(artifact_meta) = manifest
      .artifacts
      .iter()
      .find(|a| a.include == include.tag())
    else {
      steps.push(RestoreStepReport {
        include: include.tag().to_string(),
        target: target_display(include),
        artifact: include.artifact_name().to_string(),
        status: "skipped".to_string(),
        duration_ms: 0,
        reason: Some("not present in this bundle's manifest".to_string()),
      });
      continue;
    };
    steps.push(restore_one(include, &input, args, artifact_meta).await);
  }

  let total_executed = steps
    .iter()
    .filter(|s| s.status == "executed" || s.status == "dry_run")
    .count();
  let total_skipped = steps.iter().filter(|s| s.status == "skipped").count();
  let total_failed = steps.iter().filter(|s| s.status == "failed").count();
  let finished_at = Utc::now();

  Ok(RestoreReport {
    manifest_version: manifest.manifest_version,
    input: input.display().to_string(),
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

fn load_manifest(input: &Path) -> Result<BundleManifest> {
  let manifest_path = input.join("manifest.json");
  let raw = std::fs::read_to_string(&manifest_path).with_context(|| {
    format!(
      "read bundle manifest at {} — is '{}' a directory produced by `agentflow backup`?",
      manifest_path.display(),
      input.display()
    )
  })?;
  serde_json::from_str(&raw)
    .with_context(|| format!("parse bundle manifest at {}", manifest_path.display()))
}

fn target_display(include: BackupInclude) -> String {
  match include {
    BackupInclude::Db => "<postgres>".to_string(),
    other => resolve_include_dir(other)
      .map(|p| p.display().to_string())
      .unwrap_or_else(|| "<unresolved>".to_string()),
  }
}

async fn restore_one(
  include: BackupInclude,
  input: &Path,
  args: &RestoreArgs,
  artifact_meta: &BundleManifestArtifact,
) -> RestoreStepReport {
  let started = std::time::Instant::now();
  match include {
    BackupInclude::Db => restore_db(input, args, artifact_meta, started).await,
    other => restore_dir(other, input, args, artifact_meta, started).await,
  }
}

/// Recomputes the on-disk artifact's SHA-256 and compares it against
/// the manifest's recorded hash (U0.2), before the caller executes
/// `pg_restore`/`tar` against it.
///
/// - `Ok(None)` — either verified (hash matches) or there is nothing
///   to verify (manifest predates U0.2); the caller proceeds silently.
/// - `Ok(Some(warning))` — the caller proceeds but should surface
///   `warning` in the step's `reason` (legacy manifest with no
///   recorded hash, or a mismatch the caller explicitly opted past
///   via `--skip-integrity-check`).
/// - `Err(reason)` — the caller must fail this step without touching
///   `pg_restore`/`tar`.
fn verify_artifact_integrity(
  artifact: &Path,
  artifact_meta: &BundleManifestArtifact,
  skip_check: bool,
) -> Result<Option<String>, String> {
  let Some(expected) = artifact_meta.sha256.as_deref() else {
    return Ok(Some(
      "integrity check skipped: this bundle's manifest predates SHA-256 recording (U0.2) and \
       has no hash to verify against"
        .to_string(),
    ));
  };
  let actual = sha256_file(artifact)
    .map_err(|err| format!("failed to hash artifact for integrity check: {err}"))?;
  if actual == expected {
    return Ok(None);
  }
  if skip_check {
    return Ok(Some(format!(
      "INTEGRITY CHECK FAILED (expected {expected}, computed {actual}) but \
       --skip-integrity-check was passed; restoring an artifact that does not match the \
       manifest's recorded checksum"
    )));
  }
  Err(format!(
    "artifact integrity check failed: manifest recorded {expected}, computed {actual} — the \
     artifact may be corrupted or tampered with; pass --skip-integrity-check to restore anyway \
     (not recommended)"
  ))
}

async fn restore_db(
  input: &Path,
  args: &RestoreArgs,
  artifact_meta: &BundleManifestArtifact,
  started: std::time::Instant,
) -> RestoreStepReport {
  let artifact = input.join(BackupInclude::Db.artifact_name());
  let url = args
    .database_url
    .clone()
    .or_else(|| std::env::var("DATABASE_URL").ok());

  let Some(url) = url else {
    return RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: "<postgres>".to_string(),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "skipped".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(
        "no DATABASE_URL provided and `--database-url` not set; skipping DB restore".to_string(),
      ),
    };
  };

  if args.dry_run {
    return RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "dry_run".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: None,
    };
  }

  if !artifact.is_file() {
    return RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(format!(
        "manifest references {} but the file is missing from the bundle",
        artifact.display()
      )),
    };
  }

  if which_in_path("pg_restore").is_none() {
    return RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(
        "pg_restore not found on PATH (install postgresql-client or postgresql via your package \
         manager)"
          .to_string(),
      ),
    };
  }

  // U0.2: verify the dump on disk still matches what `agentflow
  // backup` recorded, before feeding it to `pg_restore --clean
  // --if-exists` (which deletes existing objects unconditionally).
  let integrity_warning =
    match verify_artifact_integrity(&artifact, artifact_meta, args.skip_integrity_check) {
      Ok(warning) => warning,
      Err(reason) => {
        return RestoreStepReport {
          include: BackupInclude::Db.tag().to_string(),
          target: redact_url(&url),
          artifact: BackupInclude::Db.artifact_name().to_string(),
          status: "failed".to_string(),
          duration_ms: started.elapsed().as_millis() as u64,
          reason: Some(reason),
        };
      }
    };

  let mut cmd = Command::new("pg_restore");
  // `--clean --if-exists` lets restore target either a truly fresh
  // database (the DROP IF EXISTS no-ops) or a previously-restored one
  // (idempotent re-restore) without the caller having to know which.
  cmd
    .arg("--clean")
    .arg("--if-exists")
    .arg("--no-owner")
    .arg("--no-acl")
    .arg("--dbname")
    .arg(&url)
    .arg(&artifact);

  let outcome = cmd.output().await;
  let elapsed_ms = started.elapsed().as_millis() as u64;
  match outcome {
    Err(err) => RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: elapsed_ms,
      reason: Some(format!("failed to spawn pg_restore: {err}")),
    },
    Ok(out) if !out.status.success() => RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: elapsed_ms,
      reason: Some(format!(
        "pg_restore exited {}: {}",
        exit_label(&out.status),
        String::from_utf8_lossy(&out.stderr).trim()
      )),
    },
    Ok(_) => RestoreStepReport {
      include: BackupInclude::Db.tag().to_string(),
      target: redact_url(&url),
      artifact: BackupInclude::Db.artifact_name().to_string(),
      status: "executed".to_string(),
      duration_ms: elapsed_ms,
      reason: integrity_warning,
    },
  }
}

/// Pure guard, no filesystem access — checked before any destructive
/// operation touches `target`. Extracted out of `restore_dir` so the
/// safety condition itself can be exercised by tests without ever
/// performing a real filesystem mutation against a root-like path
/// (only the literal filesystem root satisfies `parent().is_none()`,
/// so an integration test that actually reached the deletion branch
/// would have to operate on real `/`).
fn reject_unsafe_restore_target(target: &Path) -> Option<String> {
  if target.parent().is_none() {
    Some(format!(
      "refusing to restore into a top-level path: {}",
      target.display()
    ))
  } else {
    None
  }
}

async fn restore_dir(
  include: BackupInclude,
  input: &Path,
  args: &RestoreArgs,
  artifact_meta: &BundleManifestArtifact,
  started: std::time::Instant,
) -> RestoreStepReport {
  let artifact = input.join(include.artifact_name());
  let Some(target) = resolve_include_dir(include) else {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: "<unresolved>".to_string(),
      artifact: include.artifact_name().to_string(),
      status: "skipped".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(format!(
        "could not resolve target dir for {} (no env override and no home directory)",
        include.tag()
      )),
    };
  };
  let target_display = target.display().to_string();

  if !artifact.is_file() {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(format!(
        "manifest references {} but the file is missing from the bundle",
        artifact.display()
      )),
    };
  }

  if args.dry_run {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "dry_run".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: None,
    };
  }

  if which_in_path("tar").is_none() {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some("tar not found on PATH".to_string()),
    };
  }

  // U0.2: verify the tarball on disk still matches what `agentflow
  // backup` recorded, before extracting it — an unverified archive
  // that path-traverses or symlinks outside `target` is otherwise
  // entirely at the mercy of the host's unpinned `tar` binary.
  let integrity_warning =
    match verify_artifact_integrity(&artifact, artifact_meta, args.skip_integrity_check) {
      Ok(warning) => warning,
      Err(reason) => {
        return RestoreStepReport {
          include: include.tag().to_string(),
          target: target_display,
          artifact: include.artifact_name().to_string(),
          status: "failed".to_string(),
          duration_ms: started.elapsed().as_millis() as u64,
          reason: Some(reason),
        };
      }
    };

  // Safety guard MUST run before any destructive filesystem operation
  // below (`remove_dir_all` in particular) — never after. A
  // misconfigured env override (`AGENTFLOW_RUN_DIR=/`, etc.) combined
  // with `--force` must be rejected before we touch the target, not
  // after we've already deleted it.
  if let Some(reason) = reject_unsafe_restore_target(&target) {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(reason),
    };
  }

  if target.exists() {
    if !args.force {
      return RestoreStepReport {
        include: include.tag().to_string(),
        target: target_display,
        artifact: include.artifact_name().to_string(),
        status: "failed".to_string(),
        duration_ms: started.elapsed().as_millis() as u64,
        reason: Some(format!(
          "target directory {} already exists; pass --force to overwrite",
          target.display()
        )),
      };
    }
    // Clear rather than merge: extracting on top of a stale tree
    // would leave orphaned files the backup never had.
    if let Err(err) = std::fs::remove_dir_all(&target) {
      return RestoreStepReport {
        include: include.tag().to_string(),
        target: target_display,
        artifact: include.artifact_name().to_string(),
        status: "failed".to_string(),
        duration_ms: started.elapsed().as_millis() as u64,
        reason: Some(format!(
          "failed to clear existing target directory before restore: {err}"
        )),
      };
    }
  }

  if let Err(err) = std::fs::create_dir_all(&target) {
    return RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: started.elapsed().as_millis() as u64,
      reason: Some(format!(
        "failed to create target directory {}: {err}",
        target.display()
      )),
    };
  }

  // tar -C <target> --strip-components=1 -xzf <artifact>
  // `agentflow backup` anchors the archive's top-level entry to the
  // *source* directory's basename (`backup::run_tar_dir`'s `tar -C
  // <parent> -czf <artifact> <basename>`), which will not generally
  // match this restore's target basename (different host, renamed
  // env override, etc.). `--strip-components=1` drops that top-level
  // prefix from every archived path so extraction lands directly in
  // <target> regardless of what it was called at backup time.
  let mut cmd = Command::new("tar");
  cmd
    .arg("-C")
    .arg(&target)
    .arg("--strip-components=1")
    .arg("-xzf")
    .arg(&artifact);

  let outcome = cmd.output().await;
  let elapsed_ms = started.elapsed().as_millis() as u64;
  match outcome {
    Err(err) => RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: elapsed_ms,
      reason: Some(format!("failed to spawn tar: {err}")),
    },
    Ok(out) if !out.status.success() => RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "failed".to_string(),
      duration_ms: elapsed_ms,
      reason: Some(format!(
        "tar exited {}: {}",
        exit_label(&out.status),
        String::from_utf8_lossy(&out.stderr).trim()
      )),
    },
    Ok(_) => RestoreStepReport {
      include: include.tag().to_string(),
      target: target_display,
      artifact: include.artifact_name().to_string(),
      status: "executed".to_string(),
      duration_ms: elapsed_ms,
      reason: integrity_warning,
    },
  }
}

fn canonicalize_input(p: &Path) -> Result<PathBuf> {
  std::fs::canonicalize(p)
    .with_context(|| format!("resolve restore input directory {}", p.display()))
}

fn emit_report(report: &RestoreReport, format: &str) -> Result<()> {
  match format {
    "text" => emit_text(report),
    "json" => {
      let body = serde_json::to_string_pretty(report)?;
      println!("{body}");
      Ok(())
    }
    "json-envelope" => {
      let envelope = if report.ok {
        CliJsonEnvelope::ok("restore", report.clone())
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
        CliJsonEnvelope::with_errors("restore", report.clone(), errors)
      };
      let body = serde_json::to_string_pretty(&envelope)?;
      println!("{body}");
      Ok(())
    }
    other => anyhow::bail!("unknown --format value: {other}"),
  }
}

fn emit_text(report: &RestoreReport) -> Result<()> {
  println!("agentflow restore");
  println!("  input:         {}", report.input);
  println!("  dry_run:       {}", report.dry_run);
  println!(
    "  duration:      {} ms",
    (report.finished_at - report.started_at).num_milliseconds()
  );
  println!();
  for step in &report.steps {
    println!("  {:18} {:8} {}", step.include, step.status, step.target);
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
  use crate::commands::backup::BundleManifestArtifact;

  /// Writes `manifest.json` *and* an empty placeholder file for each
  /// declared artifact, so the artifact-presence check (unconditional
  /// in both dry-run and real mode — see `restore_db`/`restore_dir`)
  /// doesn't fail every dry-run test. `missing_artifact_file_on_disk_fails_that_step`
  /// deliberately skips this helper to exercise that check.
  fn write_manifest(dir: &Path, artifacts: &[BackupInclude]) {
    let manifest = BundleManifest {
      manifest_version: BUNDLE_MANIFEST_VERSION.to_string(),
      created_at: Utc::now(),
      artifacts: artifacts
        .iter()
        .map(|include| BundleManifestArtifact {
          include: include.tag().to_string(),
          source: "<test>".to_string(),
          artifact: include.artifact_name().to_string(),
          bytes: 0,
          // Legacy-manifest shape on purpose: most tests in this
          // module predate U0.2 and don't care about integrity
          // verification, so they get the "cannot verify, no hash
          // recorded" path rather than a real hash check.
          sha256: None,
        })
        .collect(),
    };
    std::fs::write(
      dir.join("manifest.json"),
      serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    for include in artifacts {
      std::fs::write(dir.join(include.artifact_name()), b"placeholder").unwrap();
    }
  }

  /// Like [`write_manifest`], but records the real SHA-256 of each
  /// artifact's content (U0.2) instead of leaving it unset — for
  /// tests that exercise integrity verification.
  fn write_manifest_with_content(dir: &Path, artifacts: &[(BackupInclude, &[u8])]) {
    let mut manifest_artifacts = Vec::new();
    for (include, content) in artifacts {
      let path = dir.join(include.artifact_name());
      std::fs::write(&path, content).unwrap();
      manifest_artifacts.push(BundleManifestArtifact {
        include: include.tag().to_string(),
        source: "<test>".to_string(),
        artifact: include.artifact_name().to_string(),
        bytes: content.len() as u64,
        sha256: Some(sha256_file(&path).unwrap()),
      });
    }
    let manifest = BundleManifest {
      manifest_version: BUNDLE_MANIFEST_VERSION.to_string(),
      created_at: Utc::now(),
      artifacts: manifest_artifacts,
    };
    std::fs::write(
      dir.join("manifest.json"),
      serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
  }

  fn base_args(input: PathBuf) -> RestoreArgs {
    RestoreArgs {
      input,
      database_url: None,
      dry_run: true,
      force: false,
      includes: vec![],
      format: "json".into(),
      skip_integrity_check: false,
    }
  }

  #[test]
  fn restore_order_covers_every_backup_include_exactly_once() {
    let mut sorted = RESTORE_ORDER.to_vec();
    sorted.sort_by_key(|i| i.tag());
    let mut expected = BackupInclude::ALL.to_vec();
    expected.sort_by_key(|i| i.tag());
    assert_eq!(sorted, expected);
  }

  #[test]
  fn restore_order_puts_db_first_and_run_dir_last() {
    assert_eq!(RESTORE_ORDER[0], BackupInclude::Db);
    assert_eq!(
      RESTORE_ORDER[RESTORE_ORDER.len() - 1],
      BackupInclude::RunDir
    );
  }

  #[tokio::test]
  async fn missing_manifest_errors_before_touching_any_artifact() {
    let tmp = tempfile::TempDir::new().unwrap();
    let args = base_args(tmp.path().to_path_buf());
    let err = run_restore(&args).await.unwrap_err();
    assert!(
      err.to_string().contains("manifest"),
      "expected a manifest-related error, got: {err}"
    );
  }

  #[tokio::test]
  async fn mismatched_manifest_version_is_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let manifest = serde_json::json!({
      "manifest_version": "agentflow.backup/999",
      "created_at": Utc::now().to_rfc3339(),
      "artifacts": [],
    });
    std::fs::write(
      tmp.path().join("manifest.json"),
      serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let args = base_args(tmp.path().to_path_buf());
    let err = run_restore(&args).await.unwrap_err();
    assert!(err.to_string().contains("manifest_version"));
  }

  #[tokio::test]
  async fn dry_run_reports_every_manifest_artifact_without_mutating() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_manifest(tmp.path(), &[BackupInclude::Db, BackupInclude::SkillsDir]);
    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = true;
    args.database_url = Some("postgres://u:p@localhost/x".into());
    // `includes` empty = every include in `BackupInclude::ALL` is
    // *requested*; the two present in the manifest come back `dry_run`,
    // the four absent from this bundle come back `skipped`.
    let report = run_restore(&args).await.unwrap();
    assert_eq!(report.steps.len(), BackupInclude::ALL.len());
    for step in &report.steps {
      let expected = if step.include == "db" || step.include == "skills_dir" {
        "dry_run"
      } else {
        "skipped"
      };
      assert_eq!(step.status, expected, "step: {step:?}");
    }
    assert!(report.ok);
  }

  #[tokio::test]
  async fn artifact_not_in_manifest_is_skipped_not_failed() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_manifest(tmp.path(), &[BackupInclude::SkillsDir]);
    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = true;
    // Db is not in the manifest's artifact list.
    args.includes = vec![BackupInclude::Db, BackupInclude::SkillsDir];
    let report = run_restore(&args).await.unwrap();
    let db_step = report.steps.iter().find(|s| s.include == "db").unwrap();
    assert_eq!(db_step.status, "skipped");
    assert!(
      db_step
        .reason
        .as_deref()
        .unwrap()
        .contains("not present in this bundle's manifest")
    );
    assert!(report.ok, "a skipped-not-in-manifest step is not a failure");
  }

  #[tokio::test]
  async fn missing_artifact_file_on_disk_fails_that_step() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Manifest claims skills_dir was backed up, but the tarball was
    // never actually written (corrupt/partial bundle).
    write_manifest(tmp.path(), &[BackupInclude::SkillsDir]);
    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = false;
    args.includes = vec![BackupInclude::SkillsDir];
    let report = run_restore(&args).await.unwrap();
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].status, "failed");
    assert!(!report.ok);
  }

  #[tokio::test]
  async fn includes_filter_restricts_to_requested_set() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_manifest(
      tmp.path(),
      &[
        BackupInclude::Db,
        BackupInclude::SkillsDir,
        BackupInclude::RunDir,
      ],
    );
    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = true;
    args.database_url = Some("postgres://u:p@localhost/x".into());
    args.includes = vec![BackupInclude::Db];
    let report = run_restore(&args).await.unwrap();
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].include, "db");
  }

  // U0.1 regression coverage: `restore_dir` used to call
  // `std::fs::remove_dir_all(&target)` before checking
  // `target.parent().is_none()`, so a misconfigured env override
  // (e.g. `AGENTFLOW_RUN_DIR=/`) combined with `--force` would delete
  // the filesystem root before the guard ever ran. The guard is now
  // `reject_unsafe_restore_target`, called unconditionally as the
  // first statement in `restore_dir`, before the `target.exists()` /
  // `remove_dir_all` branch. These tests exercise the guard directly
  // — pure `Path` inspection, no filesystem access — so they can
  // assert the root-path case is rejected without ever actually
  // operating on a real filesystem root (`Path::new("/")` never
  // touches disk; only a broken guard reaching `remove_dir_all` would).

  #[test]
  fn reject_unsafe_restore_target_rejects_filesystem_root() {
    let reason = reject_unsafe_restore_target(Path::new("/"));
    assert!(
      reason.is_some(),
      "a top-level path must be rejected before any destructive operation"
    );
    assert!(reason.unwrap().contains("top-level path"));
  }

  #[test]
  fn reject_unsafe_restore_target_allows_nested_path() {
    assert!(
      reject_unsafe_restore_target(Path::new("/home/user/.agentflow/skills")).is_none(),
      "an ordinary nested restore target must not be rejected"
    );
  }

  #[tokio::test]
  async fn restore_dir_fails_closed_before_deleting_an_unresolvable_root_target() {
    // Simulates the disaster scenario end-to-end through `restore_dir`
    // itself (not just the pure guard): `AGENTFLOW_SKILLS_DIR=/`, with
    // `--force`, on a target that "exists" (the real root always
    // does). If the ordering bug ever regressed, this is exactly the
    // call that would previously reach `remove_dir_all("/")`. The
    // fixed guard now runs first and returns a `failed` step without
    // ever touching `target.exists()` / `remove_dir_all` / `tar`, so
    // running this against the real root is safe by construction —
    // that safety is exactly what this test is verifying.
    let tmp = tempfile::TempDir::new().unwrap();
    write_manifest(tmp.path(), &[BackupInclude::SkillsDir]);

    let saved = std::env::var("AGENTFLOW_SKILLS_DIR").ok();
    // SAFETY: this test isolates AGENTFLOW_SKILLS_DIR to its own
    // scope and restores the prior value before returning, matching
    // the existing DATABASE_URL isolation pattern in
    // `commands::backup`'s tests.
    unsafe { std::env::set_var("AGENTFLOW_SKILLS_DIR", "/") };

    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = false;
    args.force = true;
    args.includes = vec![BackupInclude::SkillsDir];
    let report = run_restore(&args).await.unwrap();

    match saved {
      Some(value) => unsafe { std::env::set_var("AGENTFLOW_SKILLS_DIR", value) },
      None => unsafe { std::env::remove_var("AGENTFLOW_SKILLS_DIR") },
    }

    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].status, "failed");
    assert!(
      report.steps[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("top-level path"),
      "expected the top-level-path guard to fire, got: {:?}",
      report.steps[0].reason
    );
    assert!(std::path::Path::new("/").is_dir(), "sanity: root untouched");
  }

  // U0.2 regression coverage: `agentflow backup` now records a
  // `sha256:<hex>` for every artifact in the manifest, and
  // `agentflow restore` recomputes + compares it before running
  // `pg_restore`/`tar` against the artifact on disk.

  #[test]
  fn verify_artifact_integrity_accepts_a_matching_hash() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("skills_dir.tar.gz");
    std::fs::write(&path, b"original content").unwrap();
    let meta = BundleManifestArtifact {
      include: "skills_dir".to_string(),
      source: "<test>".to_string(),
      artifact: "skills_dir.tar.gz".to_string(),
      bytes: 17,
      sha256: Some(sha256_file(&path).unwrap()),
    };
    let outcome = verify_artifact_integrity(&path, &meta, false).unwrap();
    assert!(
      outcome.is_none(),
      "a matching hash must verify silently, got: {outcome:?}"
    );
  }

  #[test]
  fn verify_artifact_integrity_rejects_a_tampered_artifact_by_default() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("skills_dir.tar.gz");
    std::fs::write(&path, b"original content").unwrap();
    let meta = BundleManifestArtifact {
      include: "skills_dir".to_string(),
      source: "<test>".to_string(),
      artifact: "skills_dir.tar.gz".to_string(),
      bytes: 17,
      sha256: Some(sha256_file(&path).unwrap()),
    };
    // Tamper with the artifact after the "manifest" recorded its hash.
    std::fs::write(&path, b"tampered content!").unwrap();
    let err = verify_artifact_integrity(&path, &meta, false).unwrap_err();
    assert!(
      err.contains("integrity check failed"),
      "expected an integrity-check error, got: {err}"
    );
  }

  #[test]
  fn verify_artifact_integrity_allows_a_tampered_artifact_when_skip_flag_is_set() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("skills_dir.tar.gz");
    std::fs::write(&path, b"original content").unwrap();
    let meta = BundleManifestArtifact {
      include: "skills_dir".to_string(),
      source: "<test>".to_string(),
      artifact: "skills_dir.tar.gz".to_string(),
      bytes: 17,
      sha256: Some(sha256_file(&path).unwrap()),
    };
    std::fs::write(&path, b"tampered content!").unwrap();
    let warning = verify_artifact_integrity(&path, &meta, true)
      .unwrap()
      .expect("--skip-integrity-check must still surface a warning, not silence it");
    assert!(warning.contains("INTEGRITY CHECK FAILED"));
  }

  #[test]
  fn verify_artifact_integrity_skips_verification_for_a_legacy_manifest_with_no_hash() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("skills_dir.tar.gz");
    std::fs::write(&path, b"content").unwrap();
    let meta = BundleManifestArtifact {
      include: "skills_dir".to_string(),
      source: "<test>".to_string(),
      artifact: "skills_dir.tar.gz".to_string(),
      bytes: 7,
      sha256: None,
    };
    let warning = verify_artifact_integrity(&path, &meta, false)
      .unwrap()
      .expect("a legacy (pre-U0.2) manifest with no recorded hash must warn, not silently pass");
    assert!(warning.contains("predates SHA-256"));
  }

  #[tokio::test]
  async fn restore_dir_rejects_a_tampered_artifact_before_ever_invoking_tar() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_manifest_with_content(
      tmp.path(),
      &[(BackupInclude::SkillsDir, b"original tarball bytes")],
    );
    // Tamper with the on-disk artifact after the manifest recorded
    // its hash — simulates a corrupted or maliciously modified bundle.
    std::fs::write(
      tmp.path().join(BackupInclude::SkillsDir.artifact_name()),
      b"not the original bytes",
    )
    .unwrap();

    let mut args = base_args(tmp.path().to_path_buf());
    args.dry_run = false;
    args.includes = vec![BackupInclude::SkillsDir];
    let report = run_restore(&args).await.unwrap();

    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].status, "failed");
    assert!(
      report.steps[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("integrity check failed"),
      "got: {:?}",
      report.steps[0].reason
    );
    assert!(!report.ok);
  }

  #[tokio::test]
  async fn restore_dir_restores_normally_when_the_artifact_matches_its_recorded_hash() {
    if which_in_path("tar").is_none() {
      eprintln!(
        "skipping restore_dir_restores_normally_when_the_artifact_matches_its_recorded_hash — \
         tar not on PATH"
      );
      return;
    }

    let work = tempfile::TempDir::new().unwrap();
    let source_dir = work.path().join("source_skills");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(source_dir.join("SKILL.md"), "hello skill").unwrap();

    // Build a real tarball the same way `agentflow backup` does
    // (`run_tar_dir`'s `tar -C <parent> -czf <artifact> <basename>`),
    // so this exercises the actual extraction path, not just the
    // integrity check in isolation.
    let bundle_dir = work.path().join("bundle");
    std::fs::create_dir_all(&bundle_dir).unwrap();
    let artifact = bundle_dir.join(BackupInclude::SkillsDir.artifact_name());
    let status = tokio::process::Command::new("tar")
      .arg("-C")
      .arg(work.path())
      .arg("-czf")
      .arg(&artifact)
      .arg("source_skills")
      .status()
      .await
      .unwrap();
    assert!(status.success(), "test setup: tar failed to build fixture");

    let manifest = BundleManifest {
      manifest_version: BUNDLE_MANIFEST_VERSION.to_string(),
      created_at: Utc::now(),
      artifacts: vec![BundleManifestArtifact {
        include: BackupInclude::SkillsDir.tag().to_string(),
        source: source_dir.display().to_string(),
        artifact: BackupInclude::SkillsDir.artifact_name().to_string(),
        bytes: std::fs::metadata(&artifact).unwrap().len(),
        sha256: Some(sha256_file(&artifact).unwrap()),
      }],
    };
    std::fs::write(
      bundle_dir.join("manifest.json"),
      serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let target_dir = work.path().join("restored_skills");
    let saved = std::env::var("AGENTFLOW_SKILLS_DIR").ok();
    // SAFETY: isolated to this test's scope, restored before return —
    // same pattern as `restore_dir_fails_closed_before_deleting_...`
    // above. Uses `AGENTFLOW_SKILLS_DIR`, the only other test in this
    // file touching that variable is the root-path guard test, which
    // never reaches a real filesystem write, so there's no
    // read/write race between the two.
    unsafe { std::env::set_var("AGENTFLOW_SKILLS_DIR", &target_dir) };

    let mut args = base_args(bundle_dir.clone());
    args.dry_run = false;
    args.includes = vec![BackupInclude::SkillsDir];
    let report = run_restore(&args).await.unwrap();

    match saved {
      Some(value) => unsafe { std::env::set_var("AGENTFLOW_SKILLS_DIR", value) },
      None => unsafe { std::env::remove_var("AGENTFLOW_SKILLS_DIR") },
    }

    assert_eq!(report.steps.len(), 1);
    assert_eq!(
      report.steps[0].status, "executed",
      "an untampered artifact must restore normally; got reason: {:?}",
      report.steps[0].reason
    );
    assert!(
      report.steps[0].reason.is_none(),
      "an untampered artifact must not surface an integrity warning, got: {:?}",
      report.steps[0].reason
    );
    assert_eq!(
      std::fs::read_to_string(target_dir.join("SKILL.md")).unwrap(),
      "hello skill"
    );
  }
}
