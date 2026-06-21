//! The [`FlowRunner`] contract — "execute a `Flow`, give me the state pool".
//!
//! The `Flow` IR lives here in `agentflow-graph`; the *executor* (the
//! topological/concurrent scheduler) lives in `agentflow-core` behind `FlowExt`.
//! Code that needs to *run* an embedded flow but should not depend on the
//! executor runtime (e.g. `agentflow-agents`' `WorkflowTool` /
//! `DynamicWorkflowAgent`) depends on this trait and has the concrete runner
//! injected at its construction site. `agentflow-core::CoreFlowRunner` is the
//! production implementation (P-A: burns the `agents -> core` edge).

use std::collections::HashMap;

use crate::async_node::{AsyncNodeInputs, AsyncNodeResult};
use crate::error::AgentFlowError;
use crate::flow::Flow;

/// Executes a [`Flow`] from a set of initial inputs and returns the per-node
/// state pool. The execution mode (serial / concurrent, retry, timeout, …) is
/// the runner's concern, configured where the concrete runner is built.
#[async_trait::async_trait]
pub trait FlowRunner: Send + Sync {
  /// Run `flow` seeded with `inputs`, returning each node's result keyed by
  /// node id.
  async fn run(
    &self,
    flow: &Flow,
    inputs: AsyncNodeInputs,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError>;
}
