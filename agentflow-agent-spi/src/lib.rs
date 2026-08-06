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
//!
//! The [`capability`] module holds the [`Capability`](capability::Capability)
//! contract (RFC §2): a packaged ability that `lower()`s to tools + context.
//! `agentflow-skills` implements it for a Skill so a surface can merge any
//! capability into a runtime's registry + prompt uniformly.
//!
//! The [`harness`] module holds the Harness governance contracts
//! (`HarnessEvent`, `ApprovalRequest` / `ApprovalDecision`, the hook / approval
//! / context-provider / sink traits), moved here in P-A1.1 step 2/2 so the
//! operations crates depend on the contract rather than the `agentflow-harness`
//! runtime. `agentflow-harness` re-exports them under their original paths.

pub mod aggregation;
pub mod capability;
pub mod delegation;
pub mod harness;
pub mod runtime;
pub mod schema_validation;
pub mod turn;

pub use aggregation::{AggregationReport, AnswerGroup, SubagentAnswer, aggregate_answers};
pub use capability::{Capability, CapabilityError, Lowered};
pub use delegation::{DelegationSpec, SchemaValidation, validate_output};
pub use harness::*;
pub use runtime::*;
pub use schema_validation::{validate_json_against_schema, validate_json_str_against_schema};
pub use turn::*;
