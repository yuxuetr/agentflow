//! Harness context contract (`ContextProvider`, `HarnessContext` /
//! `HarnessProfile` / `HarnessRuntimeKind`, `ContextItem` / `ContextPriority`).
//!
//! Moved to `agentflow-agent-spi` in P-A1.1 step 2/2; re-exported here under
//! the original path. Concrete providers live in [`crate::providers`].

pub use agentflow_agent_spi::harness::context::*;
