//! Harness error contract.
//!
//! Moved to `agentflow-agent-spi` in P-A1.1 step 2/2 (RFC §4) so the
//! operations crates depend on the kernel contract, not this runtime crate.
//! Re-exported here under the original path for compatibility.

pub use agentflow_agent_spi::harness::error::*;
