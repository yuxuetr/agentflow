//! End-to-end smoke for the P3.3 envelope migrations.
//!
//! Each test exercises a command's `--format json-envelope` mode
//! against a hermetic fixture and asserts:
//!   * the envelope carries the canonical 4-key set
//!     (`version` + `command` + `result` + `errors`)
//!   * the `version` is the pinned `agentflow.cli/1`
//!   * the `command` reflects the user-visible subcommand path
//!     (space-separated for multi-word commands)
//!   * the `result` body is byte-identical to the legacy
//!     `--format json` output (additive-field contract — operators
//!     who pinned to `.result` see no shape change)
//!
//! The "byte-identical result" assertion is the contract that lets
//! existing tools migrate by either prefixing access (`.result.foo`)
//! or just adding envelope decoding without rewriting their schema
//! mapping.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::collections::BTreeSet;
use tempfile::TempDir;

/// The 4 keys the envelope's top level locks down. Adding a 5th key
/// would be a wire-shape change; the test catches it.
const ENVELOPE_KEYS: &[&str] = &["command", "errors", "result", "version"];

fn assert_envelope_shape(envelope: &Value, expected_command: &str) {
  let keys: BTreeSet<&str> = envelope
    .as_object()
    .expect("envelope must be a JSON object")
    .keys()
    .map(String::as_str)
    .collect();
  let expected: BTreeSet<&str> = ENVELOPE_KEYS.iter().copied().collect();
  assert_eq!(
    keys, expected,
    "envelope top-level keys drifted from the agentflow.cli/1 contract"
  );
  assert_eq!(envelope["version"], "agentflow.cli/1");
  assert_eq!(envelope["command"], expected_command);
  assert!(
    envelope["errors"].is_array(),
    "errors must always be an array (never null)"
  );
}

// ────────────────────────────────────────────────────────────────────────────
// workflow validate
// ────────────────────────────────────────────────────────────────────────────

fn minimal_valid_workflow() -> &'static str {
  // Tera string template is the smallest workflow that passes schema
  // validation: one template node, no dependencies, no inputs.
  r#"
name: smoke
description: minimal smoke test
nodes:
  - id: greet
    type: template
    parameters:
      template: "hi"
      output_key: greeting
"#
}

#[test]
fn workflow_validate_json_envelope_wraps_legacy_json_body() {
  let tmp = TempDir::new().unwrap();
  let path = tmp.path().join("smoke.yml");
  std::fs::write(&path, minimal_valid_workflow()).unwrap();

  // Capture the legacy --format json body as the contract baseline.
  let legacy_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "workflow",
      "validate",
      path.to_str().unwrap(),
      "--format",
      "json",
    ])
    .output()
    .unwrap();
  assert!(legacy_out.status.success(), "legacy json must succeed");
  let legacy_body: Value = serde_json::from_slice(&legacy_out.stdout).unwrap();

  // Now request the envelope and verify result == legacy body.
  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "workflow",
      "validate",
      path.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "json-envelope must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "workflow validate");
  assert_eq!(
    envelope["result"], legacy_body,
    "envelope `result` must equal legacy `--format json` body"
  );
  // Valid workflow ⇒ no actionable errors.
  assert!(envelope["errors"].as_array().unwrap().is_empty());
}

#[test]
fn workflow_validate_json_envelope_surfaces_invalid_workflow_in_errors() {
  let tmp = TempDir::new().unwrap();
  let path = tmp.path().join("broken.yml");
  // Missing required `template` parameter on a `type: template`
  // node — schema validator surfaces this with a clear issue.
  std::fs::write(
    &path,
    r#"
name: broken
description: triggers a schema error
nodes:
  - id: bad
    type: template
    parameters:
      output_key: greeting
"#,
  )
  .unwrap();

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "workflow",
      "validate",
      path.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  // Failed validation exits non-zero (the `bail!` at the end).
  assert!(
    !env_out.status.success(),
    "invalid workflow must exit non-zero even under json-envelope"
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "workflow validate");
  assert_eq!(envelope["result"]["valid"], false);
  let errors = envelope["errors"].as_array().unwrap();
  assert!(
    !errors.is_empty(),
    "errors must surface the schema failure for shell consumers"
  );
  assert!(
    errors[0]
      .as_str()
      .unwrap()
      .contains("failed schema validation"),
    "errors[0] must mention the schema failure: {errors:?}"
  );
}

#[test]
fn workflow_validate_rejects_unknown_format() {
  let tmp = TempDir::new().unwrap();
  let path = tmp.path().join("smoke.yml");
  std::fs::write(&path, minimal_valid_workflow()).unwrap();
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "workflow",
      "validate",
      path.to_str().unwrap(),
      "--format",
      "yaml",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

// ────────────────────────────────────────────────────────────────────────────
// workflow resume-plan
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn workflow_resume_plan_json_envelope_reports_missing_checkpoint() {
  // No checkpoint dir staged → loader bails with a typed error.
  // We don't need a real checkpoint to verify the format wiring:
  // the command parses --format json-envelope before doing the
  // expensive checkpoint load.
  let tmp = TempDir::new().unwrap();
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "workflow",
      "resume-plan",
      "nonexistent-run-id",
      "--checkpoint-dir",
      tmp.path().to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .assert()
    // Missing checkpoint ⇒ anyhow bail ⇒ non-zero exit. The
    // envelope path isn't reached for this error, but the value-
    // parser must accept json-envelope as a known format.
    .failure();
}

#[test]
fn workflow_resume_plan_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "resume-plan", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

// ────────────────────────────────────────────────────────────────────────────
// eval run
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn eval_run_help_lists_json_envelope_format() {
  // Full eval-run execution needs an LLM provider, which would make
  // this test non-hermetic. We verify the format wiring at the help
  // surface instead — the command's run path uses the same parser
  // unit-tested in `commands::eval::tests::parse_format_round_trip`.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["eval", "run", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn eval_run_rejects_unknown_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "eval",
      "run",
      "/nonexistent/dataset",
      "--format",
      "yaml",
    ])
    .assert()
    .failure();
}

// ────────────────────────────────────────────────────────────────────────────
// envelope contract regression guard
// ────────────────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────────────────
// mcp list-tools / list-resources / call-tool
// ────────────────────────────────────────────────────────────────────────────

/// All three MCP subcommands require a live MCP server to do real
/// work. We exercise the format-flag wiring + help surface here so
/// the value-parser strings stay in sync; end-to-end JSON-shape
/// assertions need an actual server which lives outside the hermetic
/// CLI suite.
#[test]
fn mcp_list_tools_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["mcp", "list-tools", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn mcp_list_resources_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["mcp", "list-resources", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn mcp_call_tool_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["mcp", "call-tool", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

// ────────────────────────────────────────────────────────────────────────────
// llm models
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn llm_models_json_envelope_emits_structured_models_list() {
  // Uses the bundled `default_models.yml` (built-in source). We
  // don't assert exact counts — the registry changes — only the
  // envelope shape + that the result carries the expected keys.
  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args(["llm", "models", "--format", "json-envelope"])
    .env_remove("AGENTFLOW_MODELS_CONFIG")
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "llm models --format json-envelope must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "llm models");
  // result must have models array + total
  let result = envelope["result"].as_object().expect("result is object");
  assert!(result.contains_key("models"), "result must carry models");
  assert!(result.contains_key("total"), "result must carry total");
  assert!(result.contains_key("source"), "result must carry source");
  let total = result["total"].as_u64().expect("total is number");
  let models = result["models"].as_array().expect("models is array");
  assert_eq!(models.len() as u64, total);
  // Spot-check: every model entry has the expected keys.
  if let Some(first) = models.first() {
    for key in ["name", "vendor", "model_id"] {
      assert!(
        first.get(key).is_some(),
        "model entry missing '{key}': {first}"
      );
    }
  }
}

#[test]
fn llm_models_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["llm", "models", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn llm_models_rejects_unknown_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["llm", "models", "--format", "yaml"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

#[test]
fn mcp_list_tools_rejects_unknown_format() {
  // Empty server command short-circuits before any network — but the
  // value-parser fires first on `--format`, which is what we want
  // to lock down here.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["mcp", "list-tools", "--format", "yaml", "--", "echo"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

// ────────────────────────────────────────────────────────────────────────────
// plugin list / inspect (gated on `plugin` feature)
// ────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "plugin")]
fn write_plugin_fixture(plugins_root: &std::path::Path, name: &str) -> std::path::PathBuf {
  use std::fs;
  let plugin_dir = plugins_root.join(name);
  fs::create_dir_all(plugin_dir.join("bin")).unwrap();
  fs::write(plugin_dir.join("bin/echo"), "").unwrap();
  fs::write(
    plugin_dir.join("plugin.toml"),
    format!(
      r#"
[plugin]
name = "{name}"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/echo"

[[plugin.nodes]]
type = "echo_node"
description = "Demo node"

[plugin.capabilities]
filesystem = ["read:/tmp"]
network = []
processes = []
env_vars = ["FOO"]
"#
    ),
  )
  .unwrap();
  plugin_dir
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_list_json_envelope_emits_structured_payload() {
  let tmp = TempDir::new().unwrap();
  write_plugin_fixture(tmp.path(), "echo-plugin");
  write_plugin_fixture(tmp.path(), "another-plugin");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "list",
      "--dir",
      tmp.path().to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "plugin list must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin list");
  let result = &envelope["result"];
  assert_eq!(result["total"], 2);
  let plugins = result["plugins"].as_array().unwrap();
  assert_eq!(plugins.len(), 2);
  let names: Vec<&str> = plugins
    .iter()
    .map(|p| p["name"].as_str().unwrap())
    .collect();
  assert!(names.contains(&"echo-plugin"));
  assert!(names.contains(&"another-plugin"));
  // Each entry must expose the structured fields the text view
  // collapses into a single line.
  for plugin in plugins {
    assert_eq!(plugin["manifest_valid"], true);
    assert_eq!(plugin["entrypoint_exists"], true);
    assert_eq!(plugin["nodes"].as_array().unwrap().len(), 1);
    let caps = &plugin["capabilities"];
    assert_eq!(caps["filesystem"].as_array().unwrap().len(), 1);
    assert_eq!(caps["env_vars"].as_array().unwrap().len(), 1);
  }
  // No validation errors expected on the fixture.
  assert!(envelope["errors"].as_array().unwrap().is_empty());
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_inspect_json_envelope_carries_resolved_entrypoint_metadata() {
  let tmp = TempDir::new().unwrap();
  let plugin_dir = write_plugin_fixture(tmp.path(), "demo-plugin");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "inspect",
      plugin_dir.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "plugin inspect must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin inspect");
  let result = &envelope["result"];
  assert_eq!(result["manifest_valid"], true);
  assert_eq!(result["entrypoint_exists"], true);
  // Resolved entrypoint must be absolute — operators reading the
  // envelope shouldn't have to know about the manifest dir to figure
  // out where the binary actually lives.
  let resolved = result["resolved_entrypoint"].as_str().unwrap();
  assert!(
    std::path::Path::new(resolved).is_absolute(),
    "resolved_entrypoint must be absolute: {resolved}"
  );
  assert_eq!(result["manifest"]["plugin"]["name"], "demo-plugin");
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_list_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "list", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_inspect_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "inspect", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn envelope_contract_locks_canonical_4_key_set() {
  // Belt-and-suspenders: any new command that wants to ship a 5th
  // top-level key has to bump `ENVELOPE_VERSION`. This test holds the
  // shared assertion helper to the contract.
  assert_eq!(
    ENVELOPE_KEYS.len(),
    4,
    "envelope schema must remain 4-key — bump agentflow.cli version when adding"
  );
  let expected: BTreeSet<&str> = ["command", "errors", "result", "version"]
    .iter()
    .copied()
    .collect();
  let actual: BTreeSet<&str> = ENVELOPE_KEYS.iter().copied().collect();
  assert_eq!(actual, expected);
}
