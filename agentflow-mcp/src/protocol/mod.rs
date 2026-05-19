//! MCP protocol implementation
//!
//! This module provides the core Model Context Protocol types and utilities,
//! including JSON-RPC 2.0 messaging and MCP-specific protocol extensions.

pub mod traceparent;
pub mod types;

// Re-export commonly used types
pub use traceparent::{
  META_FIELD, TRACEPARENT_FIELD, extract_traceparent_from_request,
  inject_traceparent_into_request, inject_traceparent_into_request_with,
};
pub use types::{
  ClientCapabilities, Implementation, InitializeParams, InitializeResult, JsonRpcError,
  JsonRpcRequest, JsonRpcResponse, MCP_PROTOCOL_VERSION, PromptsCapability, RequestId,
  ResourcesCapability, RootsCapability, SamplingCapability, ServerCapabilities, ToolsCapability,
};
