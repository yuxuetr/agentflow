//! Harness approval contract (`ApprovalRequest` / `ApprovalDecision` /
//! `ApprovalProvider`, risk / scope / outcome enums).
//!
//! Moved to `yanshi-agent-spi` in P-A1.1 step 2/2; re-exported here under
//! the original path. Concrete providers live in [`crate::approval_providers`].

pub use yanshi_agent_spi::harness::approval::*;
