//! MCP transport layer implementations
//!
//! This module provides transport implementations for communicating with MCP servers
//! using various mechanisms (stdio, HTTP, etc.).
//!
//! # Transports
//!
//! - **Stdio**: Communicates with local processes via stdin/stdout
//! - **HTTP**: Communicates with remote servers via HTTP (future)
//! - **HTTP+SSE**: HTTP with Server-Sent Events for bidirectional communication (future)
//!
//! # Example
//!
//! ```no_run
//! use agentflow_mcp::transport_new::{StdioTransport, Transport};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut transport = StdioTransport::new(vec![
//!   "npx".to_string(),
//!   "-y".to_string(),
//!   "@modelcontextprotocol/server-everything".to_string(),
//! ]);
//!
//! transport.connect().await?;
//! # Ok(())
//! # }
//! ```

pub mod stdio;
pub mod traits;

// Re-export commonly used types
pub use stdio::StdioTransport;
pub use traits::{Transport, TransportConfig, TransportType};
