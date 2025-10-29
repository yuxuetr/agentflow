//! Core JSON-RPC 2.0 and MCP protocol types
//!
//! This module defines the fundamental types for JSON-RPC 2.0 messaging
//! and MCP-specific protocol extensions.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

// ============================================================================
// JSON-RPC 2.0 Core Types
// ============================================================================

/// JSON-RPC 2.0 request
///
/// # Example
/// ```
/// use agentflow_mcp::protocol::JsonRpcRequest;
/// use serde_json::json;
///
/// let request = JsonRpcRequest {
///   jsonrpc: "2.0".to_string(),
///   id: Some(agentflow_mcp::protocol::RequestId::Number(1)),
///   method: "initialize".to_string(),
///   params: Some(json!({"protocolVersion": "2024-11-05"})),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
  /// JSON-RPC version (always "2.0")
  pub jsonrpc: String,
  /// Request identifier (optional for notifications)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<RequestId>,
  /// Method name to call
  pub method: String,
  /// Method parameters (optional)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub params: Option<Value>,
}

impl JsonRpcRequest {
  /// Create a new JSON-RPC request
  pub fn new<S: Into<String>>(id: RequestId, method: S, params: Option<Value>) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id: Some(id),
      method: method.into(),
      params,
    }
  }

  /// Create a new JSON-RPC notification (no id)
  pub fn notification<S: Into<String>>(method: S, params: Option<Value>) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id: None,
      method: method.into(),
      params,
    }
  }

  /// Check if this is a notification (no response expected)
  pub fn is_notification(&self) -> bool {
    self.id.is_none()
  }
}

/// JSON-RPC 2.0 response
///
/// # Example
/// ```
/// use agentflow_mcp::protocol::{JsonRpcResponse, RequestId};
/// use serde_json::json;
///
/// let response = JsonRpcResponse::success(
///   RequestId::Number(1),
///   json!({"status": "ok"}),
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
  /// JSON-RPC version (always "2.0")
  pub jsonrpc: String,
  /// Request identifier (matches the request)
  pub id: Option<RequestId>,
  /// Result (present on success)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result: Option<Value>,
  /// Error (present on failure)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
  /// Create a success response
  pub fn success(id: RequestId, result: Value) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id: Some(id),
      result: Some(result),
      error: None,
    }
  }

  /// Create an error response
  pub fn error(id: Option<RequestId>, error: JsonRpcError) -> Self {
    Self {
      jsonrpc: "2.0".to_string(),
      id,
      result: None,
      error: Some(error),
    }
  }

  /// Check if this response is an error
  pub fn is_error(&self) -> bool {
    self.error.is_some()
  }
}

/// JSON-RPC error object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
  /// Error code
  pub code: i32,
  /// Error message
  pub message: String,
  /// Additional error data (optional)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub data: Option<Value>,
}

impl JsonRpcError {
  /// Create a new JSON-RPC error
  pub fn new(code: i32, message: String) -> Self {
    Self {
      code,
      message,
      data: None,
    }
  }

  /// Create an error with additional data
  pub fn with_data(code: i32, message: String, data: Value) -> Self {
    Self {
      code,
      message,
      data: Some(data),
    }
  }
}

impl fmt::Display for JsonRpcError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "[{}] {}", self.code, self.message)
  }
}

/// Request identifier (can be string or number)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
  /// String identifier
  String(String),
  /// Numeric identifier
  Number(i64),
}

impl RequestId {
  /// Create a new string request ID
  pub fn new_string<S: Into<String>>(id: S) -> Self {
    Self::String(id.into())
  }

  /// Create a new numeric request ID
  pub fn new_number(id: i64) -> Self {
    Self::Number(id)
  }
}

impl fmt::Display for RequestId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::String(s) => write!(f, "\"{}\"", s),
      Self::Number(n) => write!(f, "{}", n),
    }
  }
}

impl From<String> for RequestId {
  fn from(s: String) -> Self {
    Self::String(s)
  }
}

impl From<&str> for RequestId {
  fn from(s: &str) -> Self {
    Self::String(s.to_string())
  }
}

impl From<i64> for RequestId {
  fn from(n: i64) -> Self {
    Self::Number(n)
  }
}

// ============================================================================
// MCP Protocol Types
// ============================================================================

/// MCP protocol version constant
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Implementation information (client or server)
///
/// # Example
/// ```
/// use agentflow_mcp::protocol::Implementation;
///
/// let info = Implementation {
///   name: "agentflow-mcp".to_string(),
///   version: "0.2.0".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Implementation {
  /// Implementation name
  pub name: String,
  /// Implementation version
  pub version: String,
}

impl Implementation {
  /// Create a new implementation info
  pub fn new<S1: Into<String>, S2: Into<String>>(name: S1, version: S2) -> Self {
    Self {
      name: name.into(),
      version: version.into(),
    }
  }

  /// Create default AgentFlow MCP implementation info
  pub fn agentflow() -> Self {
    Self {
      name: "agentflow-mcp".to_string(),
      version: env!("CARGO_PKG_VERSION").to_string(),
    }
  }
}

/// Initialize request parameters
///
/// Sent by client to initiate MCP session.
///
/// # Example
/// ```
/// use agentflow_mcp::protocol::{InitializeParams, Implementation, ClientCapabilities};
///
/// let params = InitializeParams {
///   protocol_version: "2024-11-05".to_string(),
///   capabilities: ClientCapabilities::default(),
///   client_info: Implementation::agentflow(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
  /// MCP protocol version
  pub protocol_version: String,
  /// Client capabilities
  pub capabilities: ClientCapabilities,
  /// Client implementation info
  pub client_info: Implementation,
}

impl InitializeParams {
  /// Create new initialize parameters
  pub fn new(capabilities: ClientCapabilities, client_info: Implementation) -> Self {
    Self {
      protocol_version: MCP_PROTOCOL_VERSION.to_string(),
      capabilities,
      client_info,
    }
  }
}

/// Initialize response result
///
/// Returned by server in response to initialize request.
///
/// # Example
/// ```
/// use agentflow_mcp::protocol::{InitializeResult, Implementation, ServerCapabilities};
///
/// let result = InitializeResult {
///   protocol_version: "2024-11-05".to_string(),
///   capabilities: ServerCapabilities::default(),
///   server_info: Implementation::agentflow(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
  /// MCP protocol version
  pub protocol_version: String,
  /// Server capabilities
  pub capabilities: ServerCapabilities,
  /// Server implementation info
  pub server_info: Implementation,
}

impl InitializeResult {
  /// Create new initialize result
  pub fn new(capabilities: ServerCapabilities, server_info: Implementation) -> Self {
    Self {
      protocol_version: MCP_PROTOCOL_VERSION.to_string(),
      capabilities,
      server_info,
    }
  }
}

/// Server capabilities
///
/// Declares what features the server supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ServerCapabilities {
  /// Tool calling support
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tools: Option<ToolsCapability>,
  /// Resource access support
  #[serde(skip_serializing_if = "Option::is_none")]
  pub resources: Option<ResourcesCapability>,
  /// Prompt template support
  #[serde(skip_serializing_if = "Option::is_none")]
  pub prompts: Option<PromptsCapability>,
  /// Sampling support (server can request LLM completions)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sampling: Option<SamplingCapability>,
}

impl ServerCapabilities {
  /// Create capabilities with tools support
  pub fn with_tools() -> Self {
    Self {
      tools: Some(ToolsCapability {}),
      ..Default::default()
    }
  }

  /// Create capabilities with resources support
  pub fn with_resources(subscribe: bool) -> Self {
    Self {
      resources: Some(ResourcesCapability {
        subscribe: Some(subscribe),
      }),
      ..Default::default()
    }
  }

  /// Check if server supports tools
  pub fn supports_tools(&self) -> bool {
    self.tools.is_some()
  }

  /// Check if server supports resources
  pub fn supports_resources(&self) -> bool {
    self.resources.is_some()
  }

  /// Check if server supports prompts
  pub fn supports_prompts(&self) -> bool {
    self.prompts.is_some()
  }
}

/// Client capabilities
///
/// Declares what features the client supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClientCapabilities {
  /// Sampling support (client can provide LLM completions)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sampling: Option<SamplingCapability>,
  /// Roots support (workspace roots)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub roots: Option<RootsCapability>,
}

impl ClientCapabilities {
  /// Create capabilities with sampling support
  pub fn with_sampling() -> Self {
    Self {
      sampling: Some(SamplingCapability {}),
      ..Default::default()
    }
  }

  /// Check if client supports sampling
  pub fn supports_sampling(&self) -> bool {
    self.sampling.is_some()
  }
}

/// Tools capability marker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolsCapability {}

/// Resources capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcesCapability {
  /// Whether resource subscriptions are supported
  #[serde(skip_serializing_if = "Option::is_none")]
  #[serde(default)]
  pub subscribe: Option<bool>,
}

/// Prompts capability marker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptsCapability {}

/// Sampling capability marker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SamplingCapability {}

/// Roots capability marker
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootsCapability {}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn test_request_id_serialization() {
    let string_id = RequestId::String("abc".to_string());
    let number_id = RequestId::Number(123);

    let string_json = serde_json::to_value(&string_id).unwrap();
    let number_json = serde_json::to_value(&number_id).unwrap();

    assert_eq!(string_json, json!("abc"));
    assert_eq!(number_json, json!(123));
  }

  #[test]
  fn test_jsonrpc_request() {
    let request = JsonRpcRequest::new(
      RequestId::Number(1),
      "test_method",
      Some(json!({"key": "value"})),
    );

    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "test_method");
    assert!(!request.is_notification());

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["method"], "test_method");
  }

  #[test]
  fn test_jsonrpc_notification() {
    let notification = JsonRpcRequest::notification("notify", None);

    assert!(notification.is_notification());
    assert_eq!(notification.id, None);

    let json = serde_json::to_value(&notification).unwrap();
    assert!(json.get("id").is_none());
  }

  #[test]
  fn test_jsonrpc_response_success() {
    let response = JsonRpcResponse::success(RequestId::Number(1), json!({"result": "ok"}));

    assert!(!response.is_error());
    assert!(response.result.is_some());
    assert!(response.error.is_none());
  }

  #[test]
  fn test_jsonrpc_response_error() {
    let error = JsonRpcError::new(-32600, "Invalid request".to_string());
    let response = JsonRpcResponse::error(Some(RequestId::Number(1)), error);

    assert!(response.is_error());
    assert!(response.result.is_none());
    assert!(response.error.is_some());
  }

  #[test]
  fn test_initialize_params() {
    let params = InitializeParams::new(
      ClientCapabilities::default(),
      Implementation::agentflow(),
    );

    assert_eq!(params.protocol_version, MCP_PROTOCOL_VERSION);
    assert_eq!(params.client_info.name, "agentflow-mcp");

    let json = serde_json::to_value(&params).unwrap();
    assert_eq!(json["protocolVersion"], MCP_PROTOCOL_VERSION);
  }

  #[test]
  fn test_server_capabilities() {
    let mut caps = ServerCapabilities::with_tools();
    assert!(caps.supports_tools());
    assert!(!caps.supports_resources());

    caps.resources = Some(ResourcesCapability { subscribe: Some(true) });
    assert!(caps.supports_resources());
  }

  #[test]
  fn test_capability_serialization() {
    let caps = ServerCapabilities {
      tools: Some(ToolsCapability {}),
      resources: Some(ResourcesCapability {
        subscribe: Some(true),
      }),
      prompts: None,
      sampling: None,
    };

    let json = serde_json::to_value(&caps).unwrap();
    assert!(json.get("tools").is_some());
    assert!(json.get("resources").is_some());
    assert!(json.get("prompts").is_none());
  }

  #[test]
  fn test_implementation_info() {
    let info = Implementation::agentflow();
    assert_eq!(info.name, "agentflow-mcp");
    assert!(!info.version.is_empty());

    let custom = Implementation::new("custom-client", "1.0.0");
    assert_eq!(custom.name, "custom-client");
    assert_eq!(custom.version, "1.0.0");
  }

  // ============================================================================
  // Property-Based Tests
  // ============================================================================

  mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
      /// Property: RequestId Number round-trips through JSON
      #[test]
      fn prop_request_id_number_roundtrip(id in any::<i64>()) {
        let req_id = RequestId::Number(id);
        let json = serde_json::to_value(&req_id).unwrap();
        let decoded: RequestId = serde_json::from_value(json).unwrap();
        prop_assert_eq!(req_id, decoded);
      }

      /// Property: RequestId String round-trips through JSON
      #[test]
      fn prop_request_id_string_roundtrip(id in "[a-zA-Z0-9-_]{1,50}") {
        let req_id = RequestId::String(id.clone());
        let json = serde_json::to_value(&req_id).unwrap();
        let decoded: RequestId = serde_json::from_value(json).unwrap();
        prop_assert_eq!(req_id, decoded);
      }

      /// Property: JSON-RPC request always has version "2.0"
      #[test]
      fn prop_request_has_jsonrpc_version(
        id in any::<i64>(),
        method in "[a-z/]{1,30}"
      ) {
        let request = JsonRpcRequest::new(RequestId::Number(id), method, None);
        prop_assert_eq!(request.jsonrpc, "2.0");
      }

      /// Property: Notifications have no ID
      #[test]
      fn prop_notification_has_no_id(method in "[a-z/]{1,30}") {
        let notification = JsonRpcRequest::notification(method, None);
        prop_assert!(notification.is_notification());
        prop_assert!(notification.id.is_none());
      }

      /// Property: Requests with ID are not notifications
      #[test]
      fn prop_request_with_id_not_notification(
        id in any::<i64>(),
        method in "[a-z/]{1,30}"
      ) {
        let request = JsonRpcRequest::new(RequestId::Number(id), method, None);
        prop_assert!(!request.is_notification());
        prop_assert!(request.id.is_some());
      }

      /// Property: Success responses have result, no error
      #[test]
      fn prop_success_response_properties(id in any::<i64>()) {
        let response = JsonRpcResponse::success(
          RequestId::Number(id),
          json!({"test": "value"})
        );

        prop_assert!(!response.is_error());
        prop_assert!(response.result.is_some());
        prop_assert!(response.error.is_none());
        prop_assert_eq!(response.jsonrpc, "2.0");
      }

      /// Property: Error responses have error, no result
      #[test]
      fn prop_error_response_properties(
        id in any::<i64>(),
        code in any::<i32>(),
        message in "[a-zA-Z0-9 ]{1,100}"
      ) {
        let error = JsonRpcError::new(code, message);
        let response = JsonRpcResponse::error(Some(RequestId::Number(id)), error);

        prop_assert!(response.is_error());
        prop_assert!(response.result.is_none());
        prop_assert!(response.error.is_some());
        prop_assert_eq!(response.jsonrpc, "2.0");
      }

      /// Property: JsonRpcRequest round-trips through JSON
      #[test]
      fn prop_request_roundtrip(
        id in any::<i64>(),
        method in "[a-z/]{1,30}"
      ) {
        let request = JsonRpcRequest::new(RequestId::Number(id), method, None);
        let json = serde_json::to_value(&request).unwrap();
        let decoded: JsonRpcRequest = serde_json::from_value(json).unwrap();
        prop_assert_eq!(request, decoded);
      }

      /// Property: JsonRpcResponse round-trips through JSON
      #[test]
      fn prop_response_roundtrip(id in any::<i64>()) {
        let response = JsonRpcResponse::success(
          RequestId::Number(id),
          json!({"test": 123})
        );
        let json = serde_json::to_value(&response).unwrap();
        let decoded: JsonRpcResponse = serde_json::from_value(json).unwrap();
        prop_assert_eq!(response, decoded);
      }

      /// Property: Method names are preserved exactly
      #[test]
      fn prop_method_name_preserved(
        id in any::<i64>(),
        method in "[a-zA-Z0-9/_.-]{1,50}"
      ) {
        let request = JsonRpcRequest::new(RequestId::Number(id), method.clone(), None);
        prop_assert_eq!(&request.method, &method);

        // Round-trip through JSON
        let json = serde_json::to_value(&request).unwrap();
        let decoded: JsonRpcRequest = serde_json::from_value(json).unwrap();
        prop_assert_eq!(&decoded.method, &method);
      }
    }
  }
}
