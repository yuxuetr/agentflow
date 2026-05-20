//! AgentFlow Model Context Protocol (MCP) Integration
//!
//! This crate provides production-ready MCP client and server implementations for the AgentFlow
//! workflow system, enabling seamless integration with external tools and services.
//!
//! # Features
//!
//! - **Client Implementation** - Full MCP client with fluent API
//! - **Tool Calling** - Discover and execute tools on MCP servers
//! - **Resource Access** - Read and subscribe to server resources
//! - **Prompt Templates** - Retrieve and use prompt templates
//! - **Automatic Retry** - Exponential backoff for transient failures
//! - **Type Safety** - Complete protocol types with serde serialization
//! - **Error Handling** - Rich error context with source tracking
//!
//! # Quick Start
//!
//! ```no_run
//! use agentflow_mcp::client::ClientBuilder;
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create and connect client
//! let mut client = ClientBuilder::new()
//!   .with_stdio(vec![
//!     "npx".to_string(),
//!     "-y".to_string(),
//!     "@modelcontextprotocol/server-everything".to_string(),
//!   ])
//!   .build()
//!   .await?;
//!
//! client.connect().await?;
//!
//! // List and call tools
//! let tools = client.list_tools().await?;
//! let result = client.call_tool("add", json!({"a": 5, "b": 3})).await?;
//!
//! // Read resources
//! let resources = client.list_resources().await?;
//!
//! client.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! The crate is organized into the following modules:
//!
//! - `client` - Production-ready MCP client implementation
//! - `error` - Error types with context tracking
//! - `protocol` - JSON-RPC and MCP protocol types
//! - `transport` - Transport layer (stdio; HTTP transport types are reserved)
//! - `server` - MCP server implementation. **Beta** (P10.5.2): the
//!   closed method set + wire shapes are pinned by fixture tests in
//!   `agentflow-mcp/tests/server_contracts.rs`. See
//!   `docs/STABILITY.md` for the full Beta promise.

pub mod client;
pub mod error;
pub mod protocol;
pub mod server;
pub mod tools;
pub mod transport;

/// **Deprecated** alias for [`transport`]. The module was previously
/// named `transport_new` to disambiguate from an older transport
/// implementation that has since been removed (P10.5.1). External
/// callers should migrate to `agentflow_mcp::transport`; this
/// re-export will be deleted in a future release.
#[deprecated(
  since = "0.3.0",
  note = "renamed to `agentflow_mcp::transport`; update your imports"
)]
pub use transport as transport_new;

// Re-export main types for convenience
pub use error::{MCPError, MCPResult};

// Client types are available under `client::` module
// Example: use agentflow_mcp::client::ClientBuilder;

// Protocol types are available under `protocol::` module
// Example: use agentflow_mcp::protocol::types::*;

// Transport types are available under `transport::` module
// Example: use agentflow_mcp::transport::StdioTransport;

#[cfg(test)]
mod compat_tests {
  /// P10.5.1: the deprecated `transport_new` alias must still
  /// resolve so 3rd-party callers that imported via the old name
  /// keep compiling (with a deprecation warning) through the
  /// transition window. Suppressing the warning here is intentional
  /// — we're testing the deprecated path on purpose.
  #[test]
  #[allow(deprecated)]
  fn transport_new_alias_still_resolves_to_transport_module() {
    // If the alias resolves, this expression compiles. The actual
    // type identity is the assertion — both paths must point at
    // the same `Transport` trait.
    fn _assert_same_type<T: ?Sized>(_: &T) {}
    fn assert_alias_compat<T: crate::transport::Transport>(t: &T) {
      let t_via_alias: &dyn crate::transport_new::Transport = t;
      _assert_same_type(t_via_alias);
    }
    // Pinning a concrete impl: MockTransport implements the trait
    // both via its real path and via the alias. The function only
    // type-checks when both paths refer to the same trait.
    let mock = crate::transport::MockTransport::new();
    assert_alias_compat(&mock);
  }
}
