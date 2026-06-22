//! The `Capability` contract (RFC §2 — the second load-bearing trait).
//!
//! A **capability** is a packaged unit (persona + tools + knowledge + config)
//! that **lowers** to *tools + context* at the runtime boundary. A Skill is the
//! canonical capability; `agentflow-skills` implements [`Capability`] for it.
//!
//! `lower()` returns an **owned** [`Lowered`] (not `&mut Assembly`) so
//! capabilities compose by flatten and are trivially testable: a surface merges
//! every `Lowered` into one tool registry + context bundle and hands it to a
//! runtime. The runtime forever sees only **Tool + Context + AgentRuntime** —
//! it never knows a capability existed.
//!
//! Distinct from the OS-sandbox `agentflow_tools::Capability` enum (a process
//! permission like `Exec` / `Net`); this `Capability` is the higher-level
//! "packaged ability" concept. The two never appear in the same position.

use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::harness::context::ContextItem;
use agentflow_tools::Tool;

/// The result of lowering a [`Capability`]: the tools it contributes to the
/// registry plus the context fragments it injects into the prompt.
///
/// The `context` items are the kernel's existing [`ContextItem`] (the RFC calls
/// this a "ContextFragment") so a lowered capability slots straight into the
/// harness/runtime prompt-budgeting machinery — each item already carries a
/// priority and token estimate.
#[derive(Default)]
pub struct Lowered {
  /// Tools the capability contributes, to be merged into the runtime registry.
  pub tools: Vec<Arc<dyn Tool>>,
  /// Context fragments the capability injects, to be merged under a budget.
  pub context: Vec<ContextItem>,
}

impl Lowered {
  /// An empty lowering (no tools, no context).
  pub fn new() -> Self {
    Self::default()
  }

  /// Builder: add one tool.
  pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
    self.tools.push(tool);
    self
  }

  /// Builder: add one context fragment.
  pub fn with_context(mut self, item: ContextItem) -> Self {
    self.context.push(item);
    self
  }

  /// Fold another lowering into this one — how capabilities compose by flatten.
  pub fn merge(&mut self, other: Lowered) {
    self.tools.extend(other.tools);
    self.context.extend(other.context);
  }
}

/// Errors surfaced while lowering a [`Capability`].
///
/// `#[non_exhaustive]` per the RFC §2 modeling rule: callers match via
/// `Display` / `?` / a `_` arm, so new variants are not breaking.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CapabilityError {
  /// The capability could not assemble its tools / context (bad manifest,
  /// unreadable knowledge file, MCP wiring failure, …).
  #[error("Capability lowering failed: {0}")]
  Lower(String),
}

/// A packaged ability (persona + tools + knowledge + config) that lowers to
/// tools + context at the runtime boundary (RFC §2).
///
/// Object-safe so a surface can hold a heterogeneous `Vec<Box<dyn Capability>>`,
/// lower each, and merge the results before handing a runtime its registry +
/// context.
#[async_trait]
pub trait Capability: Send + Sync {
  /// Lower this capability into its constituent tools + context fragments.
  async fn lower(&self) -> Result<Lowered, CapabilityError>;
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::harness::context::ContextPriority;

  fn fragment(source: &str) -> ContextItem {
    ContextItem {
      source: source.to_string(),
      priority: ContextPriority::Normal,
      token_estimate: 1,
      content: "x".to_string(),
      metadata: serde_json::Value::Null,
    }
  }

  #[test]
  fn merge_flattens_tools_and_context() {
    let mut a = Lowered::new().with_context(fragment("a"));
    let b = Lowered::new().with_context(fragment("b"));
    a.merge(b);
    assert_eq!(a.context.len(), 2);
    assert_eq!(a.context[0].source, "a");
    assert_eq!(a.context[1].source, "b");
  }

  // A capability must stay object-safe — surfaces hold `Box<dyn Capability>`.
  #[allow(dead_code)]
  fn assert_object_safe(_: &dyn Capability) {}
}
