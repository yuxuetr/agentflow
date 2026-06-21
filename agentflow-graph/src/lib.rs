//! `agentflow-graph` — the AgentFlow execution IR (intermediate representation).
//!
//! This crate holds the *types* a workflow is built from — `AsyncNode`,
//! `GraphNode`, `Flow`, `NodeType`, the `expr` mini-language, and the shared
//! `AgentFlowError` — separate from the *executor* that runs them (the
//! topological / concurrent scheduler stays in `agentflow-core`). Splitting the
//! IR from the executor (`docs/RFC_CRATE_ARCHITECTURE.md` §5) lets a runtime
//! *construct* a `Flow` by depending on `graph` alone — the dynamic-workflow
//! prerequisite — without pulling in the scheduler.
//!
//! Extracted from `agentflow-core` in P-A1.3; `agentflow-core` re-exports every
//! item here under its original path for backward compatibility.

// The universal value leaf (`agentflow-value`), surfaced under the original
// `crate::value` module path + crate root so the moved modules — and downstream
// `agentflow_graph::FlowValue` consumers — resolve unchanged.
pub use agentflow_value::{self as value, FlowValue};

pub mod async_node;
pub mod checkpoint;
pub mod error;
pub mod events;
pub mod expr;
pub mod flow;
pub mod node;
pub mod state_size;

// Root convenience re-export: several IR modules refer to `crate::AgentFlowError`.
pub use error::AgentFlowError;
