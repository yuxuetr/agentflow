//! AgentFlow Model Context Protocol (MCP) Integration
//!
//! This crate provides MCP client and server implementations for the AgentFlow
//! workflow system, enabling seamless integration with external tools and services.
//!
//! # Example
//!
//! ```no_run
//! use agentflow_mcp::{MCPClient, protocol::RequestId};
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a client
//! let mut client = MCPClient::stdio(vec!["npx".into(), "-y".into(), "@modelcontextprotocol/server-everything".into()]);
//!
//! // Connect and initialize
//! client.connect().await?;
//!
//! // List available tools
//! let tools = client.list_tools().await?;
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod error;
pub mod protocol;
pub mod server;
pub mod tools;
pub mod transport;
pub mod transport_new; // New refactored transport layer

pub use client::*;
pub use error::*;
pub use protocol::*;
pub use server::*;
pub use tools::*;
pub use transport::*;

// Note: transport_new is not re-exported at top level to avoid conflicts
// Use `transport_new::StdioTransport` explicitly until migration is complete
