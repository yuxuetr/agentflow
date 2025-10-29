//! MCP protocol implementation
//!
//! This module provides the core Model Context Protocol types and utilities,
//! including JSON-RPC 2.0 messaging and MCP-specific protocol extensions.

pub mod types;

// Re-export commonly used types
pub use types::{
  ClientCapabilities, Implementation, InitializeParams, InitializeResult, JsonRpcError,
  JsonRpcRequest, JsonRpcResponse, PromptsCapability, RequestId, ResourcesCapability,
  RootsCapability, SamplingCapability, ServerCapabilities, ToolsCapability, MCP_PROTOCOL_VERSION,
};
