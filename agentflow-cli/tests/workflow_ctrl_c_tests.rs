//! Q3.1.2 regression — the CLI Ctrl-C handler must cancel the
//! in-flight flow AND flush trace events before exiting, so the JSONL
//! trace file the CLI tells the operator to inspect carries the
//! terminal `WorkflowCancelled` event.
//!
//! This integration test exercises the underlying primitives the CLI
//! composes (`FlowCancellationToken` + `TraceCollector::flush`) on a
//! Flow built with a slow custom `AsyncNode`, without subprocessing
//! the binary. The CLI's `workflow run` handler is a thin wrapper
//! around exactly this pattern — when the primitives behave, the
//! handler behaves; the build also verifies the wiring compiles.

use agentflow_core::FlowExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::error::AgentFlowError;
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::value::FlowValue;
use agentflow_core::{FlowCancellationToken, FlowExecutionConfig};
use agentflow_tracing::storage::TraceStorage;
use agentflow_tracing::storage::file::FileTraceStorage;
use agentflow_tracing::{TraceCollector, TraceConfig};
use async_trait::async_trait;
use tempfile::TempDir;

/// AsyncNode that finishes quickly. The CLI Ctrl-C flow checks the
/// cancellation token *between* nodes — so the realistic test path
/// is "node A finishes, token already cancelled, flow emits
/// WorkflowCancelled before node B runs".
struct FastNode;

#[async_trait]
impl AsyncNode for FastNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let mut outputs = HashMap::new();
    outputs.insert("output".into(), FlowValue::Json(serde_json::json!("done")));
    Ok(outputs)
  }
}

#[tokio::test]
async fn cancel_and_flush_writes_workflow_cancelled_to_trace_file() {
  let dir = TempDir::new().expect("trace tmp");
  let storage = Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
  let collector = Arc::new(TraceCollector::new(
    storage.clone(),
    TraceConfig::development(),
  ));

  let mut flow = Flow::default().with_event_listener(collector.clone());
  flow.add_node(GraphNode {
    id: "first".into(),
    node_type: NodeType::Standard(Arc::new(FastNode)),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  });
  flow.add_node(GraphNode {
    id: "second".into(),
    node_type: NodeType::Standard(Arc::new(FastNode)),
    dependencies: vec!["first".into()],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  });

  // Mirror the CLI's invariant: cancel BEFORE the run starts, so the
  // pre-loop check in `Flow::execute_from_inputs_with_internal` trips
  // immediately and emits `WorkflowCancelled`. This is the same
  // codepath the CLI's signal handler hits — `cancel_token.cancel()`
  // happens before `tokio::time::timeout` waits for the in-flight
  // node to acknowledge.
  let token = FlowCancellationToken::new();
  token.cancel();
  let workflow_id = "wf-ctrl-c".to_string();

  let result = flow
    .execute_from_inputs_with_id_and_config(
      workflow_id.clone(),
      AsyncNodeInputs::new(),
      FlowExecutionConfig::serial().with_cancellation_token(token),
    )
    .await;
  assert!(
    matches!(result, Err(AgentFlowError::TaskCancelled)),
    "the flow must surface TaskCancelled when the token is pre-set; got {result:?}"
  );

  // Now drain the trace collector — the CLI handler does this before
  // `process::exit(130)`. Without it the JSONL file could end mid
  // event because the drain task is still spawning.
  let drained = collector.flush(Duration::from_secs(5)).await;
  assert!(
    drained,
    "trace collector flush must catch up after cancellation; submitted={} processed={}",
    collector.submitted_count(),
    collector.processed_count()
  );

  // Storage must reflect the cancellation as a terminal status —
  // before the Q3.1.2 fix, `WorkflowCancelled` was ignored by
  // `process_event` and the trace was never persisted at all
  // (workflow stayed "Running" forever in `agentflow trace tui`).
  let trace = storage
    .get_trace(&workflow_id)
    .await
    .expect("storage lookup")
    .expect("Q3.1.2: trace must be persisted after cancellation");

  let status_label = format!("{:?}", trace.status);
  assert!(
    status_label.to_lowercase().contains("cancel"),
    "trace status must reflect cancellation; got {status_label}"
  );
}

#[tokio::test]
async fn ctrl_c_constants_match_posix() {
  // Defensive: pin the constants the handler exits with. If a
  // refactor changes them by accident the contract breaks.
  assert_eq!(agentflow_cli::shutdown::SIGINT_EXIT_CODE, 130);
  assert!(
    agentflow_cli::shutdown::DEFAULT_TRACE_FLUSH_TIMEOUT >= Duration::from_secs(1),
    "flush timeout must be generous enough for a healthy local FS write"
  );
}
