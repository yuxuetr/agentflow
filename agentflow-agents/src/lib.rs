//! AgentFlow Agents - Reusable AI Agent Applications
//!
//! This crate provides shared utilities, traits, and common components
//! for building AI agent applications using AgentFlow.
//!
//! ## ReAct Agent (Phase 1)
//! Use [`react::ReActAgent`] for autonomous Thought/Action/Observation loops.

pub mod common;
pub mod nodes;
pub mod react;
pub mod reflection;
pub mod runtime;
pub mod supervisor;
pub mod tools;
pub mod traits;

// Re-export common types and utilities
pub use common::*;
pub use traits::*;

// Re-export core AgentFlow types for convenience
pub use agentflow_core::{AgentFlowError, AsyncNode};
pub use agentflow_llm::AgentFlow;

// Re-export MCP utilities
pub use agentflow_mcp::client::MCPClient;
pub use agentflow_mcp::tools::{ToolCall, ToolRegistry as McpToolRegistry};

// Re-export new Phase-1 building blocks
pub use agentflow_memory;
pub use agentflow_tools;

// Re-export M3 multi-agent building blocks
pub use nodes::AgentNode;
pub use reflection::{
  FailureReflection, FinalReflection, NoOpReflection, Reflection, ReflectionContext,
  ReflectionError, ReflectionStrategy, ReflectionTrigger,
};
pub use runtime::{
  AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentRuntimeError, AgentStep,
  AgentStepKind, AgentStopReason, RuntimeLimits,
};
pub use supervisor::{Supervisor, SupervisorBuilder};
pub use tools::AgentTool;

// Common result type for agents
pub type AgentResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
