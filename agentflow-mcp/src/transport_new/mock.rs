//! Mock transport for testing
//!
//! This module provides a mock transport that simulates MCP server responses
//! for testing purposes without requiring a real server.

use crate::error::{MCPError, MCPResult};
use crate::transport_new::{Transport, TransportType};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Mock transport for testing
///
/// This transport allows you to pre-configure responses that will be returned
/// when the client sends messages.
///
/// # Example
///
/// ```
/// use agentflow_mcp::transport_new::MockTransport;
/// use serde_json::json;
///
/// let mut transport = MockTransport::new();
///
/// // Configure response for initialize request
/// transport.add_response(json!({
///   "jsonrpc": "2.0",
///   "id": 1,
///   "result": {
///     "protocolVersion": "2024-11-05",
///     "capabilities": {},
///     "serverInfo": {
///       "name": "test-server",
///       "version": "1.0.0"
///     }
///   }
/// }));
/// ```
#[derive(Debug, Clone)]
pub struct MockTransport {
  /// Queue of responses to return
  responses: Arc<Mutex<VecDeque<Value>>>,
  /// Whether the transport is connected
  connected: Arc<Mutex<bool>>,
  /// Messages that were sent
  sent_messages: Arc<Mutex<Vec<Value>>>,
}

impl MockTransport {
  /// Create a new mock transport
  pub fn new() -> Self {
    Self {
      responses: Arc::new(Mutex::new(VecDeque::new())),
      connected: Arc::new(Mutex::new(false)),
      sent_messages: Arc::new(Mutex::new(Vec::new())),
    }
  }

  /// Add a response that will be returned for the next send_message call
  pub fn add_response(&mut self, response: Value) {
    self.responses.lock().unwrap().push_back(response);
  }

  /// Add multiple responses
  pub fn add_responses(&mut self, responses: Vec<Value>) {
    let mut queue = self.responses.lock().unwrap();
    for response in responses {
      queue.push_back(response);
    }
  }

  /// Get all messages that were sent
  pub fn sent_messages(&self) -> Vec<Value> {
    self.sent_messages.lock().unwrap().clone()
  }

  /// Get the last sent message
  pub fn last_sent_message(&self) -> Option<Value> {
    self.sent_messages.lock().unwrap().last().cloned()
  }

  /// Clear sent messages
  pub fn clear_sent_messages(&mut self) {
    self.sent_messages.lock().unwrap().clear();
  }

  /// Create a standard initialize response
  pub fn standard_initialize_response() -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "protocolVersion": "2024-11-05",
        "capabilities": {
          "tools": {},
          "resources": {}
        },
        "serverInfo": {
          "name": "mock-server",
          "version": "1.0.0"
        }
      }
    })
  }

  /// Create a tools/list response
  pub fn tools_list_response(tools: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 2,
      "result": {
        "tools": tools
      }
    })
  }

  /// Create a tools/call response
  pub fn tool_call_response(content: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 3,
      "result": {
        "content": content
      }
    })
  }

  /// Create a resources/list response
  pub fn resources_list_response(resources: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 4,
      "result": {
        "resources": resources
      }
    })
  }

  /// Create a resources/read response
  pub fn resource_read_response(contents: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 5,
      "result": {
        "contents": contents
      }
    })
  }

  /// Create a prompts/list response
  pub fn prompts_list_response(prompts: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 6,
      "result": {
        "prompts": prompts
      }
    })
  }

  /// Create a prompts/get response
  pub fn prompt_get_response(messages: Vec<Value>) -> Value {
    json!({
      "jsonrpc": "2.0",
      "id": 7,
      "result": {
        "messages": messages
      }
    })
  }
}

impl Default for MockTransport {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Transport for MockTransport {
  async fn connect(&mut self) -> MCPResult<()> {
    *self.connected.lock().unwrap() = true;
    Ok(())
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    if !*self.connected.lock().unwrap() {
      return Err(MCPError::connection("Not connected"));
    }

    // Record the sent message
    self.sent_messages.lock().unwrap().push(request);

    // Return the next queued response
    let response = self
      .responses
      .lock()
      .unwrap()
      .pop_front()
      .ok_or_else(|| MCPError::transport("No response configured for this request"))?;

    Ok(response)
  }

  async fn send_notification(&mut self, notification: Value) -> MCPResult<()> {
    if !*self.connected.lock().unwrap() {
      return Err(MCPError::connection("Not connected"));
    }

    // Record the notification
    self.sent_messages.lock().unwrap().push(notification);

    Ok(())
  }

  async fn receive_message(&mut self) -> MCPResult<Option<Value>> {
    if !*self.connected.lock().unwrap() {
      return Err(MCPError::connection("Not connected"));
    }

    // Return the next queued message, or None if no messages
    Ok(self.responses.lock().unwrap().pop_front())
  }

  async fn disconnect(&mut self) -> MCPResult<()> {
    *self.connected.lock().unwrap() = false;
    Ok(())
  }

  fn is_connected(&self) -> bool {
    *self.connected.lock().unwrap()
  }

  fn transport_type(&self) -> TransportType {
    TransportType::Stdio // Mock uses stdio type
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_mock_transport_connect() {
    let mut transport = MockTransport::new();
    assert!(!transport.is_connected());

    transport.connect().await.unwrap();
    assert!(transport.is_connected());
  }

  #[tokio::test]
  async fn test_mock_transport_send_message() {
    let mut transport = MockTransport::new();
    transport.connect().await.unwrap();

    let response = json!({"result": "ok"});
    transport.add_response(response.clone());

    let request = json!({"method": "test"});
    let result = transport.send_message(request.clone()).await.unwrap();

    assert_eq!(result, response);
    assert_eq!(transport.last_sent_message().unwrap(), request);
  }

  #[tokio::test]
  async fn test_mock_transport_multiple_responses() {
    let mut transport = MockTransport::new();
    transport.connect().await.unwrap();

    transport.add_responses(vec![
      json!({"result": "first"}),
      json!({"result": "second"}),
    ]);

    let result1 = transport.send_message(json!({"req": 1})).await.unwrap();
    let result2 = transport.send_message(json!({"req": 2})).await.unwrap();

    assert_eq!(result1["result"], "first");
    assert_eq!(result2["result"], "second");
  }

  #[test]
  fn test_standard_responses() {
    let init_resp = MockTransport::standard_initialize_response();
    assert_eq!(init_resp["result"]["protocolVersion"], "2024-11-05");

    let tools_resp = MockTransport::tools_list_response(vec![
      json!({"name": "test_tool"}),
    ]);
    assert_eq!(tools_resp["result"]["tools"][0]["name"], "test_tool");
  }
}
