//! Dynamic-workflow vertical-slice spike (P-A1.6).
//!
//! Proves the contract kernel can carry a **dynamic workflow** — the payoff of
//! the whole crate-architecture RFC. A runtime *generates* a `Flow` at runtime
//! and the executor runs it, the two meeting only through the `agentflow-graph`
//! contract:
//!
//! ```text
//!   value ──▶ graph ──▶ (toy agent) emits a Flow ──▶ core executes it
//! ```
//!
//! The `DynamicPlanner` below stands in for `PlanExecuteAgent`: in P-A4 that
//! real agent will emit a `Flow` (with dependencies / parallel / conditional
//! nodes, some of them `AgentNode`s) instead of executing a plan step-by-step.
//! The architectural point this spike makes:
//!
//! * Building the `Flow` needs only the **IR** (`agentflow-graph`: `Flow`,
//!   `GraphNode`, `NodeType`, `AsyncNode`). A runtime can construct one without
//!   depending on the scheduler — the dynamic-workflow prerequisite.
//! * Running it is the **executor** (`agentflow-core`: `FlowExt::run`).
//! * The two never depend on each other; they share only the `graph` contract.
//!
//! Crucially, the shape of the generated `Flow` is **not known at compile time**
//! — it is derived from the `ops` list chosen at runtime (which in production
//! would come from an LLM plan). Run with:
//!
//! ```bash
//! cargo run -p agentflow-agents --example dynamic_workflow_spike
//! ```

use std::collections::HashMap;
use std::sync::Arc;

// The IR types (re-exported from `agentflow-graph` through `agentflow-core`).
// A real planner depends on `agentflow-graph` alone to build these.
use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::{FlowExt, FlowValue};
use serde_json::json;

/// A text transformation the planner can chain into a workflow.
#[derive(Clone, Copy, Debug)]
enum Op {
  Upper,
  Reverse,
  Exclaim,
}

impl Op {
  fn apply(self, text: &str) -> String {
    match self {
      Op::Upper => text.to_uppercase(),
      Op::Reverse => text.chars().rev().collect(),
      Op::Exclaim => format!("{text}!"),
    }
  }
}

/// A node that applies one [`Op`] to the `text` value in its input pool and
/// emits the transformed `text`. The executor wires each node's input from its
/// dependency's output (see `DynamicPlanner::plan`'s `input_mapping`).
struct OpNode {
  op: Op,
}

#[async_trait::async_trait]
impl AsyncNode for OpNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let text = inputs
      .get("text")
      .and_then(|v| match v {
        FlowValue::Json(j) => j.as_str(),
        _ => None,
      })
      .unwrap_or("");
    let transformed = self.op.apply(text);
    let mut out = HashMap::new();
    out.insert("text".to_string(), FlowValue::Json(json!(transformed)));
    Ok(out)
  }
}

/// Stands in for `PlanExecuteAgent`: turns a runtime goal (a seed string + a
/// list of ops) into a linear `Flow`. The number and order of nodes — i.e. the
/// graph's shape — is decided here, at runtime, not at compile time.
struct DynamicPlanner;

impl DynamicPlanner {
  fn plan(&self, seed: &str, ops: &[Op]) -> Flow {
    let mut nodes = Vec::with_capacity(ops.len());
    let mut prev: Option<String> = None;
    for (i, op) in ops.iter().enumerate() {
      let id = format!("op{i}_{op:?}").to_lowercase();
      let (dependencies, input_mapping, initial_inputs) = match &prev {
        // First node: seed the pool from the runtime goal.
        None => (
          Vec::new(),
          None,
          HashMap::from([("text".to_string(), FlowValue::Json(json!(seed)))]),
        ),
        // Subsequent nodes: read `text` from the previous node's `text` output.
        Some(prev_id) => (
          vec![prev_id.clone()],
          Some(HashMap::from([(
            "text".to_string(),
            (prev_id.clone(), "text".to_string()),
          )])),
          HashMap::new(),
        ),
      };
      nodes.push(GraphNode {
        id: id.clone(),
        node_type: NodeType::Standard(Arc::new(OpNode { op: *op })),
        dependencies,
        input_mapping,
        run_if: None,
        initial_inputs,
      });
      prev = Some(id);
    }
    Flow::new(nodes)
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // A "goal" chosen at runtime — in production this list would come from an LLM
  // plan. The compiler does not know the workflow's shape.
  let seed = "hello";
  let ops = [Op::Upper, Op::Reverse, Op::Exclaim];

  println!("goal: seed={seed:?}, ops={ops:?}");

  // 1. The agent EMITS a Flow (built from the `graph` IR only).
  let flow = DynamicPlanner.plan(seed, &ops);
  let order = flow.execution_order()?;
  println!(
    "planner emitted a {}-node Flow; execution order: {order:?}",
    order.len()
  );

  // 2. The executor RUNS it (`core::FlowExt`). The planner never touched the
  //    scheduler; the executor never touched the planner.
  let state = flow.run().await?;

  // 3. Read the final node's `text` output.
  let last = order.last().expect("non-empty workflow");
  let result = state
    .get(last)
    .and_then(|r| r.as_ref().ok())
    .and_then(|outputs| outputs.get("text"))
    .and_then(|v| match v {
      FlowValue::Json(j) => j.as_str(),
      _ => None,
    })
    .unwrap_or("<none>");

  // Upper("hello")="HELLO" -> Reverse="OLLEH" -> Exclaim="OLLEH!"
  println!("result: {result:?}");
  assert_eq!(
    result, "OLLEH!",
    "dynamic workflow produced the wrong result"
  );
  println!("✓ dynamic workflow executed end-to-end via the contract kernel");
  Ok(())
}
