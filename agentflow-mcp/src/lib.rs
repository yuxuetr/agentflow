//! AgentFlow Model Context Protocol (MCP) Integration
//! 
//! This crate provides MCP client and server implementations for the AgentFlow
//! workflow system, enabling seamless integration with external tools and services.

pub mod client;
pub mod server;
pub mod transport;
pub mod error;
pub mod tools;

pub use client::*;
pub use server::*;
pub use transport::*;
pub use error::*;
pub use tools::*;

// Re-export core MCP types (when available)
// pub use rmcp::*;