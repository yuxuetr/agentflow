//! End-to-end Streamable HTTP server tests (W5.8-8).
//!
//! Covers the header-validation branches in
//! `server_streamable_http::handle_streamable_http` directly against a
//! real bound TCP server, plus a capstone test connecting with this
//! session's own client-side stack
//! (`transport::StreamableHttpTransport` + `client::ClientBuilder`, from
//! W5.8-3/4) — the first genuine Modern↔Modern client↔server round trip
//! in this crate's test suite, proving the RFC's compatibility-matrix
//! row "Modern ↔ Modern: Works" end-to-end rather than only against
//! hand-built fixtures on each side independently.

use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::protocol::McpEra;
use agentflow_mcp::server::{AgentFlowServerHandler, MCPServer};
use agentflow_mcp::server_streamable_http::streamable_http_router;
use agentflow_mcp::transport::StreamableHttpTransport;
use serde_json::{Value, json};
use std::sync::Arc;

/// Bind a fresh Streamable HTTP server on an OS-assigned loopback port
/// and return its base URL. `run_streamable_http` isn't used here
/// because it binds internally and never returns the resolved address —
/// fine for a real long-running process, useless for a test that needs
/// port 0. `streamable_http_router` (the composable piece
/// `run_streamable_http` itself is a thin wrapper around) is exactly
/// what's meant to be embedded like this.
async fn spawn_test_server() -> String {
  let server = Arc::new(MCPServer::new(Box::new(AgentFlowServerHandler::new())));
  let app = streamable_http_router(server);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
    .await
    .expect("bind");
  let addr = listener.local_addr().expect("local_addr");
  tokio::spawn(async move {
    axum::serve(listener, app).await.expect("serve");
  });
  format!("http://{addr}")
}

/// Rust-HTTP-testing rule: loopback requests must bypass any system
/// proxy (Clash/V2Ray etc. on 127.0.0.1) or they black-hole and
/// misreport as an opaque connection error.
fn test_client() -> reqwest::Client {
  reqwest::Client::builder()
    .no_proxy()
    .build()
    .expect("test client")
}

#[tokio::test]
async fn valid_modern_request_returns_200_with_json_rpc_response() {
  let base_url = spawn_test_server().await;
  let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2026-07-28")
    .header("Mcp-Method", "tools/list")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::OK);
  let value: Value = response.json().await.expect("json body");
  assert!(value["result"]["tools"].is_array());
}

#[tokio::test]
async fn missing_protocol_version_header_returns_unsupported_protocol_version_error() {
  let base_url = spawn_test_server().await;
  let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});

  let response = test_client()
    .post(&base_url)
    .header("Mcp-Method", "tools/list")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let value: Value = response.json().await.expect("json body");
  assert_eq!(value["error"]["code"], json!(-32022));
  assert!(
    value["error"]["data"]["supported"]
      .as_array()
      .expect("supported array")
      .contains(&json!("2026-07-28"))
  );
}

#[tokio::test]
async fn wrong_protocol_version_header_returns_unsupported_protocol_version_error() {
  let base_url = spawn_test_server().await;
  let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2024-11-05")
    .header("Mcp-Method", "tools/list")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let value: Value = response.json().await.expect("json body");
  assert_eq!(value["error"]["code"], json!(-32022));
}

#[tokio::test]
async fn mcp_method_header_mismatch_returns_header_mismatch() {
  let base_url = spawn_test_server().await;
  let body = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2026-07-28")
    .header("Mcp-Method", "tools/call") // disagrees with body.method
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let value: Value = response.json().await.expect("json body");
  assert_eq!(value["error"]["code"], json!(-32020));
}

#[tokio::test]
async fn tools_call_with_wrong_mcp_name_header_returns_header_mismatch() {
  let base_url = spawn_test_server().await;
  let body = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": { "name": "run_workflow", "arguments": { "workflow_path": "x.yml" } }
  });

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2026-07-28")
    .header("Mcp-Method", "tools/call")
    .header("Mcp-Name", "not_run_workflow")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let value: Value = response.json().await.expect("json body");
  assert_eq!(value["error"]["code"], json!(-32020));
}

#[tokio::test]
async fn tools_call_missing_mcp_name_header_returns_header_mismatch() {
  let base_url = spawn_test_server().await;
  let body = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": { "name": "run_workflow", "arguments": {} }
  });

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2026-07-28")
    .header("Mcp-Method", "tools/call")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
  let value: Value = response.json().await.expect("json body");
  assert_eq!(value["error"]["code"], json!(-32020));
}

#[tokio::test]
async fn notification_returns_202_with_empty_body() {
  let base_url = spawn_test_server().await;
  let body = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});

  let response = test_client()
    .post(&base_url)
    .header("MCP-Protocol-Version", "2026-07-28")
    .header("Mcp-Method", "notifications/initialized")
    .json(&body)
    .send()
    .await
    .expect("send");

  assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
  let bytes = response.bytes().await.expect("bytes");
  assert!(bytes.is_empty());
}

/// Capstone: the real client-side stack (W5.8-3/4) against the real
/// server-side stack (W5.8-6/7), both built this session, talking over
/// a real TCP socket — not a hand-built fixture on either side.
#[tokio::test]
async fn client_and_server_interop_end_to_end() {
  let base_url = spawn_test_server().await;
  let transport = StreamableHttpTransport::with_client(base_url, test_client());

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .expect("build client");
  client.connect().await.expect("connect");
  assert_eq!(client.era().await, McpEra::Modern);

  let tools = client.list_tools().await.expect("list_tools");
  assert!(
    tools.iter().any(|t| t.name == "run_workflow"),
    "expected the example server's run_workflow tool: {tools:?}"
  );

  let result = client
    .call_tool("run_workflow", json!({"workflow_path": "example.yml"}))
    .await
    .expect("call_tool");
  assert!(!result.is_error());
  assert!(
    result
      .first_text()
      .expect("text content")
      .contains("example.yml")
  );
}
