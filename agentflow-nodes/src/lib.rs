//! AgentFlow Nodes - Built-in Node Implementations
//!
//! This crate provides ready-to-use node implementations for AgentFlow workflows,
//! supporting both code-first and configuration-first approaches.

pub mod error;
pub mod nodes;

// Factory traits for configuration support
pub mod factory_traits;

// Optional factory implementations for configuration-first workflows
#[cfg(feature = "factories")]
pub mod factories;

// Re-export core types for convenience
pub use agentflow_core::{AgentFlowError, AsyncNode, Result};

// Text-based AI model nodes
#[cfg(feature = "llm")]
pub use nodes::llm::LlmNode;

// Factory trait exports
pub use factory_traits::{NodeConfig, NodeFactory, NodeRegistry, ResolvedNodeConfig};

// Factory implementations for configuration support
#[cfg(feature = "factories")]
pub use factories::*;

// Error types
pub use error::{NodeError, NodeResult};
