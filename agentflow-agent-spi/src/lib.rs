//! `agentflow-agent-spi` — the agent-runtime contracts of the AgentFlow kernel.
//!
//! Holds the interfaces a runtime loop and its governors agree on: the
//! [`AgentRuntime`](runtime::AgentRuntime) trait, the structured
//! [`AgentStep`](runtime::AgentStep) / [`AgentEvent`](runtime::AgentEvent)
//! records, [`AgentContext`](runtime::AgentContext), `RuntimeLimits`, the
//! cancellation token, and the event/memory hook traits.
//!
//! Extracted from `agentflow-agents` in P-A1.1 (RFC §4 agent-spi). The concrete
//! runtimes (`ReActAgent`, `PlanExecuteAgent`, supervisors) stay in
//! `agentflow-agents`, which re-exports everything here under its original
//! paths. This lets `agentflow-harness` govern a runtime by depending on the
//! contract rather than the `agents` impl crate (the P-A2.1 target).

pub mod runtime;
pub mod turn;

pub use runtime::*;
pub use turn::*;
