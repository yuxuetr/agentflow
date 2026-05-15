//! `agentflow eval run` end-to-end CLI tests.
//!
//! Drives the bundled `agentflow-agents/eval_datasets/ci_offline/`
//! fixture against the mock LLM provider so the suite is hermetic — no
//! API key, no network, no DB. Slice 3 of P4.4.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Write a `~/.agentflow/models.yml` registering the two mock-model ids
/// the `ci_offline` fixture uses. Mirrors the helper in
/// `skill_cli_tests.rs` but with the eval-specific ids.
fn write_eval_mock_models_config(home: &Path) {
  let config_dir = home.join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join("models.yml"),
    r#"
models:
  mock-eval-hello:
    vendor: mock
    type: text
    model_id: mock-eval-hello
  mock-eval-budget:
    vendor: mock
    type: text
    model_id: mock-eval-budget
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#,
  )
  .unwrap();
}

/// Two canned ReAct responses sufficient for the two-case fixture: the
/// agent plans, then emits a final answer matching the case's `contains`
/// assertion. Mock provider serves responses round-robin.
fn mock_responses() -> String {
  serde_json::to_string(&vec![
    r#"{"thought":"answer directly","answer":"hello there"}"#,
    r#"{"thought":"answer directly","answer":"done"}"#,
  ])
  .unwrap()
}

fn fixture_path() -> String {
  format!(
    "{}/../agentflow-agents/eval_datasets/ci_offline",
    env!("CARGO_MANIFEST_DIR")
  )
}

#[test]
fn cli_eval_run_text_summary_emits_passed_count() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", &fixture_path()])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .assert()
    .success()
    .stdout(predicate::str::contains("Dataset: ci-offline"))
    .stdout(predicate::str::contains("2 total, 2 passed, 0 failed"));
}

#[test]
fn cli_eval_run_json_envelope_has_expected_top_level_keys() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path(), "--format", "json"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["schema_version"], 1);
  assert_eq!(report["dataset"], "ci-offline");
  assert_eq!(report["dataset_version"], "0.1.0");
  assert_eq!(report["summary"]["total"], 2);
  assert_eq!(report["summary"]["passed"], 2);
  assert_eq!(report["summary"]["failed"], 0);
  let cases = report["cases"].as_array().unwrap();
  assert_eq!(cases.len(), 2);
  // Each case carries a trace_id so operators can hand it to `agentflow
  // trace replay`.
  for case in cases {
    assert!(case["trace_id"].is_string());
    assert_eq!(case["status"], "passed");
  }
}

#[test]
fn cli_eval_run_filter_skips_non_matching_cases() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      &fixture_path(),
      "--format",
      "json",
      "--filter",
      "hello-*",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .output()
    .unwrap();
  assert!(output.status.success());
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["total"], 2);
  assert_eq!(report["summary"]["passed"], 1);
  assert_eq!(report["summary"]["skipped"], 1);
  let cases = report["cases"].as_array().unwrap();
  let by_id: std::collections::HashMap<&str, &Value> = cases
    .iter()
    .map(|c| (c["id"].as_str().unwrap(), c))
    .collect();
  assert_eq!(by_id["hello-world"]["status"], "passed");
  assert_eq!(by_id["step-budget"]["status"], "skipped");
}

#[test]
fn cli_eval_run_exits_nonzero_when_case_fails() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  // Force a failure by feeding mock responses whose `answer` doesn't
  // contain the needle the case expects.
  let bad_responses = serde_json::to_string(&vec![
    r#"{"thought":"oops","answer":"totally unrelated"}"#,
    r#"{"thought":"oops","answer":"also unrelated"}"#,
  ])
  .unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path()])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", bad_responses)
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("failed"), "stdout: {stdout}");
}

#[test]
fn cli_eval_run_fail_on_status_never_returns_zero_even_on_failure() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let bad_responses = serde_json::to_string(&vec![
    r#"{"thought":"oops","answer":"totally unrelated"}"#,
    r#"{"thought":"oops","answer":"also unrelated"}"#,
  ])
  .unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", &fixture_path(), "--fail-on-status", "never"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", bad_responses)
    .assert()
    .success();
}

#[test]
fn cli_eval_run_help_lists_format_filter_fail_on_status_flags() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("--format"))
    .stdout(predicate::str::contains("--filter"))
    .stdout(predicate::str::contains("--fail-on-status"));
}

// ── Skill-aware factory + tools admission ───────────────────────────────

/// Write a minimal skill.toml under `dir` that registers `shell` + `file`
/// tools. Used to exercise `case.skill` loading and the
/// `tools_allowed` / `tools_denied` admission filter. Intentionally
/// skips the `script` tool to avoid the validator's `scripts/` dir
/// requirement.
fn write_three_tool_skill(dir: &Path, model: &str) {
  fs::write(
    dir.join("skill.toml"),
    format!(
      r#"
[skill]
name = "eval-three-tools"
version = "0.1.0"
description = "Two-tool skill fixture for eval CLI tests"

[persona]
role = "Answer the user concisely."

[model]
name = "{model}"
max_iterations = 4

[[tools]]
name = "shell"

[[tools]]
name = "file"
"#
    ),
  )
  .unwrap();
}

fn write_dataset_targeting_skill(dir: &Path, skill_dir: &Path, dataset_body: &str) {
  fs::write(
    dir.join("dataset.toml"),
    format!(
      r#"
schema_version = 1
name = "skill-fixture"
version = "0.0.1"

[defaults]
skill = "{skill}"
max_steps = 4
"#,
      skill = skill_dir.display(),
    ),
  )
  .unwrap();
  fs::write(dir.join("cases.jsonl"), dataset_body).unwrap();
}

#[test]
fn cli_eval_run_loads_case_skill_when_present() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let skill_dir = work.path().join("eval-skill");
  fs::create_dir_all(&skill_dir).unwrap();
  let model = "mock-eval-skill-loaded";
  write_three_tool_skill(&skill_dir, model);

  // Write the models.yml that registers this case's model id.
  let cfg = home.path().join(".agentflow");
  fs::create_dir_all(&cfg).unwrap();
  fs::write(
    cfg.join("models.yml"),
    format!(
      r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
    ),
  )
  .unwrap();

  let dataset_dir = work.path().join("dataset");
  fs::create_dir_all(&dataset_dir).unwrap();
  // One case that asserts `tool_not_called` for `shell` — should pass
  // because the canned mock response answers directly without tools.
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "skill-loaded",
        "prompt": "say hello",
        "model": model,
        "expected_assertions": [
          {"type": "contains", "needle": "hello"},
          {"type": "tool_not_called", "tool": "shell"}
        ]
      })
    ),
  );

  let responses = serde_json::to_string(&vec![
    r#"{"thought":"answer directly","answer":"hello world"}"#,
  ])
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", responses)
    .assert()
    .success()
    .stdout(predicate::str::contains("\"passed\": 1"));
}

#[test]
fn cli_eval_run_tools_denied_filters_skill_registry_before_invocation() {
  // This case is structured so the *only* way the agent could pass the
  // `contains` assertion is via a tool the case explicitly denies. The
  // factory should filter that tool out of the registry, so the agent
  // can never reach for it — the case must therefore Fail with a
  // tool_called assertion mismatch, proving admission was applied.
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let skill_dir = work.path().join("eval-skill-denied");
  fs::create_dir_all(&skill_dir).unwrap();
  let model = "mock-eval-skill-denied";
  write_three_tool_skill(&skill_dir, model);

  let cfg = home.path().join(".agentflow");
  fs::create_dir_all(&cfg).unwrap();
  fs::write(
    cfg.join("models.yml"),
    format!(
      r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
    ),
  )
  .unwrap();

  let dataset_dir = work.path().join("dataset");
  fs::create_dir_all(&dataset_dir).unwrap();
  // The case asserts shell *must* be called at least once. The skill
  // declares shell, but the case denies it. Expectation: case fails
  // because the registry the agent sees has no `shell` tool, so
  // tool_called for shell hits min_count=0.
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "shell-denied",
        "prompt": "echo via shell",
        "model": model,
        "tools_denied": ["shell"],
        "expected_assertions": [
          {"type": "tool_called", "tool": "shell", "min_count": 1}
        ]
      })
    ),
  );

  let responses = serde_json::to_string(&vec![
    r#"{"thought":"answer directly","answer":"no tools used"}"#,
  ])
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", responses)
    .output()
    .unwrap();
  // Exit 1 because the case fails (tools_denied prevented the
  // shell tool from being registered → tool_called min_count=1 missed).
  assert_eq!(output.status.code(), Some(1));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["failed"], 1);
  let case = &report["cases"][0];
  assert_eq!(case["status"], "failed");
  assert_eq!(case["tool_call_count"], 0);
}

#[test]
fn cli_eval_run_cost_usd_actual_reflects_pricing_table_via_env() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());
  // Pricing table priced at $1 / 1k input + $2 / 1k output. Mock
  // provider stamps `prompt_tokens: Some(50)` and a response-word-count-
  // dependent `completion_tokens` (see agentflow-llm/src/providers/mock.rs);
  // exact dollar amount varies with response length so the test asserts
  // strictly-positive cost + proportional bounds rather than an exact
  // figure that would couple this test to mock-provider internals.
  let pricing_path = home.path().join("pricing.yml");
  fs::write(
    &pricing_path,
    r#"
models:
  mock-eval-hello:
    input_per_1k: 1.0
    output_per_1k: 2.0
  mock-eval-budget:
    input_per_1k: 1.0
    output_per_1k: 2.0
"#,
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path(), "--format", "json"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .env("AGENTFLOW_PRICING_TABLE", pricing_path.to_str().unwrap())
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let total_cost = report["summary"]["cost_usd_total"].as_f64().unwrap();
  // Lower bound: each case has ≥1 mock LLM call with prompt_tokens=50,
  // input @ $1/1k = $0.05 minimum. Two cases → ≥ $0.10.
  // Upper bound: each case has at most ~3 LLM iterations on the canned
  // response, completion tokens are bounded by response length (~3
  // words), so $0.10 × 6 = $0.60 is a safe ceiling.
  assert!(
    total_cost >= 0.05,
    "expected cost_usd_total ≥ $0.05 from mock token usage; got ${total_cost}"
  );
  assert!(
    total_cost < 1.0,
    "expected cost_usd_total < $1.00 from a 2-case mock-provider run; got ${total_cost}"
  );
  // Every case must report a strictly-positive per-case cost when its
  // model is in the pricing table — confirms the runner aggregates
  // events per-case, not just at the summary level.
  for case in report["cases"].as_array().unwrap() {
    let cost = case["cost_usd_actual"].as_f64().unwrap();
    assert!(
      cost > 0.0,
      "per-case cost_usd_actual should be > 0 when model is priced; got ${cost}",
    );
  }
}

#[test]
fn cli_eval_run_cost_usd_actual_zero_when_no_pricing_table_configured() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());
  // No AGENTFLOW_PRICING_TABLE; no ~/.agentflow/pricing.yml under
  // the synthetic HOME. The harness should fall back to an empty
  // pricing table → $0 across the run despite real token usage.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path(), "--format", "json"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .output()
    .unwrap();
  assert!(output.status.success());
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["cost_usd_total"], 0.0);
}

/// Write a skill with a `[validation] kind = "regex"` section. Used
/// to exercise `final_answer_matches_skill` end-to-end.
fn write_regex_validated_skill(dir: &Path, model: &str, pattern: &str) {
  fs::write(
    dir.join("skill.toml"),
    format!(
      r#"
[skill]
name = "validator-skill"
version = "0.1.0"
description = "Skill with a regex validator for eval tests"

[persona]
role = "Answer concisely."

[model]
name = "{model}"
max_iterations = 4

[validation]
kind = "regex"
pattern = "{pattern}"
"#
    ),
  )
  .unwrap();
}

fn write_models_yml_for(home: &Path, model: &str) {
  let cfg = home.join(".agentflow");
  fs::create_dir_all(&cfg).unwrap();
  fs::write(
    cfg.join("models.yml"),
    format!(
      r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
    ),
  )
  .unwrap();
}

#[test]
fn cli_eval_run_final_answer_matches_skill_passes_when_regex_validator_accepts() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let model = "mock-validator-pass";
  let skill_dir = work.path().join("validator-pass");
  fs::create_dir_all(&skill_dir).unwrap();
  write_regex_validated_skill(&skill_dir, model, r"\\bOK\\b");
  write_models_yml_for(home.path(), model);

  let dataset_dir = work.path().join("ds");
  fs::create_dir_all(&dataset_dir).unwrap();
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "ok-case",
        "prompt": "say ok",
        "model": model,
        "expected_assertions": [
          {"type": "final_answer_matches_skill"}
        ]
      })
    ),
  );

  // Mock response includes the word "OK" so the validator passes.
  let responses =
    serde_json::to_string(&vec![r#"{"thought":"answer","answer":"OK ready"}"#]).unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", responses)
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["passed"], 1);
}

#[test]
fn cli_eval_run_final_answer_matches_skill_fails_with_validator_reason_in_report() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let model = "mock-validator-fail";
  let skill_dir = work.path().join("validator-fail");
  fs::create_dir_all(&skill_dir).unwrap();
  write_regex_validated_skill(&skill_dir, model, r"\\bOK\\b");
  write_models_yml_for(home.path(), model);

  let dataset_dir = work.path().join("ds");
  fs::create_dir_all(&dataset_dir).unwrap();
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "missing-ok-case",
        "prompt": "should reject",
        "model": model,
        "expected_assertions": [
          {"type": "final_answer_matches_skill"}
        ]
      })
    ),
  );

  // Mock response intentionally omits "OK" so the regex fails.
  let responses = serde_json::to_string(&vec![
    r#"{"thought":"answer","answer":"totally unrelated"}"#,
  ])
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", responses)
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["failed"], 1);
  // The validator's own reason should surface verbatim into the case
  // report's assertion outcome.
  let case = &report["cases"][0];
  let assertions = case["assertions"].as_array().unwrap();
  let validator_outcome = &assertions[0];
  let reason = validator_outcome["reason"].as_str().unwrap_or("");
  assert!(
    reason.contains("regex"),
    "expected validator reason to mention the pattern; got: {reason}"
  );
}

#[test]
fn cli_eval_run_final_answer_matches_skill_no_validator_falls_through_to_no_validator_reason() {
  // Skill omits the [validation] section entirely. The harness should
  // emit the "skill declares no validator" reason rather than crashing.
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let model = "mock-no-validator";
  let skill_dir = work.path().join("no-validator");
  fs::create_dir_all(&skill_dir).unwrap();
  fs::write(
    skill_dir.join("skill.toml"),
    format!(
      r#"
[skill]
name = "no-validator"
version = "0.1.0"
description = "Skill without a [validation] section"

[persona]
role = "Answer."

[model]
name = "{model}"
max_iterations = 4
"#
    ),
  )
  .unwrap();
  write_models_yml_for(home.path(), model);

  let dataset_dir = work.path().join("ds");
  fs::create_dir_all(&dataset_dir).unwrap();
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "no-validator-case",
        "prompt": "x",
        "model": model,
        "expected_assertions": [
          {"type": "final_answer_matches_skill"}
        ]
      })
    ),
  );
  let responses =
    serde_json::to_string(&vec![r#"{"thought":"answer","answer":"anything"}"#]).unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", responses)
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let case = &report["cases"][0];
  let reason = case["assertions"][0]["reason"].as_str().unwrap_or("");
  assert!(
    reason.contains("no validator"),
    "expected 'skill declares no validator' reason; got: {reason}"
  );
}

#[test]
fn cli_eval_run_skill_load_failure_reports_factory_error() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  // Point `case.skill` at a directory that exists but has no manifest.
  let skill_dir = work.path().join("missing-skill");
  fs::create_dir_all(&skill_dir).unwrap();

  let cfg = home.path().join(".agentflow");
  fs::create_dir_all(&cfg).unwrap();
  fs::write(
    cfg.join("models.yml"),
    r#"
models:
  mock-eval-broken:
    vendor: mock
    type: text
    model_id: mock-eval-broken
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#,
  )
  .unwrap();

  let dataset_dir = work.path().join("dataset");
  fs::create_dir_all(&dataset_dir).unwrap();
  write_dataset_targeting_skill(
    &dataset_dir,
    &skill_dir,
    &format!(
      "{}\n",
      serde_json::json!({
        "id": "broken-skill",
        "prompt": "hi",
        "model": "mock-eval-broken",
        "expected_assertions": [
          {"type": "contains", "needle": "hi"}
        ]
      })
    ),
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      dataset_dir.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", "[]")
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  let case = &report["cases"][0];
  assert_eq!(case["status"], "failed");
  assert_eq!(case["stop_reason"], "factory_error");
  assert!(
    case["runtime_error"]
      .as_str()
      .unwrap()
      .contains("failed to load skill"),
    "runtime_error: {:?}",
    case["runtime_error"]
  );
}
