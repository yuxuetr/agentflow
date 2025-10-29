//! MCP client implementation
//!
//! This module provides a production-ready MCP client with the following features:
//!
//! - **Fluent builder API** - Easy client construction with `ClientBuilder`
//! - **Session management** - Automatic initialization and connection handling
//! - **Tool calling** - List and call tools on MCP servers
//! - **Resource access** - Read and subscribe to server resources
//! - **Prompt templates** - Retrieve prompt templates with argument substitution
//! - **Automatic retry** - Exponential backoff for transient failures
//!
//! # Quick Start
//!
//! ```no_run
//! use agentflow_mcp::client::ClientBuilder;
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create client
//! let mut client = ClientBuilder::new()
//!   .with_stdio(vec![
//!     "npx".to_string(),
//!     "-y".to_string(),
//!     "@modelcontextprotocol/server-everything".to_string(),
//!   ])
//!   .with_max_retries(3)
//!   .build()
//!   .await?;
//!
//! // Connect and initialize
//! client.connect().await?;
//!
//! // List and call tools
//! let tools = client.list_tools().await?;
//! println!("Available tools: {}", tools.len());
//!
//! let result = client.call_tool("add", json!({"a": 5, "b": 3})).await?;
//! if let Some(text) = result.first_text() {
//!   println!("Result: {}", text);
//! }
//!
//! // List and read resources
//! let resources = client.list_resources().await?;
//! if let Some(resource) = resources.first() {
//!   let content = client.read_resource(&resource.uri).await?;
//!   println!("Resource content: {:?}", content.first_content());
//! }
//!
//! // Disconnect
//! client.disconnect().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! The client module is organized into the following submodules:
//!
//! - `builder` - Fluent API for client construction
//! - `session` - Session lifecycle and state management
//! - `tools` - Tool discovery and calling
//! - `resources` - Resource access and subscriptions
//! - `prompts` - Prompt template retrieval
//! - `retry` - Retry logic with exponential backoff

mod builder;
mod prompts;
mod resources;
pub mod retry; // Public for direct access to retry utilities
mod session;
mod tools;

// Re-export main types
pub use builder::ClientBuilder;
pub use prompts::{
  GetPromptResult, Prompt, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole,
};
pub use resources::{ReadResourceResult, Resource, ResourceContent};
pub use retry::{retry_with_backoff, RetryConfig};
pub use session::{MCPClient, SessionState};
pub use tools::{CallToolResult, Content, Tool};

// Note: Internal types (ClientConfig, etc.) are kept private
