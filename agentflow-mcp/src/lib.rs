//! AgentFlow Model Context Protocol (MCP) Integration
//!
//! This crate provides MCP client and server implementations for the AgentFlow
//! workflow system, enabling seamless integration with external tools and services.

pub mod client;
pub mod error;
pub mod server;
pub mod tools;
pub mod transport;

pub use client::*;
pub use error::*;
pub use server::*;
pub use tools::*;
pub use transport::*;

// Re-export core MCP types (when available)
// pub use rmcp::*;
