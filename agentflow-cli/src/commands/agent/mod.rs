//! `agentflow agent` namespace.
//!
//! Currently single-subcommand: `replay --diff <baseline> <current>` (P10.8.1).
//! The namespace is reserved for ReAct-native agent surfaces. It's separate
//! from `harness` (workspace-aware long-lived sessions, HarnessEvent wire
//! shape) and from `trace` (workflow-scoped ExecutionTrace JSON) — see
//! `docs/HARNESS_MODE.md` for the boundary.

pub mod replay;
