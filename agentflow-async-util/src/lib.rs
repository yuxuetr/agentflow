//! `agentflow-async-util` — reliability combinators for AgentFlow.
//!
//! Houses the retry ([`retry`]) and timeout ([`timeout`]) primitives in one
//! place so the executor (`agentflow-core`) and the agent loop
//! (`agentflow-agents`) share a single implementation instead of duplicating
//! them (RFC §7 law 7; the de-dup against `agents` is P-A3.2). Extracted from
//! `agentflow-core` in P-A1.4; `agentflow-core` re-exports both modules under
//! their original paths.

// `retry`/`timeout` refer to `crate::error::{AgentFlowError, Result}`; surface
// the graph error module under that path so the moved code resolves unchanged.
pub use agentflow_graph::error;

pub mod retry;
pub mod timeout;
