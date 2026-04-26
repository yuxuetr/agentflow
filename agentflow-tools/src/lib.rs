//! # agentflow-tools
//!
//! Unified tool abstraction and built-in tool implementations for AgentFlow agents.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use agentflow_tools::{ToolRegistry, SandboxPolicy};
//! use agentflow_tools::builtin::{ShellTool, FileTool, HttpTool};
//!
//! let policy = Arc::new(SandboxPolicy::permissive());
//! let mut registry = ToolRegistry::new();
//! registry.register(Arc::new(ShellTool::new(policy.clone())));
//! registry.register(Arc::new(FileTool::new(policy.clone())));
//! registry.register(Arc::new(HttpTool::new(policy.clone())));
//!
//! println!("{}", registry.prompt_tools_description());
//! ```

pub mod builtin;
pub mod error;
pub mod registry;
pub mod sandbox;
pub mod tool;

pub use error::ToolError;
pub use registry::ToolRegistry;
pub use sandbox::SandboxPolicy;
pub use tool::{Tool, ToolCall, ToolDefinition, ToolOutput, ToolOutputPart};
