//! End-to-end smoke test: a real `agentflow_agents::ReActAgent` driven
//! by the mock LLM provider, wrapped inside the Harness runtime.
//!
//! This complements the scripted unit tests in `runtime.rs` by proving
//! the `Box<dyn AgentRuntime>` boundary actually delivers a working
//! agent through the Harness wrapper. The mock provider is configured
//! via env vars, so a static `Mutex` serializes mutation across tests
//! inside this binary.

use std::sync::Arc;
use std::sync::OnceLock;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_harness::{
  AgentsMdProvider, HarnessEventBody, HarnessEventSink, HarnessRunOptions, HarnessRuntime,
  InMemoryEventSink, StopReason,
};
use agentflow_llm::AgentFlow;
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;
use tokio::sync::Mutex;

fn env_lock() -> &'static Mutex<()> {
  static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
  LOCK.get_or_init(|| Mutex::new(()))
}

async fn init_mock_model(model: &str) {
  let config_path = std::env::temp_dir().join(format!(
    "agentflow-harness-mock-{}.yml",
    uuid::Uuid::new_v4()
  ));
  std::fs::write(
    &config_path,
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

  AgentFlow::init_with_config(config_path.to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn harness_runtime_drives_react_agent_with_mock_provider() {
  let _guard = env_lock().lock().await;
  let model = format!("mock-harness-{}", uuid::Uuid::new_v4());

  // SAFETY: env_lock() serializes mutation of process-wide mock env vars.
  unsafe {
    std::env::set_var(
      "AGENTFLOW_MOCK_RESPONSES",
      serde_json::to_string(&vec![
        r#"{"thought":"answer directly","answer":"hi from harness"}"#,
      ])
      .unwrap(),
    );
  }

  init_mock_model(&model).await;

  let agent = ReActAgent::new(
    ReActConfig::new(&model).with_max_iterations(2),
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  );

  let dir = tempfile::tempdir().unwrap();
  tokio::fs::write(
    dir.path().join("AGENTS.md"),
    "keep answers short; mention harness in the reply.\n",
  )
  .await
  .unwrap();

  let sink = Arc::new(InMemoryEventSink::new());
  let mut runtime = HarnessRuntime::new(Box::new(agent))
    .with_context_provider(Arc::new(AgentsMdProvider::new()))
    .with_event_sink(sink.clone() as Arc<dyn HarnessEventSink>);

  let result = runtime
    .run(HarnessRunOptions::new("hi", dir.path(), &model))
    .await
    .expect("harness run succeeds");

  assert_eq!(result.answer.as_deref(), Some("hi from harness"));
  assert_eq!(result.context_items_admitted, 1);
  assert!(!result.session_id.is_empty());

  let events = sink.snapshot().await;
  assert!(events.len() >= 3, "expected ≥3 events, got {}", events.len());
  let first = &events[0];
  assert!(matches!(first.body, HarnessEventBody::SessionStarted(_)));
  assert_eq!(first.seq, 0, "first event must have seq 0");
  let last = events.last().unwrap();
  match &last.body {
    HarnessEventBody::Stopped(payload) => {
      assert_eq!(payload.reason, StopReason::Completed);
      assert_eq!(payload.final_answer.as_deref(), Some("hi from harness"));
    }
    other => panic!("expected stopped, got {other:?}"),
  }

  // SAFETY: cleanup of dedicated mock env vars after read.
  unsafe {
    std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
  }
}
