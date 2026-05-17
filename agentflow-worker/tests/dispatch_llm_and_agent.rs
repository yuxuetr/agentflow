//! P2.8 — distributed `llm` and `agent` dispatch happy-path coverage.
//!
//! Both share the global `agentflow-llm` model registry (a `OnceLock`),
//! so we keep them in the same integration binary and drive them
//! through the in-memory mock provider. Each test owns its own slice of
//! the mock response queue: every test sets `AGENTFLOW_MOCK_RESPONSES`
//! up front, then re-runs `AgentFlow::init_with_config` to rebuild the
//! provider with the fresh queue. The `SERIAL_GATE` mutex prevents
//! cross-test races on those globals.

use std::collections::HashMap;
use std::sync::OnceLock;

use agentflow_core::FlowValue;
use agentflow_server::{
  InMemoryWorkerProtocol, NodeExecutionPayload, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult,
};
use agentflow_worker::{WorkerConfig, WorkerRuntime};
use serde_json::json;
use tokio::sync::Mutex;
use uuid::Uuid;

const LLM_MODEL: &str = "mock-worker-dispatch";

/// Serial gate. All process-global state (`AGENTFLOW_MOCK_RESPONSES`,
/// `AGENTFLOW_MODELS_CONFIG`, the LLM registry) is mutated by these
/// tests, so they can't run concurrently.
///
/// `tokio::sync::Mutex` is held across `await` points safely; `std`
/// `Mutex` would trip the clippy `await_holding_lock` lint.
fn serial_gate() -> &'static Mutex<()> {
  static GATE: OnceLock<Mutex<()>> = OnceLock::new();
  GATE.get_or_init(|| Mutex::new(()))
}

/// Replace the mock response queue, then rebuild the LLM registry so
/// the new `MockProvider` snapshots the freshly seeded queue.
async fn reset_mock_with_responses(responses: &[&str]) -> anyhow::Result<()> {
  let encoded = serde_json::to_string(responses)?;

  let config_path =
    std::env::temp_dir().join(format!("agentflow-worker-mock-{}.yml", Uuid::new_v4()));
  std::fs::write(
    &config_path,
    format!(
      r#"
models:
  {LLM_MODEL}:
    vendor: mock
    type: text
    model_id: {LLM_MODEL}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
    ),
  )?;
  let config_path_str = config_path
    .to_str()
    .expect("temp path is utf-8")
    .to_string();

  // SAFETY: the SERIAL_GATE mutex held by the caller serializes env
  // writes; nothing else in the worker tests reads these vars.
  unsafe {
    std::env::set_var("AGENTFLOW_MOCK_RESPONSES", encoded);
    // LlmNode::execute calls `AgentFlow::init()` on every invocation,
    // which re-reads `AGENTFLOW_MODELS_CONFIG`. Pin it to our temp
    // YAML so the rebuild keeps the same mock model registered.
    std::env::set_var("AGENTFLOW_MODELS_CONFIG", &config_path_str);
  }
  agentflow_llm::AgentFlow::init_with_config(&config_path_str).await?;
  Ok(())
}

fn worker_id(label: &str) -> WorkerId {
  WorkerId::new(label).expect("worker label is valid")
}

async fn run_payload(worker: &str, payload: NodeExecutionPayload) -> WorkerTaskResult {
  let protocol = InMemoryWorkerProtocol::new();
  let run_id = Uuid::new_v4();
  let node_id = payload.node_id.clone();
  let task = WorkerTask::new(
    run_id,
    node_id,
    serde_json::to_value(payload).expect("payload serializes"),
  );
  let task_id = task.task_id;
  protocol.submit_task(task).await.expect("submit task");

  let runtime = WorkerRuntime::new(
    protocol.clone(),
    WorkerConfig::new(worker_id(worker), "memory://local"),
  );
  runtime.run_once().await.expect("run once");
  protocol
    .completed_result(task_id)
    .await
    .expect("result recorded")
}

/// Read a `FlowValue::Json(string)` field out of a serialized outputs map.
///
/// `FlowValue` uses a tagged envelope on the wire — `Json(s)` becomes
/// `{"type":"json","value":<inner>}`.
fn extract_json_text(output: &serde_json::Value, key: &str) -> String {
  output
    .get(key)
    .and_then(|v| v.get("value"))
    .and_then(|v| v.as_str())
    .unwrap_or_else(|| {
      panic!("expected output.{key}.value to be a JSON string; full output was {output:?}")
    })
    .to_string()
}

#[tokio::test(flavor = "current_thread")]
async fn llm_payload_returns_mock_response() {
  let _guard = serial_gate().lock().await;
  reset_mock_with_responses(&["llm-dispatched-from-worker"])
    .await
    .expect("mock LLM ready");

  let mut inputs = HashMap::new();
  inputs.insert(
    "prompt".to_string(),
    FlowValue::Json(json!("ignored prompt")),
  );
  inputs.insert("model".to_string(), FlowValue::Json(json!(LLM_MODEL)));
  inputs.insert("temperature".to_string(), FlowValue::Json(json!(0.0)));
  let payload = NodeExecutionPayload::new("call_model", "llm", HashMap::new(), inputs);

  let result = run_payload("worker-llm", payload).await;
  let WorkerTaskResult::Succeeded { output, .. } = result else {
    panic!("expected llm payload to succeed, got {result:?}");
  };
  let answer = extract_json_text(&output, "output");
  assert_eq!(answer, "llm-dispatched-from-worker");
}

#[tokio::test(flavor = "current_thread")]
async fn agent_payload_runs_react_loop_to_completion() {
  let _guard = serial_gate().lock().await;
  reset_mock_with_responses(&[
    r#"{"thought":"answer immediately","answer":"agent-dispatched-from-worker"}"#,
  ])
  .await
  .expect("mock LLM ready");

  let mut inputs = HashMap::new();
  inputs.insert(
    "message".to_string(),
    FlowValue::Json(json!("answer immediately")),
  );
  inputs.insert("model".to_string(), FlowValue::Json(json!(LLM_MODEL)));
  inputs.insert("max_iterations".to_string(), FlowValue::Json(json!(2u64)));
  let payload = NodeExecutionPayload::new("react", "agent", HashMap::new(), inputs);

  let result = run_payload("worker-agent", payload).await;
  let WorkerTaskResult::Succeeded { output, .. } = result else {
    panic!("expected agent payload to succeed, got {result:?}");
  };
  let answer = extract_json_text(&output, "answer");
  assert_eq!(answer, "agent-dispatched-from-worker");
}
