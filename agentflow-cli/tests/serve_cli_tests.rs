//! End-to-end CLI tests for `agentflow serve`.
//!
//! The serve command shells out to the `agentflow-server` binary, so
//! these tests exercise the full subprocess pipeline by relying on the
//! workspace test build that produces both binaries.
//!
//! `--check` is exercised here because it does not require a live
//! Postgres. The actual binding path is excluded: it would need a real
//! database and a free port; that is covered by the server's own
//! `runs_routes` integration suite when `AGENTFLOW_DATABASE_TEST_URL`
//! is configured.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

/// Returns `Some(path)` if the `agentflow-server` binary is co-located
/// with the test binary (the agentflow-cli target dir), `None` otherwise.
///
/// `cargo test -p agentflow-cli` does not pull in agentflow-server, so
/// when developers run that command directly we'd otherwise spawn the
/// CLI's `serve --check` path, which tries to locate the server binary,
/// fails, and panics on empty-JSON stdout. Skipping here yields an
/// informative reason instead of a confusing panic. CI sidesteps this by
/// pre-building agentflow-server in the matrix step.
fn server_binary_present() -> Option<PathBuf> {
  let cli_bin = assert_cmd::cargo::cargo_bin("agentflow");
  let exe_name = if cfg!(windows) {
    "agentflow-server.exe"
  } else {
    "agentflow-server"
  };
  let candidate = cli_bin.parent()?.join(exe_name);
  candidate.is_file().then_some(candidate)
}

macro_rules! skip_if_server_missing {
  () => {
    if server_binary_present().is_none() {
      eprintln!(
        "skipped: agentflow-server binary not built — run `cargo build -p agentflow-server` first"
      );
      return;
    }
  };
}

#[test]
fn serve_check_outputs_a_structured_readiness_report() {
  skip_if_server_missing!();
  // Use a dedicated token env var with a known-empty value so the
  // report's auth section is deterministic.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "serve",
    "--check",
    "--bind",
    "127.0.0.1:0",
    "--security-profile",
    "local",
    "--auth-token-env",
    "AGENTFLOW_SERVE_TEST_TOKEN_MISSING",
  ]);
  cmd.env_remove("AGENTFLOW_API_TOKEN");
  cmd.env_remove("AGENTFLOW_SERVE_TEST_TOKEN_MISSING");
  cmd.env_remove("DATABASE_URL");
  // Without DB the local profile should warn (exit 1). The structured
  // report must include the auth, database, sandbox, and readiness
  // fields.
  let output = cmd.output().unwrap();
  let stdout = String::from_utf8(output.stdout).unwrap();
  let parsed: serde_json::Value = serde_json::from_str(&stdout)
    .unwrap_or_else(|err| panic!("--check stdout not JSON: {err}\nstdout=`{stdout}`"));
  assert_eq!(parsed["bind"], "127.0.0.1:0");
  assert_eq!(parsed["security_profile"], "local");
  assert_eq!(
    parsed["auth"]["token_env"],
    "AGENTFLOW_SERVE_TEST_TOKEN_MISSING"
  );
  assert_eq!(parsed["auth"]["token_present"], false);
  assert_eq!(parsed["database"]["url_present"], false);
  assert!(parsed["sandbox"]["backend"].is_string());
  assert!(parsed["readiness"].is_string());
  assert!(
    parsed["warnings"].is_array()
      && parsed["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("DATABASE_URL is not set"))
  );
  // Local profile without DB and without token: warn → exit 1.
  assert_eq!(output.status.code(), Some(1));
}

#[test]
fn serve_check_production_without_token_fails_with_exit_code_2() {
  skip_if_server_missing!();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "serve",
    "--check",
    "--security-profile",
    "production",
    "--auth-token-env",
    "AGENTFLOW_SERVE_TEST_TOKEN_PROD",
  ]);
  cmd.env_remove("AGENTFLOW_SERVE_TEST_TOKEN_PROD");
  cmd.env_remove("DATABASE_URL");
  let output = cmd.output().unwrap();
  let stdout = String::from_utf8(output.stdout).unwrap();
  let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
  assert_eq!(parsed["security_profile"], "production");
  assert_eq!(parsed["readiness"], "fail");
  let errors = parsed["errors"].as_array().unwrap();
  assert!(
    errors
      .iter()
      .any(|e| e.as_str().unwrap().contains("requires bearer auth"))
  );
  assert_eq!(output.status.code(), Some(2));
}

#[test]
fn serve_check_with_token_and_local_profile_warns_only_about_db() {
  skip_if_server_missing!();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "serve",
    "--check",
    "--security-profile",
    "local",
    "--auth-token-env",
    "AGENTFLOW_SERVE_TEST_TOKEN_OK",
  ]);
  cmd.env("AGENTFLOW_SERVE_TEST_TOKEN_OK", "topsecret");
  cmd.env_remove("DATABASE_URL");
  let output = cmd.output().unwrap();
  let stdout = String::from_utf8(output.stdout).unwrap();
  let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
  assert_eq!(parsed["auth"]["token_present"], true);
  assert_eq!(parsed["readiness"], "warn");
  // The token value itself must never appear in the report.
  assert!(!stdout.contains("topsecret"), "auth token leaked: {stdout}");
}

#[test]
fn serve_help_lists_the_documented_flags() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["serve", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("--bind"))
    .stdout(predicate::str::contains("--security-profile"))
    .stdout(predicate::str::contains("--check"))
    .stdout(predicate::str::contains("--cors-origins"))
    .stdout(predicate::str::contains("--max-body-mb"));
}
