//! MCP protocol implementation
//!
//! This module provides the core Model Context Protocol types and utilities,
//! including JSON-RPC 2.0 messaging and MCP-specific protocol extensions.

pub mod modern;
pub mod traceparent;
pub mod types;

// Re-export commonly used types
pub use modern::{
  DiscoverResult, HEADER_MISMATCH_ERROR_CODE, INPUT_REQUIRED_RESULT_FIELD, INPUT_RESPONSES_FIELD,
  KNOWN_PROTOCOL_VERSIONS, MCP_PROTOCOL_VERSION_2024_11_05, MCP_PROTOCOL_VERSION_2025_03_26,
  MCP_PROTOCOL_VERSION_2025_11_25, MCP_PROTOCOL_VERSION_2026_07_28,
  MODERN_CLIENT_CAPABILITIES_META_FIELD, MODERN_CLIENT_INFO_META_FIELD,
  MODERN_PROTOCOL_VERSION_META_FIELD, McpEra, SERVER_DISCOVER_METHOD,
  UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE, UnsupportedProtocolVersionErrorData,
  as_unsupported_protocol_version_error, attach_input_responses, inject_modern_meta_into_request,
  is_input_required_result, is_recognized_modern_error, server_discover_request,
};
pub use traceparent::{
  META_FIELD, TRACEPARENT_FIELD, extract_traceparent_from_request, inject_traceparent_into_request,
  inject_traceparent_into_request_with,
};
pub use types::{
  ClientCapabilities, Implementation, InitializeParams, InitializeResult, JsonRpcError,
  JsonRpcRequest, JsonRpcResponse, MCP_PROTOCOL_VERSION, PromptsCapability, RequestId,
  ResourcesCapability, RootsCapability, SamplingCapability, ServerCapabilities, ToolsCapability,
};
