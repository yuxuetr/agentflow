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
fn plugin_install_json_envelope_emits_structured_install_record() {
  let src = TempDir::new().unwrap();
  write_plugin_fixture(src.path(), "stub-plugin");
  let dest_root = TempDir::new().unwrap();

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      src.path().join("stub-plugin").to_str().unwrap(),
      "--dir",
      dest_root.path().to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .env("AGENTFLOW_SECURITY_PROFILE", "dev")
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "install must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin install");
  let result = &envelope["result"];
  assert_eq!(result["name"], "stub-plugin");
  assert_eq!(result["version"], "0.1.0");
  // Destination must be an absolute path under the dest_root.
  let dest = result["destination"].as_str().unwrap();
  assert!(
    dest.contains("stub-plugin"),
    "destination must end at the plugin dir: {dest}"
  );
  assert_eq!(result["policy"]["profile"], "dev");
  assert_eq!(result["policy"]["allowed"], true);
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_uninstall_json_envelope_reports_removal() {
  let src = TempDir::new().unwrap();
  // Stage a fixture, install it, then uninstall via the CLI to
  // verify the envelope contract for the happy path.
  write_plugin_fixture(src.path(), "to-remove");
  let dest_root = TempDir::new().unwrap();
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      src.path().join("to-remove").to_str().unwrap(),
      "--dir",
      dest_root.path().to_str().unwrap(),
    ])
    .env("AGENTFLOW_SECURITY_PROFILE", "dev")
    .assert()
    .success();

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "to-remove",
      "--dir",
      dest_root.path().to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "uninstall must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin uninstall");
  assert_eq!(envelope["result"]["name"], "to-remove");
  assert_eq!(envelope["result"]["removed"], true);
  assert_eq!(envelope["result"]["reason"], "removed");
  // Directory really is gone.
  assert!(!dest_root.path().join("to-remove").exists());
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_uninstall_force_on_missing_returns_not_installed_reason() {
  let dest_root = TempDir::new().unwrap();
  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "ghost-plugin",
      "--dir",
      dest_root.path().to_str().unwrap(),
      "--force",
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "--force must not error on missing");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin uninstall");
  assert_eq!(envelope["result"]["removed"], false);
  assert_eq!(envelope["result"]["reason"], "not_installed_force_acked");
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_generate_workflow_stub_json_envelope_inlines_stub_when_no_output_set() {
  let src = TempDir::new().unwrap();
  write_plugin_fixture(src.path(), "stub-source");
  let plugin_dir = src.path().join("stub-source");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "generate-workflow-stub",
      plugin_dir.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "stub generation must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "plugin generate-workflow-stub");
  let result = &envelope["result"];
  assert_eq!(result["plugin"], "stub-source");
  // No --output supplied ⇒ stub gets inlined as a string.
  let stub = result["stub"].as_str().unwrap();
  assert!(stub.contains("type: plugin"), "stub must contain plugin node yaml: {stub}");
  assert!(result["output_path"].is_null());
  let selected = result["selected_node_types"].as_array().unwrap();
  assert_eq!(selected.len(), 1);
  assert_eq!(selected[0], "echo_node");
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_generate_workflow_stub_json_envelope_omits_stub_when_output_set() {
  let src = TempDir::new().unwrap();
  write_plugin_fixture(src.path(), "stub-with-output");
  let plugin_dir = src.path().join("stub-with-output");
  let out_dir = TempDir::new().unwrap();
  let out_path = out_dir.path().join("stub.yml");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "generate-workflow-stub",
      plugin_dir.to_str().unwrap(),
      "--output",
      out_path.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success());
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  let result = &envelope["result"];
  // Stub went to disk; envelope reports the path, not the content.
  assert!(result["stub"].is_null());
  let reported = result["output_path"].as_str().unwrap();
  assert_eq!(reported, out_path.to_str().unwrap());
  // File actually carries the raw YAML.
  let on_disk = std::fs::read_to_string(&out_path).unwrap();
  assert!(on_disk.contains("type: plugin"));
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_install_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "install", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_uninstall_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "uninstall", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[cfg(feature = "plugin")]
#[test]
fn plugin_generate_workflow_stub_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "generate-workflow-stub", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
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

// ────────────────────────────────────────────────────────────────────────────
// trace replay
// ────────────────────────────────────────────────────────────────────────────

/// Write a minimal valid `ExecutionTrace` JSON to the format
/// `FileTraceStorage` expects (one file per trace,
/// `{base_path}/{workflow_id}.json`). Returns the trace directory
/// so the test can pass it via `--dir`.
fn write_trace_fixture(workflow_id: &str) -> TempDir {
  let tmp = TempDir::new().unwrap();
  let trace_json = serde_json::json!({
    "workflow_id": workflow_id,
    "context": {
      "run_id": workflow_id,
      "trace_id": workflow_id,
      "span_id": "workflow"
    },
    "workflow_name": "envelope-smoke",
    "started_at": "2026-05-19T00:00:00Z",
    "completed_at": "2026-05-19T00:00:01Z",
    "status": { "type": "completed" },
    "nodes": [],
    "metadata": {
      "tags": ["smoke", "test"],
      "environment": "development"
    }
  });
  std::fs::write(
    tmp.path().join(format!("{workflow_id}.json")),
    serde_json::to_string_pretty(&trace_json).unwrap(),
  )
  .unwrap();
  tmp
}

#[test]
fn trace_replay_json_envelope_emits_full_trace_as_result() {
  let dir = write_trace_fixture("envelope-test-trace");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "trace",
      "replay",
      "envelope-test-trace",
      "--dir",
      dir.path().to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "trace replay --format json-envelope must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "trace replay");

  // The result body IS the (redacted) ExecutionTrace — operators get
  // every field the storage layer persisted.
  let result = &envelope["result"];
  assert_eq!(result["workflow_id"], "envelope-test-trace");
  assert_eq!(result["workflow_name"], "envelope-smoke");
  assert_eq!(result["status"]["type"], "completed");
  assert!(result["nodes"].is_array());
  // Metadata round-trips through the envelope verbatim.
  assert_eq!(
    result["metadata"]["environment"]
      .as_str()
      .unwrap_or_default(),
    "development"
  );
  let tags = result["metadata"]["tags"].as_array().unwrap();
  assert_eq!(tags.len(), 2);
}

#[test]
fn trace_replay_json_envelope_ignores_json_flag_silently() {
  // `--json` is the legacy "append raw JSON after text replay" flag.
  // In envelope mode it's redundant (envelope already carries the
  // trace) so we ignore it without erroring — orthogonal flags
  // shouldn't need to compose.
  let dir = write_trace_fixture("envelope-with-json-flag");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "trace",
      "replay",
      "envelope-with-json-flag",
      "--dir",
      dir.path().to_str().unwrap(),
      "--format",
      "json-envelope",
      "--json", // legacy flag — ignored in envelope mode
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "envelope mode must succeed even with legacy --json flag");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "trace replay");
}

#[test]
fn trace_replay_default_text_format_unchanged() {
  // Regression guard: the legacy text replay path must keep working
  // without specifying `--format`. Default = text = current behavior.
  let dir = write_trace_fixture("envelope-text-default");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "trace",
      "replay",
      "envelope-text-default",
      "--dir",
      dir.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("envelope-text-default"));
}

#[test]
fn trace_replay_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["trace", "replay", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn trace_replay_rejects_unknown_format() {
  let dir = write_trace_fixture("envelope-rejects-format");
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "trace",
      "replay",
      "envelope-rejects-format",
      "--dir",
      dir.path().to_str().unwrap(),
      "--format",
      "yaml",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

// ────────────────────────────────────────────────────────────────────────────
// harness list / inspect / resume / run
// ────────────────────────────────────────────────────────────────────────────

/// Stage a synthetic `harness/sessions/<id>.jsonl` log under `run_dir`.
/// `events_json` is the JSONL body (one event per line).
fn write_harness_session_log(run_dir: &std::path::Path, session_id: &str, events_jsonl: &str) {
  let session_dir = run_dir.join("harness").join("sessions");
  std::fs::create_dir_all(&session_dir).unwrap();
  std::fs::write(session_dir.join(format!("{session_id}.jsonl")), events_jsonl).unwrap();
}

/// Minimal valid `HarnessEvent` JSONL body — one `stopped` event so
/// `harness inspect`'s summariser has at least one entry to bucket.
fn minimal_stopped_event_jsonl(session_id: &str) -> String {
  // Match the serde `tag = "kind", content = "payload",
  // rename_all = "snake_case"` discriminator on `HarnessEventBody`.
  let event = serde_json::json!({
    "seq": 0,
    "session_id": session_id,
    "ts": "2026-05-19T00:00:00Z",
    "kind": "stopped",
    "payload": {
      "reason": "completed",
      "final_answer": "smoke-ok",
      "error": null
    }
  });
  format!("{}\n", serde_json::to_string(&event).unwrap())
}

#[test]
fn harness_list_json_envelope_emits_structured_sessions_array() {
  let tmp = TempDir::new().unwrap();
  write_harness_session_log(tmp.path(), "session-a", "{\"line\":1}\n{\"line\":2}\n");
  write_harness_session_log(tmp.path(), "session-b", "{\"line\":1}\n");

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "harness",
      "list",
      "--run-dir",
      tmp.path().to_str().unwrap(),
      "--output",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "harness list must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "harness list");
  let sessions = envelope["result"]["sessions"].as_array().unwrap();
  assert_eq!(sessions.len(), 2);
  // Both ids must surface; counts must reflect non-empty line counts.
  let ids: Vec<&str> = sessions
    .iter()
    .map(|s| s["session_id"].as_str().unwrap())
    .collect();
  assert!(ids.contains(&"session-a"));
  assert!(ids.contains(&"session-b"));
  let a_count = sessions
    .iter()
    .find(|s| s["session_id"] == "session-a")
    .unwrap()["event_count"]
    .as_u64()
    .unwrap();
  assert_eq!(a_count, 2);
}

#[test]
fn harness_list_json_envelope_handles_empty_session_dir() {
  // No sessions persisted yet — envelope must still surface the
  // expected shape with an empty array.
  let tmp = TempDir::new().unwrap();
  // Don't stage any session files — the harness/sessions dir is
  // created on first persist, not on first list.
  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "harness",
      "list",
      "--run-dir",
      tmp.path().to_str().unwrap(),
      "--output",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success());
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "harness list");
  assert_eq!(envelope["result"]["sessions"].as_array().unwrap().len(), 0);
}

#[test]
fn harness_inspect_json_envelope_carries_summary_with_metadata() {
  let tmp = TempDir::new().unwrap();
  write_harness_session_log(
    tmp.path(),
    "session-stopped",
    &minimal_stopped_event_jsonl("session-stopped"),
  );

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "harness",
      "inspect",
      "session-stopped",
      "--run-dir",
      tmp.path().to_str().unwrap(),
      "--output",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "harness inspect must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "harness inspect");
  let result = &envelope["result"];
  assert_eq!(result["session_id"], "session-stopped");
  assert_eq!(result["event_count"], 1);
  let counts = result["counts_by_kind"].as_object().unwrap();
  assert_eq!(counts.get("stopped").and_then(|v| v.as_u64()), Some(1));
  assert_eq!(result["final_answer"], "smoke-ok");
}

#[test]
fn harness_resume_json_envelope_returns_events_in_array() {
  let tmp = TempDir::new().unwrap();
  let jsonl = minimal_stopped_event_jsonl("session-resume");
  write_harness_session_log(tmp.path(), "session-resume", &jsonl);

  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "harness",
      "resume",
      "session-resume",
      "--run-dir",
      tmp.path().to_str().unwrap(),
      "--output",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(env_out.status.success(), "harness resume must succeed");
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "harness resume");
  let result = &envelope["result"];
  assert_eq!(result["session_id"], "session-resume");
  assert_eq!(result["event_count"], 1);
  let events = result["events"].as_array().unwrap();
  assert_eq!(events.len(), 1);
  assert_eq!(events[0]["kind"], "stopped");
}

#[test]
fn harness_list_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["harness", "list", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn harness_inspect_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["harness", "inspect", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn harness_resume_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["harness", "resume", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn harness_run_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["harness", "run", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn harness_list_rejects_unknown_output_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["harness", "list", "--output", "yaml"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

// ────────────────────────────────────────────────────────────────────────────
// workflow list / cancel / graph (server-backed)
//
// Real envelope-shape round-trips need a Postgres-backed server (covered by
// `agentflow-cli/tests/cli_server_mode.rs` gated on
// `AGENTFLOW_DATABASE_TEST_URL`). Here we exercise the clap wiring + help
// surface so the value-parser strings stay locked.
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn workflow_list_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "list", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn workflow_cancel_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "cancel", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn workflow_graph_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "graph", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[test]
fn workflow_list_rejects_unknown_format() {
  // value-parser fires before the "requires --server" bail, so this
  // tests the format wiring cleanly without needing a live server.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "list", "--format", "yaml"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

#[test]
fn workflow_cancel_rejects_unknown_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "cancel", "run-abc", "--format", "text"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("text"));
}

#[test]
fn workflow_graph_rejects_unknown_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["workflow", "graph", "run-abc", "--format", "xml"])
    .assert()
    .failure()
    .stderr(predicate::str::contains("xml"));
}

// ────────────────────────────────────────────────────────────────────────────
// rag search / eval (gated on `rag` feature)
// ────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "rag")]
fn rag_ci_offline_fixture_path() -> String {
  // Mirror `rag_eval_cli_tests.rs::fixture_path`: the ci_offline
  // dataset lives next door in `agentflow-rag/`, two levels up from
  // the per-test target dir.
  format!(
    "{}/../agentflow-rag/eval_datasets/ci_offline",
    env!("CARGO_MANIFEST_DIR")
  )
}

#[cfg(feature = "rag")]
#[test]
fn rag_eval_json_envelope_emits_canonical_envelope_against_ci_offline() {
  // Drives the existing ci_offline fixture through `--format
  // json-envelope`. Asserts the envelope shape + that the result
  // carries the same top-level keys the legacy `--output` file
  // emits (`dataset`, `baseline`, etc.).
  let env_out = Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "rag",
      "eval",
      "--dataset",
      &rag_ci_offline_fixture_path(),
      "--format",
      "json-envelope",
    ])
    .output()
    .unwrap();
  assert!(
    env_out.status.success(),
    "rag eval --format json-envelope must succeed; stderr: {}",
    String::from_utf8_lossy(&env_out.stderr)
  );
  let envelope: Value = serde_json::from_slice(&env_out.stdout).unwrap();
  assert_envelope_shape(&envelope, "rag eval");
  let result = &envelope["result"];
  // Top-level keys the legacy --output file mode emits; the envelope
  // wraps the same body so existing baseline-comparison tools can
  // migrate by reading `envelope.result.<field>`.
  for key in ["dataset", "baseline", "candidate", "comparison", "regression"] {
    assert!(
      result.get(key).is_some(),
      "result must contain '{key}': {result}"
    );
  }
  // No --compare-* flag ⇒ no regression decision ⇒ regression is null.
  assert!(result["regression"].is_null());
  // No regression ⇒ errors[] empty.
  assert!(envelope["errors"].as_array().unwrap().is_empty());
}

#[cfg(feature = "rag")]
#[test]
fn rag_eval_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["rag", "eval", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("json-envelope"));
}

#[cfg(feature = "rag")]
#[test]
fn rag_eval_rejects_unknown_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "rag",
      "eval",
      "--dataset",
      &rag_ci_offline_fixture_path(),
      "--format",
      "yaml",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("yaml"));
}

#[cfg(feature = "rag")]
#[test]
fn rag_search_help_lists_json_envelope_format() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["rag", "search", "--help"])
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
