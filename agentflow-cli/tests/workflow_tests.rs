use agentflow_core::{
  error::AgentFlowError,
  flow::{Flow, GraphNode, NodeType},
  value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

// Helper function to check for API key and skip test if not present
fn check_api_key() -> bool {
  if std::env::var("STEPFUN_API_KEY").is_err() {
    println!("Skipping integration test: STEPFUN_API_KEY not set.");
    true
  } else {
    false
  }
}

fn write_template_workflow(dir: &TempDir) -> std::path::PathBuf {
  let workflow = dir.path().join("template_workflow.yml");
  fs::write(
    &workflow,
    r#"
name: CLI Template Workflow
nodes:
  - id: render
    type: template
    parameters:
      template: "Hello {{ topic }}"
"#,
  )
  .unwrap();
  workflow
}

fn fixed_dag_multibranch_fixture() -> std::path::PathBuf {
  std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("examples/workflows/fixed_dag_multibranch.yml")
}

fn write_mock_models_config(home: &TempDir) {
  let config_dir = home.path().join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join("models.yml"),
    r#"
models:
  mock-model:
    vendor: mock
    type: text
    model_id: mock-model
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#,
  )
  .unwrap();
}

fn write_llm_workflow(dir: &TempDir) -> std::path::PathBuf {
  let workflow = dir.path().join("llm_workflow.yml");
  fs::write(
    &workflow,
    r#"
name: CLI LLM Workflow
nodes:
  - id: answer
    type: llm
    parameters:
      prompt: "Say hello"
"#,
  )
  .unwrap();
  workflow
}

fn write_invalid_llm_workflow(dir: &TempDir) -> std::path::PathBuf {
  let workflow = dir.path().join("invalid_llm_workflow.yml");
  fs::write(
    &workflow,
    r#"
name: Invalid LLM Workflow
nodes:
  - id: answer
    type: llm
    parameters:
      model: "mock-model"
"#,
  )
  .unwrap();
  workflow
}

fn write_unknown_parameter_workflow(dir: &TempDir) -> std::path::PathBuf {
  let workflow = dir.path().join("unknown_parameter_workflow.yml");
  fs::write(
    &workflow,
    r#"
name: Unknown Parameter Workflow
nodes:
  - id: render
    type: template
    parameters:
      template: "Hello"
      typo_param: true
"#,
  )
  .unwrap();
  workflow
}

#[cfg(not(feature = "mcp"))]
fn write_mcp_workflow(dir: &TempDir) -> std::path::PathBuf {
  let workflow = dir.path().join("mcp_workflow.yml");
  fs::write(
    &workflow,
    r#"
name: MCP Workflow
nodes:
  - id: list_files
    type: mcp
    parameters:
      server_command: ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
      tool_name: list_directory
"#,
  )
  .unwrap();
  workflow
}

fn write_basic_skill(dir: &TempDir) -> std::path::PathBuf {
  let skill_dir = dir.path().join("review-skill");
  fs::create_dir_all(&skill_dir).unwrap();
  fs::write(
    skill_dir.join("skill.toml"),
    r#"
[skill]
name = "review-skill"
version = "0.1.0"
description = "Review skill for workflow tests"

[persona]
role = "Return a concise review."
"#,
  )
  .unwrap();
  skill_dir
}

fn write_named_skill(dir: &TempDir, name: &str, role: &str) -> std::path::PathBuf {
  let skill_dir = dir.path().join(name);
  fs::create_dir_all(&skill_dir).unwrap();
  fs::write(
    skill_dir.join("skill.toml"),
    format!(
      r#"
[skill]
name = "{name}"
version = "0.1.0"
description = "Multi-agent test skill: {name}"

[persona]
role = "{role}"
"#
    ),
  )
  .unwrap();
  skill_dir
}

fn write_multi_agent_handoff_workflow(
  dir: &TempDir,
  triage_dir: &std::path::Path,
  billing_dir: &std::path::Path,
) -> std::path::PathBuf {
  let workflow = dir.path().join("multi_agent_handoff_workflow.yml");
  fs::write(
    &workflow,
    format!(
      r#"
name: CLI Multi-Agent Handoff Workflow
nodes:
  - id: prepare
    type: template
    parameters:
      template: "Refund my duplicate charge"
  - id: pipeline
    type: multi_agent
    dependencies: ["prepare"]
    input_mapping:
      message: "{{{{ nodes.prepare.outputs.output }}}}"
    parameters:
      mode: handoff
      initial_agent: triage
      max_handoffs: 3
      agents:
        - name: triage
          skill: {triage:?}
        - name: billing
          skill: {billing:?}
"#,
      triage = triage_dir.display().to_string(),
      billing = billing_dir.display().to_string(),
    ),
  )
  .unwrap();
  workflow
}

fn write_skill_agent_workflow(dir: &TempDir, skill_dir: &std::path::Path) -> std::path::PathBuf {
  let workflow = dir.path().join("skill_agent_workflow.yml");
  fs::write(
    &workflow,
    format!(
      r#"
name: CLI Skill Agent Workflow
nodes:
  - id: prepare
    type: template
    parameters:
      template: "Review AgentFlow"
  - id: review
    type: skill_agent
    dependencies: ["prepare"]
    input_mapping:
      message: "{{{{ nodes.prepare.outputs.output }}}}"
    parameters:
      skill: {:?}
"#,
      skill_dir.display().to_string()
    ),
  )
  .unwrap();
  workflow
}

#[test]
fn cli_workflow_run_dry_run_shows_execution_order_without_running_nodes() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "run", workflow.to_str().unwrap(), "--dry-run"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Dry run complete"))
    .stdout(predicate::str::contains("1. render"))
    .stdout(predicate::str::contains("Final State Pool").not());
}

#[test]
fn cli_workflow_run_accepts_inputs_and_writes_output_file() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);
  let output = work.path().join("result.json");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--input",
      "topic",
      "AgentFlow",
      "--output",
      output.to_str().unwrap(),
    ])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Final state written"));

  let saved = fs::read_to_string(output).unwrap();
  assert!(saved.contains("Hello AgentFlow"));
}

#[test]
fn cli_workflow_run_accepts_explicit_run_artifacts_directory() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);
  let run_dir = work.path().join("runs");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--input",
      "topic",
      "AgentFlow",
      "--run-dir",
      run_dir.to_str().unwrap(),
    ])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Run artifacts directory"));

  let run_dirs = fs::read_dir(&run_dir)
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
  assert_eq!(run_dirs.len(), 1);
  assert!(run_dirs[0].path().join("render_outputs.json").exists());
}

#[test]
fn cli_workflow_run_uses_env_run_artifacts_directory() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);
  let run_dir = work.path().join("env-runs");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "run", workflow.to_str().unwrap()])
    .env("HOME", home.path())
    .env("AGENTFLOW_RUN_DIR", &run_dir)
    .assert()
    .success()
    .stdout(predicate::str::contains("Run artifacts directory"));

  let run_dirs = fs::read_dir(&run_dir)
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
  assert_eq!(run_dirs.len(), 1);
  assert!(run_dirs[0].path().join("render_outputs.json").exists());
}

#[test]
fn cli_workflow_run_accepts_concurrent_execution_mode() {
  let home = TempDir::new().unwrap();
  let workflow = fixed_dag_multibranch_fixture();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--input",
      "topic",
      "AgentFlow",
      "--execution-mode",
      "concurrent",
      "--max-concurrency",
      "2",
    ])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Execution mode: concurrent"))
    .stdout(predicate::str::contains("Branch A saw Topic: AgentFlow"))
    .stdout(predicate::str::contains("Branch B saw Topic: AgentFlow"))
    .stdout(predicate::str::contains("join"));
}

#[test]
fn cli_workflow_run_dry_run_ignores_execution_mode_and_does_not_run_nodes() {
  let home = TempDir::new().unwrap();
  let workflow = fixed_dag_multibranch_fixture();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--dry-run",
      "--execution-mode",
      "concurrent",
      "--max-concurrency",
      "2",
    ])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Dry run complete"))
    .stdout(predicate::str::contains("1. prepare"))
    .stdout(predicate::str::contains("branch_a"))
    .stdout(predicate::str::contains("branch_b"))
    .stdout(predicate::str::contains("Final State Pool").not())
    .stdout(predicate::str::contains("Execution mode: concurrent").not())
    .stdout(predicate::str::contains("Branch A saw Topic").not());
}

#[test]
fn cli_workflow_debug_plan_mentions_concurrent_ready_nodes() {
  let home = TempDir::new().unwrap();
  let workflow = fixed_dag_multibranch_fixture();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "debug", workflow.to_str().unwrap(), "--plan"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("EXECUTION PLAN"))
    .stdout(predicate::str::contains("Concurrent mode hint"))
    .stdout(predicate::str::contains(
      "workflow run --execution-mode concurrent",
    ))
    .stdout(predicate::str::contains("Level 1 (2 nodes):"));
}

#[test]
fn cli_workflow_run_rejects_zero_max_concurrency() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--execution-mode",
      "concurrent",
      "--max-concurrency",
      "0",
    ])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "--max-concurrency must be greater than zero",
    ));
}

#[test]
fn cli_workflow_run_rejects_unimplemented_watch_flag() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_template_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "run", workflow.to_str().unwrap(), "--watch"])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stderr(predicate::str::contains("--watch is not implemented yet"));
}

#[test]
fn cli_workflow_run_validates_required_node_parameters_before_execution() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_invalid_llm_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "run", workflow.to_str().unwrap(), "--dry-run"])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stderr(predicate::str::contains("failed schema validation"))
    .stdout(predicate::str::contains("requires 'prompt'"));
}

#[test]
fn cli_workflow_validate_outputs_machine_readable_json() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_invalid_llm_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "validate",
      workflow.to_str().unwrap(),
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stdout(predicate::str::contains("\"valid\": false"))
    .stdout(predicate::str::contains("requires 'prompt'"));
}

#[test]
fn cli_workflow_validate_warns_for_unknown_parameters_by_default() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_unknown_parameter_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "validate", workflow.to_str().unwrap()])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("Schema warnings"))
    .stdout(predicate::str::contains("typo_param"));
}

#[test]
fn cli_workflow_validate_strict_rejects_unknown_parameters() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_unknown_parameter_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "validate",
      workflow.to_str().unwrap(),
      "--strict",
      "--format",
      "json",
    ])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stdout(predicate::str::contains("\"valid\": false"))
    .stdout(predicate::str::contains("\"issues\""))
    .stdout(predicate::str::contains("typo_param"));
}

#[cfg(not(feature = "mcp"))]
#[test]
fn cli_workflow_run_reports_feature_gated_mcp_node() {
  let home = TempDir::new().unwrap();
  let work = TempDir::new().unwrap();
  let workflow = write_mcp_workflow(&work);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["workflow", "run", workflow.to_str().unwrap(), "--dry-run"])
    .env("HOME", home.path())
    .assert()
    .failure()
    .stderr(predicate::str::contains("failed schema validation"))
    .stdout(predicate::str::contains("enable the `mcp` feature"));
}

#[test]
fn cli_workflow_run_model_override_applies_to_llm_nodes() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(&home);
  let work = TempDir::new().unwrap();
  let workflow = write_llm_workflow(&work);
  let output = work.path().join("llm-result.json");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--model",
      "mock-model",
      "--output",
      output.to_str().unwrap(),
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSE", "mocked workflow answer")
    .assert()
    .success()
    .stdout(predicate::str::contains("Model override: mock-model"));

  let saved = fs::read_to_string(output).unwrap();
  assert!(saved.contains("mocked workflow answer"));
}

#[test]
fn cli_workflow_run_supports_multi_agent_handoff_node() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(&home);
  let work = TempDir::new().unwrap();
  let triage = write_named_skill(&work, "triage", "Hand off to billing.");
  let billing = write_named_skill(&work, "billing", "Resolve billing issues.");
  let workflow = write_multi_agent_handoff_workflow(&work, &triage, &billing);
  let output = work.path().join("multi-agent-result.json");

  // Three responses driving the supervisor:
  // 1) triage hands off to billing
  // 2) triage's brief wrap-up (discarded by supervisor)
  // 3) billing produces the final answer
  let mock_responses = serde_json::to_string(&vec![
    r#"{"thought":"this is billing","action":{"tool":"handoff","params":{"to":"billing","message":"refund duplicate charge"}}}"#,
    r#"{"thought":"transferred","answer":"transferring to billing"}"#,
    r#"{"thought":"approve","answer":"refund processed"}"#,
  ]).unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--model",
      "mock-model",
      "--output",
      output.to_str().unwrap(),
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses)
    // Fallback if the queue is exhausted unexpectedly — keeps the test
    // from hanging on a generic mock response that the parser would treat
    // as a final answer.
    .env(
      "AGENTFLOW_MOCK_RESPONSE",
      r#"{"thought":"fallback","answer":"fallback"}"#,
    )
    .assert()
    .success();

  let saved = fs::read_to_string(output).unwrap();
  assert!(
    saved.contains("refund processed"),
    "expected billing's answer in output, got: {saved}"
  );
  assert!(
    saved.contains("agent_result"),
    "expected agent_result key in output"
  );
}

#[test]
fn cli_workflow_run_supports_skill_agent_node() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(&home);
  let work = TempDir::new().unwrap();
  let skill_dir = write_basic_skill(&work);
  let workflow = write_skill_agent_workflow(&work, &skill_dir);
  let output = work.path().join("skill-agent-result.json");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "run",
      workflow.to_str().unwrap(),
      "--model",
      "mock-model",
      "--output",
      output.to_str().unwrap(),
    ])
    .env("HOME", home.path())
    .env(
      "AGENTFLOW_MOCK_RESPONSE",
      r#"{"thought":"done","answer":"skill agent reviewed"}"#,
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("Model override: mock-model"));

  let saved = fs::read_to_string(output).unwrap();
  assert!(saved.contains("skill agent reviewed"));
  assert!(saved.contains("agent_resume"));
}

#[tokio::test]
async fn test_simple_two_step_llm_workflow() {
  if check_api_key() {
    return;
  }

  let prompt_generator_node = GraphNode {
    id: "prompt_generator".to_string(),
    node_type: NodeType::Standard(Arc::new(TemplateNode::new(
      "prompt_generator",
      "Use a single word to answer: What is the capital of France?",
    ))),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  };

  let answer_generator_node = GraphNode {
    id: "answer_generator".to_string(),
    node_type: NodeType::Standard(Arc::new(LlmNode)),
    dependencies: vec!["prompt_generator".to_string()],
    input_mapping: Some({
      let mut map = HashMap::new();
      map.insert(
        "prompt".to_string(),
        ("prompt_generator".to_string(), "output".to_string()),
      );
      map
    }),
    run_if: None,
    initial_inputs: {
      let mut map = HashMap::new();
      map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
      map
    },
  };

  let flow = Flow::new(vec![prompt_generator_node, answer_generator_node]);
  let final_state = flow.run().await.expect("Flow execution failed");

  let llm_result = final_state
    .get("answer_generator")
    .expect("LLM node not in final state")
    .as_ref()
    .expect("LLM node result was an error");
  let final_answer = llm_result.get("output").expect("LLM output not found");

  if let FlowValue::Json(serde_json::Value::String(answer_str)) = final_answer {
    println!("LLM Answer: {}", answer_str);
    assert!(answer_str.to_lowercase().contains("paris"));
  } else {
    panic!("Final answer was not a string FlowValue");
  }
}

#[tokio::test]
async fn test_conditional_workflow_runs() {
  if check_api_key() {
    return;
  }

  let condition_node = GraphNode {
    id: "condition_node".to_string(),
    node_type: NodeType::Standard(Arc::new(LlmNode)),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: {
      let mut map = HashMap::new();
      map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
      map.insert(
        "prompt".to_string(),
        FlowValue::Json(json!(
          "Is the sky blue? Respond with only the word 'true' or 'false'."
        )),
      );
      map
    },
  };

  let conditional_node = GraphNode {
    id: "conditional_node".to_string(),
    node_type: NodeType::Standard(Arc::new(TemplateNode::new(
      "conditional_node",
      "The condition was true!",
    ))),
    dependencies: vec!["condition_node".to_string()],
    input_mapping: None,
    run_if: Some("{{ nodes.condition_node.outputs.output }}".to_string()),
    initial_inputs: HashMap::new(),
  };

  let flow = Flow::new(vec![condition_node, conditional_node]);
  let final_state = flow.run().await.expect("Flow execution failed");

  let conditional_result = final_state
    .get("conditional_node")
    .expect("Conditional node not in final state")
    .as_ref();
  assert!(
    conditional_result.is_ok(),
    "Conditional node should have run"
  );
  let outputs = conditional_result.unwrap();
  let message = outputs.get("output").unwrap();
  assert_eq!(message, &FlowValue::Json(json!("The condition was true!")));
}

#[tokio::test]
async fn test_conditional_workflow_skips() {
  if check_api_key() {
    return;
  }

  let condition_node = GraphNode {
    id: "condition_node".to_string(),
    node_type: NodeType::Standard(Arc::new(LlmNode)),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: {
      let mut map = HashMap::new();
      map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
      map.insert(
        "prompt".to_string(),
        FlowValue::Json(json!(
          "Is the earth flat? Respond with only the word 'true' or 'false'."
        )),
      );
      map
    },
  };

  let conditional_node = GraphNode {
    id: "conditional_node".to_string(),
    node_type: NodeType::Standard(Arc::new(TemplateNode::new(
      "conditional_node",
      "This should not be rendered.",
    ))),
    dependencies: vec!["condition_node".to_string()],
    input_mapping: None,
    run_if: Some("{{ nodes.condition_node.outputs.output }}".to_string()),
    initial_inputs: HashMap::new(),
  };

  let flow = Flow::new(vec![condition_node, conditional_node]);
  let final_state = flow.run().await.expect("Flow execution failed");

  let conditional_result = final_state
    .get("conditional_node")
    .expect("Conditional node not in final state")
    .as_ref();
  assert!(matches!(
    conditional_result,
    Err(AgentFlowError::NodeSkipped)
  ));
}

#[tokio::test]
async fn test_parallel_map_workflow() {
  if check_api_key() {
    return;
  }

  let sub_flow_template = vec![
    GraphNode {
      id: "poem_prompt".to_string(),
      node_type: NodeType::Standard(Arc::new(TemplateNode::new(
        "poem_prompt",
        "Write a four-line poem about {{item}}.",
      ))),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "poem_generator".to_string(),
      node_type: NodeType::Standard(Arc::new(LlmNode)),
      dependencies: vec!["poem_prompt".to_string()],
      input_mapping: Some(
        [(
          "prompt".to_string(),
          ("poem_prompt".to_string(), "output".to_string()),
        )]
        .into(),
      ),
      run_if: None,
      initial_inputs: {
        let mut map = HashMap::new();
        map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
        map
      },
    },
  ];

  let map_node = GraphNode {
    id: "parallel_poem_map".to_string(),
    node_type: NodeType::Map {
      template: sub_flow_template,
      parallel: true,
    },
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: {
      let mut map = HashMap::new();
      let topics = vec!["the sun", "the moon", "the stars"];
      map.insert("input_list".to_string(), FlowValue::Json(json!(topics)));
      map
    },
  };

  let flow = Flow::new(vec![map_node]);
  let final_state = flow.run().await.expect("Flow execution failed");

  let map_result = final_state
    .get("parallel_poem_map")
    .expect("Map node not in final state")
    .as_ref()
    .expect("Map node result was an error");
  let results_array = match map_result.get("results") {
    Some(FlowValue::Json(Value::Array(arr))) => arr,
    _ => panic!("Map output was not a JSON array"),
  };

  assert_eq!(
    results_array.len(),
    3,
    "Should have produced 3 results for 3 inputs"
  );
  println!("Parallel map execution successful with 3 results.");
}

#[tokio::test]
async fn test_stateful_while_loop_workflow() {
  if check_api_key() {
    return;
  }

  let sub_flow_template = vec![
    GraphNode {
      id: "decrementer_prompt".to_string(),
      node_type: NodeType::Standard(Arc::new(TemplateNode::new(
        "decrementer_prompt",
        "Calculate {{counter}} - 1. Respond with only the resulting number.",
      ))),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "decrementer_llm".to_string(),
      node_type: NodeType::Standard(Arc::new(LlmNode)),
      dependencies: vec!["decrementer_prompt".to_string()],
      input_mapping: Some(
        [(
          "prompt".to_string(),
          ("decrementer_prompt".to_string(), "output".to_string()),
        )]
        .into(),
      ),
      run_if: None,
      initial_inputs: {
        let mut map = HashMap::new();
        map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
        map
      },
    },
    GraphNode {
      id: "state_updater".to_string(),
      node_type: NodeType::Standard(Arc::new(
        TemplateNode::new("state_updater", "{{output}}").with_output_key("counter"),
      )),
      dependencies: vec!["decrementer_llm".to_string()],
      input_mapping: Some(
        [(
          "output".to_string(),
          ("decrementer_llm".to_string(), "output".to_string()),
        )]
        .into(),
      ),
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ];

  let while_node = GraphNode {
    id: "counter_loop".to_string(),
    node_type: NodeType::While {
      condition: "{{counter}}".to_string(),
      max_iterations: 5,
      template: sub_flow_template,
    },
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: {
      let mut map = HashMap::new();
      map.insert("counter".to_string(), FlowValue::Json(json!("2"))); // Start with a string
      map
    },
  };

  let flow = Flow::new(vec![while_node]);
  let final_state = flow.run().await.expect("Flow execution failed");

  let loop_result = final_state
    .get("counter_loop")
    .expect("Loop node not in final state")
    .as_ref()
    .expect("Loop node result was an error");
  let final_count = loop_result.get("counter").expect("Final count not found");

  assert_eq!(final_count, &FlowValue::Json(json!("0")));
}

#[cfg(feature = "plugin")]
mod plugin_node_tests {
  use super::*;
  use std::path::{Path, PathBuf};
  use std::process::Command as StdCommand;
  use std::sync::OnceLock;

  const ECHO_PLUGIN_BIN: &str = "agentflow-echo-plugin";

  /// Build the in-tree reference plugin binary on first call and cache the
  /// resolved path. Mirrors the helper in
  /// `agentflow-core/tests/plugin_poc.rs` so the CLI test does not have to
  /// pre-build the plugin separately.
  fn ensure_echo_plugin_built() -> PathBuf {
    static CACHED: OnceLock<PathBuf> = OnceLock::new();
    CACHED
      .get_or_init(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        // The CLI test crate's manifest dir is agentflow-cli/. The echo
        // plugin lives in agentflow-core/, so build it via -p selection.
        let mut command = StdCommand::new(&cargo);
        command.args([
          "build",
          "--quiet",
          "-p",
          "agentflow-core",
          "--features",
          "plugin",
          "--bin",
          ECHO_PLUGIN_BIN,
        ]);
        if let Some(target_dir) = current_test_target_dir() {
          command.arg("--target-dir").arg(target_dir);
        }
        let status = command
          .status()
          .expect("failed to invoke cargo build for echo plugin");
        assert!(status.success(), "cargo build for echo plugin failed");

        let exe_name = format!("{ECHO_PLUGIN_BIN}{}", std::env::consts::EXE_SUFFIX);
        let mut candidates: Vec<PathBuf> = Vec::new();
        for dir in candidate_target_dirs() {
          for profile in ["debug", "release"] {
            candidates.push(dir.join(profile).join(&exe_name));
          }
        }
        candidates
          .iter()
          .find(|p| p.exists())
          .cloned()
          .unwrap_or_else(|| {
            panic!("could not locate freshly-built '{exe_name}' in any of: {candidates:?}")
          })
      })
      .clone()
  }

  fn candidate_target_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(custom) = std::env::var("CARGO_TARGET_DIR") {
      dirs.push(PathBuf::from(custom));
    }
    if let Some(target_tmpdir) = option_env!("CARGO_TARGET_TMPDIR") {
      let path = PathBuf::from(target_tmpdir);
      if let Some(target) = path.parent() {
        dirs.push(target.to_path_buf());
      }
    }
    if let Ok(current) = std::env::current_exe()
      && let Some(deps) = current.parent()
      && let Some(profile) = deps.parent()
      && let Some(target) = profile.parent()
    {
      dirs.push(target.to_path_buf());
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace_root) = manifest_dir.parent() {
      dirs.push(workspace_root.join("target"));
    }
    dirs
  }

  fn current_test_target_dir() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let deps = current.parent()?;
    let profile = deps.parent()?;
    profile.parent().map(Path::to_path_buf)
  }

  fn write_plugin_manifest(dir: &Path, entrypoint: &Path) -> PathBuf {
    let manifest = format!(
      r#"
[plugin]
name = "agentflow-echo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "{}"

[[plugin.nodes]]
type = "echo_uppercase"
description = "Uppercase a JSON string."
"#,
      entrypoint.display()
    );
    let path = dir.join("plugin.toml");
    fs::write(&path, manifest).unwrap();
    path
  }

  fn write_plugin_workflow(dir: &Path, manifest_path: &Path) -> PathBuf {
    let workflow = dir.join("plugin_workflow.yml");
    fs::write(
      &workflow,
      format!(
        r#"
name: CLI Plugin Workflow
nodes:
  - id: shout
    type: plugin
    parameters:
      manifest: {manifest:?}
      node_type: echo_uppercase
      text: "hello plugin"
"#,
        manifest = manifest_path.display().to_string(),
      ),
    )
    .unwrap();
    workflow
  }

  #[test]
  fn cli_workflow_run_supports_plugin_node() {
    let plugin_bin = ensure_echo_plugin_built();
    let work = TempDir::new_in("/tmp").unwrap();
    let manifest = write_plugin_manifest(work.path(), &plugin_bin);
    let workflow = write_plugin_workflow(work.path(), &manifest);
    let output = work.path().join("plugin-result.json");
    let run_dir = work.path().join("runs");

    let assertion = Command::cargo_bin("agentflow")
      .unwrap()
      .args([
        "workflow",
        "run",
        workflow.to_str().unwrap(),
        "--run-dir",
        run_dir.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
      ])
      .assert();
    if let Err(err) = assertion.try_success() {
      let run_files = collect_run_files(&run_dir);
      panic!("plugin workflow command failed: {err}\nrun files:\n{run_files}");
    }

    let saved = fs::read_to_string(&output).unwrap();
    assert!(
      saved.contains("HELLO PLUGIN"),
      "expected uppercased plugin output in saved state, got: {saved}"
    );
  }

  #[test]
  fn cli_workflow_run_rejects_plugin_node_missing_manifest() {
    let work = TempDir::new_in("/tmp").unwrap();
    let workflow = work.path().join("bad_plugin_workflow.yml");
    fs::write(
      &workflow,
      r#"
name: Bad Plugin Workflow
nodes:
  - id: missing_manifest
    type: plugin
    parameters:
      node_type: echo_uppercase
"#,
    )
    .unwrap();

    Command::cargo_bin("agentflow")
      .unwrap()
      .args(["workflow", "run", workflow.to_str().unwrap()])
      .assert()
      .failure()
      .stderr(predicate::str::contains("failed schema validation"))
      .stdout(predicate::str::contains("requires 'manifest'"));
  }

  fn collect_run_files(run_dir: &Path) -> String {
    if !run_dir.exists() {
      return "<no run dir>".to_string();
    }
    let mut out = String::new();
    for entry in walkdir::WalkDir::new(run_dir)
      .into_iter()
      .filter_map(Result::ok)
      .filter(|entry| entry.file_type().is_file())
    {
      out.push_str(&format!("--- {}\n", entry.path().display()));
      match fs::read_to_string(entry.path()) {
        Ok(content) => out.push_str(&content),
        Err(err) => out.push_str(&format!("<read error: {err}>")),
      }
      out.push('\n');
    }
    out
  }
}
