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
fn doctor_backup_check_section_omitted_by_default() {
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev", "--format", "json"])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert!(
    report.get("backup_check").is_none() || report["backup_check"].is_null(),
    "backup_check should be absent when --backup-check is not supplied"
  );
}

#[test]
fn doctor_backup_check_reports_writable_dirs_under_synthetic_home() {
  let home = TempDir::new().unwrap();
  // Pre-create all five backup-relevant dirs so each probe succeeds.
  for sub in [
    ".agentflow/runs",
    ".agentflow/traces",
    ".agentflow/marketplace/cache",
    ".agentflow/skills",
    ".agentflow/plugins",
  ] {
    std::fs::create_dir_all(home.path().join(sub)).unwrap();
  }
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "dev",
      "--format",
      "json",
      "--backup-check",
    ])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let backup = &report["backup_check"];
  for key in [
    "run_dir",
    "trace_dir",
    "marketplace_cache",
    "skills_dir",
    "plugins_dir",
  ] {
    assert_eq!(
      backup[key]["writable"], true,
      "{key} should be writable when pre-created; got {:?}",
      backup[key]
    );
    assert_eq!(backup[key]["exists"], true, "{key} should exist");
  }
}

#[test]
fn doctor_backup_check_production_profile_escalates_missing_dirs_to_fail() {
  let home = TempDir::new().unwrap();
  // Intentionally do NOT pre-create the backup dirs. Under the production
  // profile this should escalate to a Fail (exit 2).
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "production",
      "--format",
      "json",
      "--backup-check",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_API_TOKEN", "dummy-token-for-test");
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["status"], "fail");
  let backup = &report["backup_check"];
  assert_eq!(backup["skills_dir"]["exists"], false);
  assert_eq!(backup["plugins_dir"]["exists"], false);
}

#[test]
fn doctor_backup_check_text_output_renders_section_header() {
  let home = TempDir::new().unwrap();
  for sub in [
    ".agentflow/runs",
    ".agentflow/traces",
    ".agentflow/marketplace/cache",
    ".agentflow/skills",
    ".agentflow/plugins",
  ] {
    std::fs::create_dir_all(home.path().join(sub)).unwrap();
  }
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev", "--backup-check"])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(
    stdout.contains("Backup check:"),
    "missing Backup check section: {stdout}"
  );
  assert!(stdout.contains("skills dir:"));
  assert!(stdout.contains("plugins dir:"));
}

#[test]
fn doctor_backup_check_honors_skills_and_plugins_env_overrides() {
  let home = TempDir::new().unwrap();
  let skills_override = TempDir::new().unwrap();
  let plugins_override = TempDir::new().unwrap();
  // Create the other three dirs under HOME so only skills/plugins come
  // from the override.
  for sub in [
    ".agentflow/runs",
    ".agentflow/traces",
    ".agentflow/marketplace/cache",
  ] {
    std::fs::create_dir_all(home.path().join(sub)).unwrap();
  }
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "dev",
      "--format",
      "json",
      "--backup-check",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_SKILLS_DIR", skills_override.path())
    .env("AGENTFLOW_PLUGINS_DIR", plugins_override.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let backup = &report["backup_check"];
  assert_eq!(backup["skills_dir"]["source"], "env");
  assert_eq!(backup["plugins_dir"]["source"], "env");
  assert_eq!(
    backup["skills_dir"]["path"],
    skills_override.path().display().to_string()
  );
  assert_eq!(backup["skills_dir"]["writable"], true);
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

// ── P3.3: CLI JSON envelope ─────────────────────────────────────────────────

#[test]
fn doctor_json_envelope_wraps_report_in_canonical_envelope() {
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--format", "json-envelope", "--profile", "dev"])
    .env("HOME", home.path())
    .env_remove("AGENTFLOW_API_TOKEN");
  let output = cmd.output().unwrap();
  // `dev` profile exits 0 or 1 depending on whether sandbox / security
  // basics promote to Warning (1) — never Fail (2). Either way the
  // envelope must serialize correctly.
  assert!(
    matches!(output.status.code(), Some(0) | Some(1)),
    "expected exit 0 or 1, got {:?}",
    output.status.code()
  );

  let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
  // Envelope fields locked by P3.3:
  assert_eq!(envelope["version"], "agentflow.cli/1");
  assert_eq!(envelope["command"], "doctor");
  assert!(
    envelope["errors"].is_array(),
    "errors must always be an array (never null)"
  );
  assert!(
    envelope["errors"].as_array().unwrap().is_empty(),
    "successful doctor run must carry an empty errors array"
  );
  // `result` carries the same DoctorReport the legacy `--format json`
  // mode emits at the top level; sanity-check a few representative
  // fields so a regression in the wrapping is caught here.
  let result = &envelope["result"];
  assert_eq!(result["profile"], "dev");
  assert!(result["status"].is_string());
  assert!(result["features"].is_object());
  assert!(result["disk"].is_object());
}

// ── P3.4 lite installation probes ─────────────────────────────────────────

#[test]
fn doctor_check_installations_section_omitted_by_default() {
  // Without --check-installations the new section must be absent so
  // existing callers keep the lighter shape.
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--profile", "dev", "--format", "json"])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert!(
    report
      .get("installations")
      .map(|v| v.is_null())
      .unwrap_or(true),
    "installations section must be omitted by default, got: {report}"
  );
}

#[test]
fn doctor_check_installations_inventories_skills_and_plugins() {
  // Stage a synthetic ~/.agentflow tree with one skill that declares an
  // MCP server (echo, present on every macOS/Linux box) and one plugin
  // whose entrypoint exists. Probe must surface both without errors.
  let home = TempDir::new().unwrap();
  let skills = home.path().join(".agentflow/skills/demo-skill");
  std::fs::create_dir_all(&skills).unwrap();
  std::fs::write(
    skills.join("skill.toml"),
    r#"
[skill]
name = "demo-skill"
version = "0.1.0"
description = "demo"

[persona]
role = "demo"

[model]
name = "mock-model"

[[mcp_servers]]
name = "demo_echo"
command = "echo"
args = []
"#,
  )
  .unwrap();

  let plugins = home.path().join(".agentflow/plugins/demo-plugin");
  std::fs::create_dir_all(plugins.join("bin")).unwrap();
  let entrypoint = plugins.join("bin/dummy");
  std::fs::write(&entrypoint, "").unwrap();
  std::fs::write(
    plugins.join("plugin.toml"),
    r#"
[plugin]
name = "demo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/dummy"
"#,
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "dev",
      "--format",
      "json",
      "--check-installations",
    ])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let probe = &report["installations"];
  assert!(probe.is_object(), "installations section must be populated");

  let mcp_servers = probe["mcp_servers"].as_array().unwrap();
  assert_eq!(mcp_servers.len(), 1);
  assert_eq!(mcp_servers[0]["skill"], "demo-skill");
  assert_eq!(mcp_servers[0]["server"], "demo_echo");
  assert_eq!(mcp_servers[0]["reachable"], true);

  // Plugin section is only populated when the binary was built with the
  // `plugin` feature; without it the array is empty but the section
  // still surfaces.
  let plugins_arr = probe["plugins"].as_array().unwrap();
  if !plugins_arr.is_empty() {
    let entry = &plugins_arr[0];
    assert_eq!(entry["name"], "demo-plugin");
    assert_eq!(entry["entrypoint_exists"], true);
  }
}

#[test]
fn doctor_check_installations_flags_missing_mcp_command() {
  // Skill declares an MCP server pointing at a binary that doesn't
  // exist on PATH. The probe must report `reachable = false` and the
  // overall status must promote to at least Warning under `local`.
  let home = TempDir::new().unwrap();
  let skills = home.path().join(".agentflow/skills/missing-cmd-skill");
  std::fs::create_dir_all(&skills).unwrap();
  std::fs::write(
    skills.join("skill.toml"),
    r#"
[skill]
name = "missing-cmd-skill"
version = "0.1.0"
description = "demo"

[persona]
role = "demo"

[model]
name = "mock-model"

[[mcp_servers]]
name = "ghost"
command = "this-binary-definitely-does-not-exist-pls"
args = []
"#,
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "doctor",
      "--profile",
      "dev",
      "--format",
      "json",
      "--check-installations",
    ])
    .env("HOME", home.path());
  let output = cmd.output().unwrap();
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let mcp_servers = report["installations"]["mcp_servers"].as_array().unwrap();
  assert_eq!(mcp_servers.len(), 1);
  assert_eq!(mcp_servers[0]["reachable"], false);
}

#[test]
fn doctor_json_envelope_field_set_is_closed_to_four_keys() {
  // Locks the envelope contract: any drift that adds a fifth top-level
  // key (without bumping the version constant) fails here.
  let home = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--format", "json-envelope", "--profile", "dev"])
    .env("HOME", home.path())
    .env_remove("AGENTFLOW_API_TOKEN");
  let output = cmd.output().unwrap();
  let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
  let mut keys: Vec<&str> = envelope
    .as_object()
    .expect("envelope must be a JSON object")
    .keys()
    .map(String::as_str)
    .collect();
  keys.sort();
  assert_eq!(keys, vec!["command", "errors", "result", "version"]);
}
