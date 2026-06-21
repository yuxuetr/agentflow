//! Harness event contract (`HarnessEvent` envelope + payload types).
//!
//! Moved to `agentflow-agent-spi` in P-A1.1 step 2/2; re-exported here under
//! the original path so existing JSONL / SSE / stdout consumers are unchanged.

pub use agentflow_agent_spi::harness::event::*;
