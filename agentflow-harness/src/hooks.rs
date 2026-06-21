//! Harness tool-hook contract (`PreToolHook` / `PostToolHook` and their
//! call-info structs).
//!
//! Moved to `agentflow-agent-spi` in P-A1.1 step 2/2; re-exported here under
//! the original path. The hook execution pipeline lives in
//! [`crate::hooks_runtime`].

pub use agentflow_agent_spi::harness::hooks::*;
