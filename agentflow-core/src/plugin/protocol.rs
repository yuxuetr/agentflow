//! JSON-RPC 2.0 wire types for the AgentFlow plugin protocol.
//!
//! Plugins and the host exchange newline-delimited JSON messages over
//! stdin/stdout. Each line is one JSON object: a `JsonRpcRequest`, a
//! `JsonRpcResponse`, or a `JsonRpcNotification`. See `docs/PLUGIN_DESIGN.md`
//! §6.3 for the protocol summary.

use crate::value::FlowValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const JSONRPC_VERSION: &str = "2.0";

/// Method names recognized by the host or by a conforming plugin.
pub mod methods {
  /// host → plugin: handshake; expects an `InitializeResult` reply.
  pub const PLUGIN_INITIALIZE: &str = "plugin/initialize";
  /// host → plugin: invoke a node; expects an `ExecuteResult` reply.
  pub const NODE_EXECUTE: &str = "node/execute";
  /// host → plugin: graceful shutdown request.
  pub const PLUGIN_SHUTDOWN: &str = "plugin/shutdown";
  /// plugin → host: structured log notification (no response expected).
  pub const PLUGIN_LOG: &str = "plugin/log";
}

/// JSON-RPC error codes used by the protocol.
pub mod error_codes {
  pub const PARSE_ERROR: i32 = -32700;
  pub const INVALID_REQUEST: i32 = -32600;
  pub const METHOD_NOT_FOUND: i32 = -32601;
  pub const INVALID_PARAMS: i32 = -32602;
  pub const INTERNAL_ERROR: i32 = -32603;
  /// Any application-level error from the plugin's node implementation.
  pub const PLUGIN_EXECUTION_ERROR: i32 = -32000;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
  pub jsonrpc: String,
  pub id: u64,
  pub method: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
  pub jsonrpc: String,
  pub id: u64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub result: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
  pub jsonrpc: String,
  pub method: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
  pub code: i32,
  pub message: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub data: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeParams {
  pub host_version: String,
  pub protocol_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
  pub plugin_name: String,
  pub plugin_version: String,
  #[serde(default)]
  pub nodes: Vec<NodeDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDescriptor {
  #[serde(rename = "type")]
  pub node_type: String,
  #[serde(default)]
  pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteParams {
  pub node_type: String,
  pub inputs: HashMap<String, FlowValue>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub run_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteResult {
  pub outputs: HashMap<String, FlowValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogParams {
  pub level: String,
  pub message: String,
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn execute_params_round_trip_through_flow_value() {
    let mut inputs = HashMap::new();
    inputs.insert(
      "text".to_string(),
      FlowValue::Json(serde_json::Value::String("hi".into())),
    );
    let params = ExecuteParams {
      node_type: "echo".into(),
      inputs,
      run_id: None,
    };
    let encoded = serde_json::to_value(&params).unwrap();
    assert_eq!(
      encoded,
      json!({
        "node_type": "echo",
        "inputs": {
          "text": { "type": "json", "value": "hi" }
        }
      })
    );
    let decoded: ExecuteParams = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.node_type, "echo");
    assert!(decoded.inputs.contains_key("text"));
  }
}
