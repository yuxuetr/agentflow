//! AgentFlow Agents - Reusable AI Agent Applications
//!
//! This crate provides shared utilities, traits, and common components
//! for building AI agent applications using AgentFlow.

pub mod common;
pub mod traits;

// Re-export common types and utilities
pub use traits::*;
pub use common::*;

// Re-export core AgentFlow types for convenience
pub use agentflow_core::{AsyncFlow, AsyncNode, SharedState, AgentFlowError};
pub use agentflow_llm::AgentFlow;

// Common result type for agents
pub type AgentResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;