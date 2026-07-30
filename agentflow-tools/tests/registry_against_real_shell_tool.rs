//! T3.3: `ToolRegistry`'s capability-narrowing and sandbox-status-surfacing
//! behavior, exercised against a real `ShellTool` rather than a hand-rolled
//! test double.
//!
//! These moved out of `agentflow-tool` (the contract crate `ToolRegistry`
//! lives in) because a dev-dependency from `agentflow-tool` back onto
//! `agentflow-tools` (which depends on `agentflow-tool` for real) produced
//! two distinct compilations of `agentflow-tool` in the graph — `ShellTool:
//! Tool` then failed to resolve against the contract crate's own copy of
//! the `Tool` trait. No such cycle exists here: this crate already depends
//! on `agentflow-tool` transitively and re-exports it, so `ToolRegistry`
//! and `ShellTool` are guaranteed to agree on the same `Tool` trait.

use std::sync::Arc;

use agentflow_tools::builtin::ShellTool;
use agentflow_tools::sandbox::{SandboxEnforcement, SandboxPolicy};
use agentflow_tools::{Capability, ToolRegistry};

#[test]
fn narrowed_capability_layer_denies_a_tool_requiring_an_ungranted_capability() {
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(ShellTool::new(Arc::new(
    SandboxPolicy::permissive(),
  ))));

  // Grant only Net — ShellTool requires Exec, so it must be denied
  // through the SAME EffectiveCapabilities::resolve merge a Skill's own
  // tool_permission_allowlist already goes through.
  let narrowed = registry.narrowed(None, Some(vec![Capability::Net]));
  let effective = narrowed.evaluate_capabilities("shell").unwrap();
  assert!(!effective.allowed);
  assert!(effective.denied.contains(&Capability::Exec));
}

#[test]
fn evaluate_capabilities_carries_sandbox_status_for_subprocess_tools() {
  let policy = Arc::new(SandboxPolicy::permissive());
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(ShellTool::new(policy)));

  let effective = registry.evaluate_capabilities("shell").unwrap();
  let status = effective
    .sandbox
    .expect("shell tool must surface a sandbox status snapshot");
  // Default ShellTool uses the no-op backend; this is the silent-fall-through
  // case the visibility task is meant to surface. It must be Disabled, not
  // missing.
  assert_eq!(status.backend, "noop");
  assert_eq!(status.enforcement, SandboxEnforcement::Disabled);
}
