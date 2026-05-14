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
pub mod capability;
pub mod error;
pub mod policy;
pub mod registry;
pub mod sandbox;
pub mod security_profile;
pub mod tool;

pub use capability::{Capability, CapabilityDecisionEntry, EffectiveCapabilities, GrantSource};
pub use error::ToolError;
pub use policy::{ToolPolicy, ToolPolicyDecision};
pub use registry::ToolRegistry;
pub use sandbox::{SandboxEnforcement, SandboxPolicy, SandboxStatus};
pub use security_profile::{
  AuthDefaults, CorsDefaults, CorsMode, MarketplaceInstallDefaults, PluginExecutionDefaults,
  RequestLimitDefaults, SECURITY_PROFILE_ENV, SandboxingDefaults, SecurityProfile,
  SecurityProfileDefaults, SecurityProfileError, ToolPermissionDefaults,
};
pub use tool::{
  Tool, ToolCall, ToolDefinition, ToolIdempotency, ToolMetadata, ToolOutput, ToolOutputPart,
  ToolPermission, ToolPermissionSet, ToolSource,
};
