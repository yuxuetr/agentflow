//! AgentFlow Nodes - Built-in Node Implementations
//!
//! This crate provides ready-to-use node implementations for AgentFlow workflows,
//! supporting both code-first and configuration-first approaches.

pub mod common;
pub mod error;
pub mod nodes;

// Re-export core types for convenience
pub use agentflow_core::{AgentFlowError, AsyncNode, Result};

// Q3.8.3: the previously-exported `factory_traits` module
// (`NodeFactory` / `NodeRegistry` / `NodeConfig` / `ResolvedNodeConfig`)
// was an unused parallel API — `agentflow-cli` builds workflows via
// `executor::build_flow_from_definition` which returns `GraphNode`,
// not via the trait's `Box<dyn AsyncNode>` shape. Zero workspace
// implementors meant the trait was a misleading attractor for new
// contributors. Removed in 2026-05-26. If you need a registry of
// node constructors, mirror the `agentflow-skills::SkillBuilder`
// pattern or the CLI's existing executor module.

// Error types
pub use error::{NodeError, NodeResult};
