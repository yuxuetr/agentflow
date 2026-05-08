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

pub mod host;
pub mod manifest;
pub mod node;
pub mod protocol;
pub mod registry;

pub use host::{PluginError, PluginHost};
pub use manifest::{
  Capabilities, ManifestError, NodeSpec, PluginManifest, PluginRuntime, PluginSection,
  SUPPORTED_PROTOCOL_VERSION,
};
pub use node::PluginNode;
pub use protocol::{ExecuteParams, ExecuteResult, InitializeParams, InitializeResult};
pub use registry::PluginRegistry;
