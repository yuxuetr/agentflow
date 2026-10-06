//! # yanshi-tools
//!
//! Built-in tool implementations for Yanshi agents (`ShellTool`,
//! `FileTool`, `HttpTool`, `ScriptTool`, `CodeExecTool`) and the concrete
//! OS-level sandbox backends they run under.
//!
//! T3.3: the `Tool` contract itself — the trait, `ToolRegistry`,
//! `ToolMetadata`, `Capability`, `ToolPolicy`, `SecurityProfile`, and the
//! `SandboxBackend` trait + its DTOs — lives in `yanshi-tool` (a
//! dependency-free L0 contract crate) and is re-exported here in full, so
//! every existing `use yanshi_tools::{Tool, ToolRegistry, ...}` call
//! site is unaffected. A runtime that only needs the contract (no concrete
//! tools) should depend on `yanshi-tool` directly instead of this crate.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use yanshi_tools::{ToolRegistry, SandboxPolicy, ToolError};
//! use yanshi_tools::builtin::{ShellTool, FileTool, HttpTool};
//!
//! # fn main() -> Result<(), ToolError> {
//! let policy = Arc::new(SandboxPolicy::permissive());
//! let mut registry = ToolRegistry::new();
//! registry.register(Arc::new(ShellTool::new(policy.clone())));
//! registry.register(Arc::new(FileTool::new(policy.clone())));
//! registry.register(Arc::new(HttpTool::new(policy.clone())?));
//!
//! println!("{}", registry.prompt_tools_description());
//! # Ok(())
//! # }
//! ```

pub mod builtin;
pub mod defaults;
pub mod manifest;
pub mod sandbox;

pub use defaults::default_governed_registry;
pub use manifest::{
  BuiltinToolKind, ToolManifest, ToolManifestEntry, build_registry_from_manifest,
};

pub use sandbox::{SandboxEnforcement, SandboxPolicy, SandboxStatus};
pub use yanshi_tool::{
  AuthDefaults, CorsDefaults, CorsMode, MarketplaceInstallDefaults, PluginExecutionDefaults,
  RequestLimitDefaults, SECURITY_PROFILE_ENV, SandboxingDefaults, SecurityProfile,
  SecurityProfileDefaults, SecurityProfileError, ToolPermissionDefaults, WorkerAdmissionDefaults,
};
pub use yanshi_tool::{Capability, CapabilityDecisionEntry, EffectiveCapabilities, GrantSource};
pub use yanshi_tool::{
  PluginEvaluationInput, PluginNetworkPolicy, PluginPolicy, PluginPolicyDecision,
};
pub use yanshi_tool::{
  Tool, ToolCall, ToolDefinition, ToolIdempotency, ToolMetadata, ToolOutput, ToolOutputPart,
  ToolPermission, ToolPermissionSet, ToolSource,
};
pub use yanshi_tool::{ToolError, ToolPolicy, ToolPolicyDecision, ToolRegistry};
