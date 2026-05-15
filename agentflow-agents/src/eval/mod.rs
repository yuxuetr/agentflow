//! Agent eval harness.
//!
//! See `docs/AGENT_EVAL_FORMAT.md` for the on-disk format, assertion DSL,
//! and JSON report envelope. This module is the v1 implementation.
//!
//! Layout:
//!
//! - [`dataset`] — `Dataset` / `EvalCase` / `EvalCaseDefaults` types,
//!   JSONL+TOML loader, validation. Pure data; no agent execution.
//! - [`assertion`] — closed enum of six assertion variants
//!   (`contains` / `regex` / `tool_called` / `tool_not_called` /
//!   `step_count_below` / `final_answer_matches_skill`) and the
//!   evaluation context they operate on.
//! - [`runner`] — `EvalRunner` (lands under P4.4 slice 2). Walks the
//!   dataset, executes a [`crate::AgentRuntime`] per case, evaluates
//!   assertions against the resulting [`crate::AgentRunResult`], emits
//!   a per-case [`runner::CaseReport`] and an aggregate
//!   [`runner::EvalReport`].

pub mod assertion;
pub mod dataset;
pub mod pricing;
pub mod runner;

pub use assertion::{
  Assertion, AssertionContext, AssertionInScope, AssertionOutcome, AssertionTarget, SkillValidator,
};
pub use dataset::{Dataset, DatasetManifest, EvalCase, EvalCaseDefaults, EvalError, RawEvalCase};
pub use pricing::{ModelPricing, PricingError, PricingTable};
pub use runner::{
  AgentRuntimeFactory, CaseReport, CaseStatus, EvalReport, EvalRunner, EvalRunnerError, EvalSummary,
};
