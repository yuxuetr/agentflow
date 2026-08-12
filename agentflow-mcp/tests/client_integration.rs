//! Integration tests for MCP client
//!
//! These tests use MockTransport to simulate MCP server responses
//! and verify the client's behavior.

use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::transport::MockTransport;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_client_initialization() {
  // Create mock transport with initialize response
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());

  // Build client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();

  // Connect should trigger initialization
  client.connect().await.unwrap();

  // Check connection state
  assert!(client.is_connected().await);

  // Check server info
  let server_info = client.server_info().await;
  assert!(server_info.is_some());
  assert_eq!(server_info.unwrap().name, "mock-server");
}

#[tokio::test]
async fn test_list_tools() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::tools_list_response(vec![
    json!({
      "name": "add_numbers",
      "description": "Add two numbers",
      "inputSchema": {
        "type": "object",
        "properties": {
          "a": {"type": "number"},
          "b": {"type": "number"}
        },
        "required": ["a", "b"]
      }
    }),
    json!({
      "name": "multiply",
      "description": "Multiply two numbers",
      "inputSchema": {
        "type": "object",
        "properties": {
          "x": {"type": "number"},
          "y": {"type": "number"}
        }
      }
    }),
  ]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // List tools
  let tools = client.list_tools().await.unwrap();

  // Verify
  assert_eq!(tools.len(), 2);
  assert_eq!(tools[0].name, "add_numbers");
  assert_eq!(tools[0].description, Some("Add two numbers".to_string()));
  assert_eq!(tools[1].name, "multiply");
}

#[tokio::test]
async fn test_call_tool() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::tool_call_response(vec![json!({
    "type": "text",
    "text": "The sum is 8"
  })]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // Call tool
  let result = client
    .call_tool("add_numbers", json!({"a": 5, "b": 3}))
    .await
    .unwrap();

  // Verify
  assert_eq!(result.content.len(), 1);
  assert_eq!(result.first_text(), Some("The sum is 8"));
  assert!(!result.is_error());
}

#[tokio::test]
async fn test_call_tool_error() {
  // Setup mock transport with error response
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(
    MockTransport::tool_call_response(vec![json!({
      "type": "text",
      "text": "Error: invalid arguments"
    })])
    .tap_mut(|v| {
      v["result"]["isError"] = json!(true);
    }),
  );

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // Call tool
  let result = client
    .call_tool("bad_tool", json!({"invalid": "args"}))
    .await
    .unwrap();

  // Verify error
  assert!(result.is_error());
}

#[tokio::test]
async fn test_list_resources() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::resources_list_response(vec![
    json!({
      "uri": "file:///test.txt",
      "name": "test.txt",
      "description": "A test file",
      "mimeType": "text/plain"
    }),
    json!({
      "uri": "file:///data.json",
      "name": "data.json",
      "mimeType": "application/json"
    }),
  ]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // List resources
  let resources = client.list_resources().await.unwrap();

  // Verify
  assert_eq!(resources.len(), 2);
  assert_eq!(resources[0].uri, "file:///test.txt");
  assert_eq!(resources[0].name, "test.txt");
  assert_eq!(resources[1].uri, "file:///data.json");
}

#[tokio::test]
async fn test_read_resource() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::resource_read_response(vec![json!({
    "uri": "file:///test.txt",
    "mimeType": "text/plain",
    "text": "Hello, world!"
  })]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // Read resource
  let result = client.read_resource("file:///test.txt").await.unwrap();

  // Verify
  let content = result.first_content().unwrap();
  assert_eq!(content.uri, "file:///test.txt");
  assert_eq!(content.as_text(), Some("Hello, world!"));
  assert!(content.is_text());
}

#[tokio::test]
async fn test_list_prompts() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::prompts_list_response(vec![json!({
    "name": "code_review",
    "description": "Review code for best practices",
    "arguments": [
      {
        "name": "code",
        "description": "The code to review",
        "required": true
      },
      {
        "name": "language",
        "description": "Programming language",
        "required": false
      }
    ]
  })]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // List prompts
  let prompts = client.list_prompts().await.unwrap();

  // Verify
  assert_eq!(prompts.len(), 1);
  assert_eq!(prompts[0].name, "code_review");
  assert_eq!(prompts[0].arguments.len(), 2);
  assert!(prompts[0].arguments[0].is_required());
  assert!(!prompts[0].arguments[1].is_required());
}

#[tokio::test]
async fn test_get_prompt() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::prompt_get_response(vec![
    json!({
      "role": "user",
      "content": {
        "type": "text",
        "text": "Please review this Rust code"
      }
    }),
    json!({
      "role": "assistant",
      "content": {
        "type": "text",
        "text": "I'll review the code for you"
      }
    }),
  ]));

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // Get prompt
  let mut args = std::collections::HashMap::new();
  args.insert("code".to_string(), "fn main() {}".to_string());

  let result = client.get_prompt("code_review", args).await.unwrap();

  // Verify
  assert_eq!(result.messages.len(), 2);
  assert_eq!(result.first_text(), Some("Please review this Rust code"));
}

#[tokio::test]
async fn test_builder_configuration() {
  let transport = MockTransport::new();

  let client = ClientBuilder::new()
    .with_transport(transport)
    .with_timeout(Duration::from_secs(120))
    .with_max_retries(5)
    .with_retry_backoff_ms(200)
    .with_client_info("test-client", "2.0.0")
    .build()
    .await
    .unwrap();

  // Verify session ID is generated
  assert!(!client.session_id().is_empty());
}

#[tokio::test]
async fn test_builder_missing_transport() {
  let result = ClientBuilder::new().build().await;

  // Should fail with configuration error
  assert!(result.is_err());
  assert!(matches!(
    result.unwrap_err(),
    agentflow_mcp::error::MCPError::Configuration { .. }
  ));
}

#[tokio::test]
async fn test_disconnect() {
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();

  client.connect().await.unwrap();
  assert!(client.is_connected().await);

  client.disconnect().await.unwrap();
  assert!(!client.is_connected().await);
}

// Helper trait for tap-like mutation
trait TapMut {
  fn tap_mut<F: FnOnce(&mut Self)>(mut self, f: F) -> Self
  where
    Self: Sized,
  {
    f(&mut self);
    self
  }
}

impl TapMut for serde_json::Value {}

// ============================================================================
// W5.6: capability gating
// ============================================================================

/// An `initialize` response advertising only the given top-level
/// capability keys (e.g. `&["tools"]`) — used to build a server that
/// deliberately omits one or more capability families, for the negative
/// capability-gating tests below.
fn initialize_response_with_capabilities(keys: &[&str]) -> serde_json::Value {
  let mut capabilities = serde_json::Map::new();
  for key in keys {
    capabilities.insert((*key).to_string(), json!({}));
  }
  json!({
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
      "protocolVersion": "2024-11-05",
      "capabilities": capabilities,
      "serverInfo": { "name": "partial-server", "version": "1.0.0" }
    }
  })
}

#[tokio::test]
async fn list_resources_fails_fast_when_server_does_not_advertise_resources() {
  let mut transport = MockTransport::new();
  transport.add_response(initialize_response_with_capabilities(&["tools"]));
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  let result = client.list_resources().await;
  assert!(
    result.is_err(),
    "must reject when the server never advertised 'resources'"
  );

  // The whole point of client-side gating: the request never goes out.
  let messages = sent.lock().unwrap().clone();
  assert!(
    !messages
      .iter()
      .any(|m| m["method"].as_str() == Some("resources/list")),
    "resources/list must never be sent when the server didn't advertise the capability; got: {messages:?}"
  );
}

#[tokio::test]
async fn list_resources_succeeds_when_server_advertises_resources() {
  // Regression guard: the existing happy path must be unaffected.
  let mut transport = MockTransport::new();
  transport.add_response(initialize_response_with_capabilities(&["resources"]));
  transport.add_response(MockTransport::resources_list_response(vec![]));

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  let result = client.list_resources().await;
  assert!(
    result.is_ok(),
    "must succeed when the server advertised 'resources': {result:?}"
  );
}

#[tokio::test]
async fn list_prompts_fails_fast_when_server_does_not_advertise_prompts() {
  let mut transport = MockTransport::new();
  transport.add_response(initialize_response_with_capabilities(&["tools"]));
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  let result = client.list_prompts().await;
  assert!(
    result.is_err(),
    "must reject when the server never advertised 'prompts'"
  );

  let messages = sent.lock().unwrap().clone();
  assert!(
    !messages
      .iter()
      .any(|m| m["method"].as_str() == Some("prompts/list")),
    "prompts/list must never be sent when the server didn't advertise the capability; got: {messages:?}"
  );
}

#[tokio::test]
async fn list_tools_fails_fast_when_server_does_not_advertise_tools() {
  let mut transport = MockTransport::new();
  transport.add_response(initialize_response_with_capabilities(&["resources"]));
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  let result = client.list_tools().await;
  assert!(
    result.is_err(),
    "must reject when the server never advertised 'tools'"
  );

  let messages = sent.lock().unwrap().clone();
  assert!(
    !messages
      .iter()
      .any(|m| m["method"].as_str() == Some("tools/list")),
    "tools/list must never be sent when the server didn't advertise the capability; got: {messages:?}"
  );
}
