//! Subprocess-based plugin runtime (PoC for P2 #12).
//!
//! AgentFlow loads third-party `AsyncNode` implementations as standalone
//! executables that talk JSON-RPC over stdio. The chosen path is documented
//! in `docs/PLUGIN_DESIGN.md`; this module implements the host side.
//!
//! Typical usage:
//!
//! ```no_run
//! use agentflow_core::plugin::{PluginHost, PluginRegistry};
//! use std::sync::Arc;
//! use std::path::Path;
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let host = PluginHost::load(Path::new("./plugin.toml")).await?;
//! let registry = PluginRegistry::new();
//! registry.register(Arc::new(host)).await?;
//! let node = registry.create_node("echo_uppercase", "my_node").await?;
//! // `node` now implements `AsyncNode`; pass it to `Flow` like any built-in node.
//! # Ok(()) }
//! ```

pub mod dry_run;
pub mod host;
pub mod manifest;
pub mod node;
pub mod protocol;
pub mod registry;

pub use dry_run::{DryRunFailure, DryRunOutcome, run_dry_run, run_dry_run_spec};
pub use host::{CommandPreparer, NoopCommandPreparer, PluginError, PluginHost, PluginHostBuilder};
pub use manifest::{
  Capabilities, DryRunSpec, FilesystemEntry, FsAccess, ManifestError, NodeSpec, PluginManifest,
  PluginRuntime, PluginSection, SUPPORTED_PROTOCOL_VERSION,
};
pub use node::PluginNode;
pub use protocol::{ExecuteParams, ExecuteResult, InitializeParams, InitializeResult};
pub use registry::PluginRegistry;
