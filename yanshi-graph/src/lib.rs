//! `yanshi-graph` — the Yanshi execution IR (intermediate representation).
//!
//! This crate holds the *types* a workflow is built from — `AsyncNode`,
//! `GraphNode`, `Flow`, `NodeType`, the `expr` mini-language, and the shared
//! `YanshiError` — separate from the *executor* that runs them (the
//! topological / concurrent scheduler stays in `yanshi-core`). Splitting the
//! IR from the executor (`docs/RFC_CRATE_ARCHITECTURE.md` §5) lets a runtime
//! *construct* a `Flow` by depending on `graph` alone — the dynamic-workflow
//! prerequisite — without pulling in the scheduler.
//!
//! Extracted from `yanshi-core` in P-A1.3; `yanshi-core` re-exports every
//! item here under its original path for backward compatibility.

// The universal value leaf (`yanshi-value`), surfaced under the original
// `crate::value` module path + crate root so the moved modules — and downstream
// `yanshi_graph::FlowValue` consumers — resolve unchanged.
pub use yanshi_value::{self as value, FlowValue};

pub mod async_node;
pub mod checkpoint;
pub mod error;
pub mod events;
pub mod expr;
pub mod flow;
pub mod runner;
pub mod state_size;

// Root convenience re-exports of the common IR surface, so consumers can write
// `yanshi_graph::{AsyncNode, Flow, ...}` (mirroring the `yanshi_core::*`
// re-export they may be migrating from).
pub use async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
pub use error::YanshiError;
pub use flow::{Flow, GraphNode, NodeType};
pub use runner::FlowRunner;
