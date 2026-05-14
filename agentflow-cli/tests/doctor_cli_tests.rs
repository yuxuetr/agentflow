//! End-to-end CLI tests covering the `agentflow doctor` expansions
//! shipped in P3.4: tri-state status + exit codes, `--profile`
//! thresholds, disk reachability section, and the optional
//! `--server <url>` reachability probe.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn doctor_default_profile_warns_on_missing_models_config() {
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--format", "json"])
    .env("HOME", home.path())
    .env_remove("AGENTFLOW_API_TOKEN");
  let output = cmd.output().unwrap();
  // Exit code 1 = warning (the default `local` profile escalates
  // missing config to Warning, never Fail).
  assert_eq!(output.status.code(), Some(1));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["status"], "warning");
  assert_eq!(report["profile"], "local");
  assert_eq!(report["disk"]["run_dir"]["exists"], false);
  assert_eq!(report["disk"]["run_dir"]["source"], "default");
}

#[test]
fn doctor_production_profile_escalates_missing_api_keys_to_fail() {
  let home = TempDir::new().unwrap();
  // Write a minimal models config that references an env var we will
  // intentionally leave unset, so the missing-env-var warning is
  // unambiguous and unrelated to the missing models file.
  let agentflow_dir = home.path().join(".agentflow");
  std::fs::create_dir_all(&agentflow_dir).unwrap();
  std::fs::write(
    agentflow_dir.join("models.yml"),
    r#"
providers:
  test:
    api_key_env: AGENTFLOW_DOCTOR_TEST_KEY_NEVER_SET
models:
  smoke:
    vendor: test
    type: text
    model_id: smoke
"#,
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--format", "json", "--profile", "production"])
    .env("HOME", home.path())
    .env_remove("AGENTFLOW_DOCTOR_TEST_KEY_NEVER_SET");
  let output = cmd.output().unwrap();
  assert_eq!(output.status.code(), Some(2));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["status"], "fail");
  assert_eq!(report["profile"], "production");
  let missing = report["config"]["missing_env_vars"].as_array().unwrap();
  assert!(
    missing
      .iter()
      .any(|v| v.as_str().unwrap() == "AGENTFLOW_DOCTOR_TEST_KEY_NEVER_SET")
  );
}

#[test]
fn doctor_dev_profile_passes_with_missing_config() {
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev", "--format", "json"])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  // dev profile is lenient: as long as nothing is structurally broken,
  // we expect Ok exit. The missing models config alone is not enough
  // to even warn under dev.
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["profile"], "dev");
  // dev still warns on sandbox + security profile basics, but the
  // exit code is at most 1 (Warning) since we do not Fail here.
  assert!(
    output.status.code() == Some(0) || output.status.code() == Some(1),
    "expected exit 0 or 1, got {:?}",
    output.status.code()
  );
}

#[test]
fn doctor_disk_section_reports_writable_run_dir_via_env_override() {
  let home = TempDir::new().unwrap();
  let run_dir = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev", "--format", "json"])
    .env("HOME", home.path())
    .env("AGENTFLOW_RUN_DIR", run_dir.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let run_check = &report["disk"]["run_dir"];
  assert_eq!(run_check["source"], "env");
  assert_eq!(run_check["exists"], true);
  assert_eq!(run_check["writable"], true);
  assert_eq!(run_check["path"], run_dir.path().display().to_string());
}

#[test]
fn doctor_server_probe_fails_against_unreachable_url() {
  let home = TempDir::new().unwrap();
  // Pick a port that almost certainly has nothing listening so the
  // probe times out / refuses. The 3s timeout caps the test runtime.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "dev",
      "--format",
      "json",
      "--server",
      "http://127.0.0.1:1/",
    ])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  assert_eq!(output.status.code(), Some(2));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["status"], "fail");
  let server = &report["server"];
  assert_eq!(server["reachable"], false);
  assert!(
    !server["error"].as_str().unwrap_or("").is_empty(),
    "expected non-empty error field"
  );
}

#[test]
fn doctor_text_output_includes_disk_and_profile_sections() {
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev"])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(stdout.contains("Disk:"), "missing Disk section: {stdout}");
  assert!(stdout.contains("run dir:"));
  assert!(stdout.contains("marketplace cache:"));
  assert!(stdout.contains("Profile: dev"));
}

#[test]
fn doctor_rejects_unknown_profile() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["doctor", "--profile", "bogus"]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("bogus"));
}
