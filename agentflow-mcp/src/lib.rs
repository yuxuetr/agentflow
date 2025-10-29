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
//! - `transport_new` - Transport layer (stdio, http)
//! - `server` - MCP server implementation (experimental)

pub mod client;
pub mod client_old; // Legacy client, kept for reference
pub mod error;
pub mod protocol;
pub mod server;
pub mod tools;
pub mod transport;
pub mod transport_new;

// Re-export main types for convenience
pub use error::{MCPError, MCPResult};

// Client types are available under `client::` module
// Example: use agentflow_mcp::client::ClientBuilder;

// Protocol types are available under `protocol::` module
// Example: use agentflow_mcp::protocol::types::*;

// Transport types are available under `transport_new::` module
// Example: use agentflow_mcp::transport_new::StdioTransport;
