//! [`CoreFlowRunner`] — the production [`FlowRunner`] backed by the executor.
//!
//! Lets a consumer that depends only on the `agentflow-graph` IR + the
//! [`FlowRunner`] contract (e.g. `agentflow-agents`' `WorkflowTool` /
//! `DynamicWorkflowAgent`) run an embedded `Flow` without depending on this
//! executor crate: the consumer takes an `Arc<dyn FlowRunner>` and the surface
//! injects a `CoreFlowRunner` (P-A: this is what burns the `agents -> core`
//! edge).

use std::collections::HashMap;

use agentflow_graph::FlowRunner;
use agentflow_graph::async_node::{AsyncNodeInputs, AsyncNodeResult};
use agentflow_graph::error::AgentFlowError;
use agentflow_graph::flow::Flow;

use crate::FlowExt;
use crate::scheduler::FlowExecutionConfig;

/// Runs a [`Flow`] via the core executor ([`FlowExt`]) with a fixed
/// [`FlowExecutionConfig`].
#[derive(Debug, Clone, Default)]
pub struct CoreFlowRunner {
  config: FlowExecutionConfig,
}

impl CoreFlowRunner {
  /// Build a runner with an explicit execution config.
  pub fn new(config: FlowExecutionConfig) -> Self {
    Self { config }
  }

  /// Serial execution (the default) — one node at a time in topological order.
  pub fn serial() -> Self {
    Self::new(FlowExecutionConfig::serial())
  }

  /// Dependency-ready concurrent execution, up to `max_concurrency` nodes.
  pub fn concurrent(max_concurrency: usize) -> Self {
    Self::new(FlowExecutionConfig::concurrent(max_concurrency))
  }
}

#[async_trait::async_trait]
impl FlowRunner for CoreFlowRunner {
  async fn run(
    &self,
    flow: &Flow,
    inputs: AsyncNodeInputs,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    flow
      .execute_from_inputs_with_config(inputs, self.config.clone())
      .await
  }
}
