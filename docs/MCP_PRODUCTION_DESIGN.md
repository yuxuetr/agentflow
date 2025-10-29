# AgentFlow MCP Module - Production-Grade Design Document

**Version**: 1.0
**Date**: 2025-10-27
**Status**: Design Phase
**Owner**: AgentFlow Core Team

---

## Executive Summary

This document outlines the design and implementation plan for transforming the agentflow-mcp module from its current experimental state (30% complete) to a production-ready, fully-compliant MCP implementation.

**Current State**: 828 lines, basic stdio transport, partial tool calling
**Target State**: Production-ready MCP client/server with full protocol compliance
**Estimated Effort**: 8-10 weeks (2 months)
**Priority**: Phase 3 in AgentFlow roadmap

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Technical Design](#2-technical-design)
3. [Implementation Phases](#3-implementation-phases)
4. [Testing Strategy](#4-testing-strategy)
5. [Quality Standards](#5-quality-standards)
6. [Risk Assessment](#6-risk-assessment)
7. [Success Metrics](#7-success-metrics)

---

## 1. Architecture Overview

### 1.1 Design Principles

**High-Level Goals**:
- ✅ **Protocol Compliance**: Full MCP spec 2024-11-05 implementation
- ✅ **Production Quality**: Robust error handling, timeout, retry logic
- ✅ **Performance**: Async I/O, connection pooling, streaming support
- ✅ **Testability**: Mock transport, integration tests, protocol conformance tests
- ✅ **Observability**: Structured logging, metrics, tracing
- ✅ **Extensibility**: Plugin architecture for custom tools and resources

**Non-Goals** (Deferred):
- ❌ Official Rust MCP SDK integration (waiting for maturity)
- ❌ GraphQL transport (not in spec)
- ❌ WebSocket transport (use SSE instead)

### 1.2 Module Structure

```
agentflow-mcp/
├── Cargo.toml                 # Dependencies and features
├── src/
│   ├── lib.rs                 # Public API and re-exports
│   ├── error.rs               # Enhanced error types with context
│   │
│   ├── protocol/              # MCP Protocol Layer
│   │   ├── mod.rs
│   │   ├── types.rs           # JSON-RPC types, MCP message types
│   │   ├── lifecycle.rs       # initialize, initialized, shutdown
│   │   ├── capabilities.rs    # Capability negotiation
│   │   └── validator.rs       # JSON Schema validation
│   │
│   ├── transport/             # Transport Layer
│   │   ├── mod.rs
│   │   ├── traits.rs          # Transport trait abstraction
│   │   ├── stdio.rs           # Stdio transport (refactored)
│   │   ├── http.rs            # HTTP transport with reqwest
│   │   ├── sse.rs             # Server-Sent Events streaming
│   │   └── connection.rs      # Connection management, pooling
│   │
│   ├── client/                # MCP Client Implementation
│   │   ├── mod.rs
│   │   ├── builder.rs         # Fluent client builder
│   │   ├── session.rs         # Session management
│   │   ├── tools.rs           # Tool calling interface
│   │   ├── resources.rs       # Resource access interface
│   │   ├── prompts.rs         # Prompt template interface
│   │   └── sampling.rs        # Sampling interface (future)
│   │
│   ├── server/                # MCP Server Implementation
│   │   ├── mod.rs
│   │   ├── builder.rs         # Fluent server builder
│   │   ├── router.rs          # Request routing
│   │   ├── handlers/          # Protocol handlers
│   │   │   ├── tools.rs
│   │   │   ├── resources.rs
│   │   │   └── prompts.rs
│   │   └── middleware.rs      # Logging, auth, rate limiting
│   │
│   ├── registry/              # Tool and Resource Registry
│   │   ├── mod.rs
│   │   ├── tool_registry.rs   # Enhanced tool management
│   │   ├── resource_registry.rs
│   │   ├── prompt_registry.rs
│   │   └── schema_validator.rs
│   │
│   ├── discovery/             # Service Discovery
│   │   ├── mod.rs
│   │   ├── local.rs           # Local process discovery
│   │   └── remote.rs          # Remote server discovery
│   │
│   └── observability/         # Logging and Metrics
│       ├── mod.rs
│       ├── logger.rs          # Structured logging
│       ├── metrics.rs         # Prometheus metrics
│       └── tracing.rs         # Distributed tracing
│
├── tests/                     # Integration tests
│   ├── protocol_conformance/  # MCP spec compliance tests
│   ├── transport/             # Transport layer tests
│   ├── client/                # Client integration tests
│   └── server/                # Server integration tests
│
├── examples/                  # Usage examples
│   ├── simple_client.rs
│   ├── simple_server.rs
│   ├── custom_tool.rs
│   ├── resource_provider.rs
│   └── http_sse_client.rs
│
└── benches/                   # Performance benchmarks
    ├── transport_bench.rs
    └── protocol_bench.rs
```

---

## 2. Technical Design

### 2.1 Protocol Layer Design

#### 2.1.1 Core Types

```rust
// src/protocol/types.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
  pub jsonrpc: String,
  pub id: Option<RequestId>,
  pub method: String,
  pub params: Option<Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
  pub jsonrpc: String,
  pub id: Option<RequestId>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<JsonRpcError>,
}

/// Request ID (string or number)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
  String(String),
  Number(i64),
}

/// Standard JSON-RPC error codes
#[derive(Debug, Clone, Copy)]
pub enum ErrorCode {
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
  // MCP-specific error codes (range: -32000 to -32099)
  ToolNotFound = -32001,
  ToolExecutionFailed = -32002,
  ResourceNotFound = -32003,
  ResourceAccessDenied = -32004,
}

/// MCP Capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tools: Option<ToolsCapability>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub resources: Option<ResourcesCapability>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub prompts: Option<PromptsCapability>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sampling: Option<SamplingCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sampling: Option<SamplingCapability>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub roots: Option<RootsCapability>,
}

/// Initialize request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
  pub protocol_version: String, // "2024-11-05"
  pub capabilities: ClientCapabilities,
  pub client_info: Implementation,
}

/// Initialize response result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
  pub protocol_version: String,
  pub capabilities: ServerCapabilities,
  pub server_info: Implementation,
}

/// Implementation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
  pub name: String,
  pub version: String,
}
```

#### 2.1.2 Tool Calling Protocol

```rust
// src/client/tools.rs

use crate::protocol::types::*;
use async_trait::async_trait;

/// Tool calling interface
#[async_trait]
pub trait ToolCaller {
  /// List available tools
  async fn list_tools(&self) -> MCPResult<Vec<Tool>>;

  /// Call a tool
  async fn call_tool(&self, request: CallToolRequest) -> MCPResult<CallToolResult>;
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
  pub name: String,
  pub description: String,
  pub input_schema: Value, // JSON Schema
}

/// Tool call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolRequest {
  pub name: String,
  pub arguments: Value,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
  pub content: Vec<Content>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub is_error: Option<bool>,
}

/// Content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
  #[serde(rename = "text")]
  Text { text: String },

  #[serde(rename = "image")]
  Image {
    data: String,      // base64
    mime_type: String,
  },

  #[serde(rename = "resource")]
  Resource {
    resource: ResourceReference,
  },
}
```

#### 2.1.3 Resource Management Protocol

```rust
// src/client/resources.rs

/// Resource access interface
#[async_trait]
pub trait ResourceProvider {
  /// List available resources
  async fn list_resources(&self) -> MCPResult<Vec<Resource>>;

  /// Read a resource
  async fn read_resource(&self, uri: String) -> MCPResult<ReadResourceResult>;

  /// Subscribe to resource changes
  async fn subscribe_resource(&self, uri: String) -> MCPResult<()>;

  /// Unsubscribe from resource changes
  async fn unsubscribe_resource(&self, uri: String) -> MCPResult<()>;
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
  pub uri: String,
  pub name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mime_type: Option<String>,
}

/// Read resource result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
  pub contents: Vec<ResourceContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
  pub uri: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mime_type: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blob: Option<String>, // base64
}
```

### 2.2 Transport Layer Design

#### 2.2.1 Transport Abstraction

```rust
// src/transport/traits.rs

use async_trait::async_trait;
use serde_json::Value;

/// Transport trait for MCP communication
#[async_trait]
pub trait Transport: Send + Sync {
  /// Connect to the MCP server
  async fn connect(&mut self) -> MCPResult<()>;

  /// Send a message and receive response
  async fn send_message(&mut self, request: Value) -> MCPResult<Value>;

  /// Send a notification (no response expected)
  async fn send_notification(&mut self, notification: Value) -> MCPResult<()>;

  /// Receive a message (for server-initiated requests)
  async fn receive_message(&mut self) -> MCPResult<Option<Value>>;

  /// Close the connection
  async fn disconnect(&mut self) -> MCPResult<()>;

  /// Check if connected
  fn is_connected(&self) -> bool;

  /// Get transport type
  fn transport_type(&self) -> TransportType;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
  Stdio,
  Http,
  HttpWithSSE,
}
```

#### 2.2.2 Stdio Transport (Refactored)

```rust
// src/transport/stdio.rs

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use std::time::Duration;

pub struct StdioTransport {
  command: Vec<String>,
  process: Option<Child>,
  stdin: Option<BufWriter<tokio::process::ChildStdin>>,
  stdout: Option<BufReader<tokio::process::ChildStdout>>,
  connected: bool,
  timeout: Duration,
}

impl StdioTransport {
  pub fn new(command: Vec<String>) -> Self {
    Self {
      command,
      process: None,
      stdin: None,
      stdout: None,
      connected: false,
      timeout: Duration::from_secs(30),
    }
  }

  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  async fn read_line_with_timeout(&mut self) -> MCPResult<String> {
    let mut line = String::new();

    if let Some(stdout) = &mut self.stdout {
      match tokio::time::timeout(self.timeout, stdout.read_line(&mut line)).await {
        Ok(Ok(0)) => {
          return Err(MCPError::Transport {
            message: "EOF: Process terminated unexpectedly".to_string(),
          });
        }
        Ok(Ok(_)) => Ok(line.trim().to_string()),
        Ok(Err(e)) => Err(MCPError::Transport {
          message: format!("Failed to read from process: {}", e),
        }),
        Err(_) => Err(MCPError::Transport {
          message: format!("Read timeout after {:?}", self.timeout),
        }),
      }
    } else {
      Err(MCPError::Connection {
        message: "Not connected".to_string(),
      })
    }
  }

  async fn write_line(&mut self, data: &str) -> MCPResult<()> {
    if let Some(stdin) = &mut self.stdin {
      stdin.write_all(data.as_bytes()).await?;
      stdin.write_all(b"\n").await?;
      stdin.flush().await?;
      Ok(())
    } else {
      Err(MCPError::Connection {
        message: "Not connected".to_string(),
      })
    }
  }

  fn check_process_health(&mut self) -> MCPResult<()> {
    if let Some(process) = &mut self.process {
      match process.try_wait() {
        Ok(Some(status)) => {
          Err(MCPError::Connection {
            message: format!("Process exited with status: {}", status),
          })
        }
        Ok(None) => Ok(()), // Still running
        Err(e) => Err(MCPError::Connection {
          message: format!("Failed to check process status: {}", e),
        }),
      }
    } else {
      Err(MCPError::Connection {
        message: "Process not started".to_string(),
      })
    }
  }
}

#[async_trait]
impl Transport for StdioTransport {
  async fn connect(&mut self) -> MCPResult<()> {
    if self.connected {
      return Ok(());
    }

    let mut cmd = Command::new(&self.command[0]);
    if self.command.len() > 1 {
      cmd.args(&self.command[1..]);
    }

    let mut child = cmd
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      .spawn()
      .map_err(|e| MCPError::Connection {
        message: format!("Failed to spawn process: {}", e),
      })?;

    let stdin = child.stdin.take().ok_or_else(|| MCPError::Connection {
      message: "Failed to capture stdin".to_string(),
    })?;

    let stdout = child.stdout.take().ok_or_else(|| MCPError::Connection {
      message: "Failed to capture stdout".to_string(),
    })?;

    self.stdin = Some(BufWriter::new(stdin));
    self.stdout = Some(BufReader::new(stdout));
    self.process = Some(child);
    self.connected = true;

    Ok(())
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    self.check_process_health()?;

    let request_str = serde_json::to_string(&request)?;
    self.write_line(&request_str).await?;

    let response_str = self.read_line_with_timeout().await?;
    let response: Value = serde_json::from_str(&response_str)?;

    Ok(response)
  }

  async fn send_notification(&mut self, notification: Value) -> MCPResult<()> {
    self.check_process_health()?;

    let notification_str = serde_json::to_string(&notification)?;
    self.write_line(&notification_str).await?;

    Ok(())
  }

  async fn receive_message(&mut self) -> MCPResult<Option<Value>> {
    self.check_process_health()?;

    match self.read_line_with_timeout().await {
      Ok(line) => {
        let message: Value = serde_json::from_str(&line)?;
        Ok(Some(message))
      }
      Err(MCPError::Transport { message }) if message.contains("timeout") => {
        Ok(None)
      }
      Err(e) => Err(e),
    }
  }

  async fn disconnect(&mut self) -> MCPResult<()> {
    if let Some(mut process) = self.process.take() {
      let _ = process.kill().await;
      let _ = process.wait().await;
    }
    self.stdin = None;
    self.stdout = None;
    self.connected = false;
    Ok(())
  }

  fn is_connected(&self) -> bool {
    self.connected && self.check_process_health().is_ok()
  }

  fn transport_type(&self) -> TransportType {
    TransportType::Stdio
  }
}
```

#### 2.2.3 HTTP Transport with SSE

```rust
// src/transport/http.rs

use reqwest::{Client, Response};
use std::time::Duration;

pub struct HttpTransport {
  base_url: String,
  client: Client,
  session_id: Option<String>,
  connected: bool,
  sse_enabled: bool,
}

impl HttpTransport {
  pub fn new(base_url: String) -> Self {
    let client = Client::builder()
      .timeout(Duration::from_secs(30))
      .build()
      .unwrap();

    Self {
      base_url,
      client,
      session_id: None,
      connected: false,
      sse_enabled: false,
    }
  }

  pub fn with_sse(mut self) -> Self {
    self.sse_enabled = true;
    self
  }

  async fn post_message(&self, endpoint: &str, body: Value) -> MCPResult<Value> {
    let url = format!("{}{}", self.base_url, endpoint);

    let response = self
      .client
      .post(&url)
      .json(&body)
      .send()
      .await
      .map_err(|e| MCPError::Transport {
        message: format!("HTTP request failed: {}", e),
      })?;

    if !response.status().is_success() {
      return Err(MCPError::Transport {
        message: format!("HTTP error: {}", response.status()),
      });
    }

    let result: Value = response.json().await.map_err(|e| MCPError::Transport {
      message: format!("Failed to parse response: {}", e),
    })?;

    Ok(result)
  }
}

#[async_trait]
impl Transport for HttpTransport {
  async fn connect(&mut self) -> MCPResult<()> {
    // HTTP is connectionless, but we can verify server is reachable
    let health_check = self.post_message("/health", serde_json::json!({})).await;

    match health_check {
      Ok(_) => {
        self.connected = true;
        Ok(())
      }
      Err(_) => {
        // No health endpoint, try initialize instead
        self.connected = true;
        Ok(())
      }
    }
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    self.post_message("/messages", request).await
  }

  async fn send_notification(&mut self, notification: Value) -> MCPResult<()> {
    self.post_message("/notifications", notification).await?;
    Ok(())
  }

  async fn receive_message(&mut self) -> MCPResult<Option<Value>> {
    if self.sse_enabled {
      // SSE implementation would go here
      todo!("SSE message receiving not yet implemented")
    } else {
      // HTTP doesn't support server-initiated messages without SSE
      Ok(None)
    }
  }

  async fn disconnect(&mut self) -> MCPResult<()> {
    self.connected = false;
    self.session_id = None;
    Ok(())
  }

  fn is_connected(&self) -> bool {
    self.connected
  }

  fn transport_type(&self) -> TransportType {
    if self.sse_enabled {
      TransportType::HttpWithSSE
    } else {
      TransportType::Http
    }
  }
}
```

### 2.3 Client Design

#### 2.3.1 Fluent Client Builder

```rust
// src/client/builder.rs

pub struct MCPClientBuilder {
  transport: Option<Box<dyn Transport>>,
  timeout: Duration,
  max_retries: u32,
  client_info: Implementation,
  capabilities: ClientCapabilities,
}

impl MCPClientBuilder {
  pub fn new() -> Self {
    Self {
      transport: None,
      timeout: Duration::from_secs(30),
      max_retries: 3,
      client_info: Implementation {
        name: "agentflow-mcp".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
      },
      capabilities: ClientCapabilities::default(),
    }
  }

  pub fn stdio(mut self, command: Vec<String>) -> Self {
    self.transport = Some(Box::new(StdioTransport::new(command)));
    self
  }

  pub fn http<S: Into<String>>(mut self, base_url: S) -> Self {
    self.transport = Some(Box::new(HttpTransport::new(base_url.into())));
    self
  }

  pub fn http_with_sse<S: Into<String>>(mut self, base_url: S) -> Self {
    self.transport = Some(Box::new(HttpTransport::new(base_url.into()).with_sse()));
    self
  }

  pub fn timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  pub fn max_retries(mut self, retries: u32) -> Self {
    self.max_retries = retries;
    self
  }

  pub fn client_info(mut self, name: String, version: String) -> Self {
    self.client_info = Implementation { name, version };
    self
  }

  pub async fn build(self) -> MCPResult<MCPClient> {
    let transport = self.transport.ok_or_else(|| MCPError::Configuration {
      message: "Transport not configured".to_string(),
    })?;

    let mut client = MCPClient {
      transport,
      session_id: Uuid::new_v4().to_string(),
      timeout: self.timeout,
      max_retries: self.max_retries,
      client_info: self.client_info,
      server_info: None,
      server_capabilities: None,
      request_counter: Arc::new(AtomicU64::new(0)),
    };

    // Auto-initialize on build
    client.connect_and_initialize().await?;

    Ok(client)
  }
}

/// Example usage:
/// ```rust
/// let client = MCPClientBuilder::new()
///   .stdio(vec!["npx".to_string(), "-y".to_string(), "@modelcontextprotocol/server-everything".to_string()])
///   .timeout(Duration::from_secs(60))
///   .build()
///   .await?;
/// ```
```

### 2.4 Server Design

#### 2.4.1 Server Builder

```rust
// src/server/builder.rs

pub struct MCPServerBuilder {
  transport_type: TransportType,
  port: Option<u16>,
  server_info: Implementation,
  tools: Vec<Box<dyn ToolHandler>>,
  resources: Vec<Box<dyn ResourceHandler>>,
  prompts: Vec<Box<dyn PromptHandler>>,
}

impl MCPServerBuilder {
  pub fn new() -> Self {
    Self {
      transport_type: TransportType::Stdio,
      port: None,
      server_info: Implementation {
        name: "agentflow-mcp-server".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
      },
      tools: Vec::new(),
      resources: Vec::new(),
      prompts: Vec::new(),
    }
  }

  pub fn stdio(mut self) -> Self {
    self.transport_type = TransportType::Stdio;
    self
  }

  pub fn http(mut self, port: u16) -> Self {
    self.transport_type = TransportType::Http;
    self.port = Some(port);
    self
  }

  pub fn register_tool(mut self, tool: Box<dyn ToolHandler>) -> Self {
    self.tools.push(tool);
    self
  }

  pub fn register_resource(mut self, resource: Box<dyn ResourceHandler>) -> Self {
    self.resources.push(resource);
    self
  }

  pub async fn serve(self) -> MCPResult<()> {
    let server = MCPServer::new(self);
    server.run().await
  }
}
```

### 2.5 Testing Infrastructure

#### 2.5.1 Mock Transport

```rust
// tests/common/mock_transport.rs

pub struct MockTransport {
  sent_messages: Arc<Mutex<Vec<Value>>>,
  responses: Arc<Mutex<VecDeque<Value>>>,
  connected: bool,
}

impl MockTransport {
  pub fn new() -> Self {
    Self {
      sent_messages: Arc::new(Mutex::new(Vec::new())),
      responses: Arc::new(Mutex::new(VecDeque::new())),
      connected: false,
    }
  }

  pub fn add_response(&mut self, response: Value) {
    self.responses.lock().unwrap().push_back(response);
  }

  pub fn get_sent_messages(&self) -> Vec<Value> {
    self.sent_messages.lock().unwrap().clone()
  }
}

#[async_trait]
impl Transport for MockTransport {
  async fn connect(&mut self) -> MCPResult<()> {
    self.connected = true;
    Ok(())
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    self.sent_messages.lock().unwrap().push(request.clone());

    if let Some(response) = self.responses.lock().unwrap().pop_front() {
      Ok(response)
    } else {
      Err(MCPError::Transport {
        message: "No mock response available".to_string(),
      })
    }
  }

  // ... other methods
}
```

---

## 3. Implementation Phases

### Phase 1: Foundation (Week 1-2)

**Goal**: Refactor and strengthen core protocol and transport layers

**Tasks**:
1. ✅ Refactor error types with context and stack traces
2. ✅ Implement enhanced protocol types with validation
3. ✅ Refactor stdio transport with buffered I/O
4. ✅ Add timeout and health check mechanisms
5. ✅ Create mock transport for testing
6. ✅ Write protocol conformance tests
7. ✅ Add structured logging with tracing

**Deliverables**:
- Robust stdio transport with error recovery
- Complete protocol type definitions
- Mock transport for testing
- ~50% test coverage

**Success Criteria**:
- All existing tests pass
- New protocol conformance tests pass
- No panics or unwraps in production code

### Phase 2: Client Implementation (Week 3-4)

**Goal**: Complete production-ready MCP client

**Tasks**:
1. ✅ Implement fluent client builder
2. ✅ Add session management and lifecycle
3. ✅ Implement tool calling interface
4. ✅ Implement resource access interface
5. ✅ Implement prompt template interface
6. ✅ Add automatic retry with exponential backoff
7. ✅ Add connection pooling for HTTP transport
8. ✅ Write comprehensive client integration tests

**Deliverables**:
- Production-ready MCPClient
- Fluent builder API
- Complete tool/resource/prompt support
- ~70% test coverage

**Success Criteria**:
- Can connect to real MCP servers
- All MCP client operations functional
- Passes integration tests with reference servers

### Phase 3: Server Implementation (Week 5-6)

**Goal**: Complete production-ready MCP server

**Tasks**:
1. ✅ Implement server builder and router
2. ✅ Implement tool handler registry
3. ✅ Implement resource handler registry
4. ✅ Implement prompt handler registry
5. ✅ Add middleware support (logging, auth, rate limiting)
6. ✅ Implement HTTP server with actix-web or axum
7. ✅ Add SSE support for server-initiated messages
8. ✅ Write server integration tests

**Deliverables**:
- Production-ready MCPServer
- Plugin architecture for handlers
- HTTP and stdio server support
- ~70% test coverage

**Success Criteria**:
- Can serve MCP requests via stdio and HTTP
- Reference MCP clients can connect
- Passes server conformance tests

### Phase 4: Advanced Features (Week 7-8)

**Goal**: Add production-grade features and polish

**Tasks**:
1. ✅ Implement JSON Schema validation for tool inputs
2. ✅ Add Prometheus metrics collection
3. ✅ Add distributed tracing support
4. ✅ Implement resource subscriptions
5. ✅ Add sampling support (if needed)
6. ✅ Create comprehensive examples
7. ✅ Write user guide and API documentation
8. ✅ Performance benchmarking and optimization

**Deliverables**:
- Full MCP spec compliance
- Observability infrastructure
- Comprehensive documentation
- Performance benchmarks
- ~80% test coverage

**Success Criteria**:
- Passes all MCP conformance tests
- Performance meets benchmarks
- Documentation complete
- Ready for production use

### Phase 5: Integration and Stabilization (Week 9-10)

**Goal**: Integrate with AgentFlow and stabilize

**Tasks**:
1. ✅ Update MCPToolNode to use new client
2. ✅ Create MCPServerNode to expose workflows
3. ✅ Add MCP tool discovery to LLMNode
4. ✅ Create workflow examples with MCP integration
5. ✅ Update CLI with MCP commands
6. ✅ Fix remaining bugs and edge cases
7. ✅ Security audit and hardening
8. ✅ Performance tuning

**Deliverables**:
- Full AgentFlow integration
- Example workflows
- CLI commands for MCP
- Security review complete
- ~85% test coverage

**Success Criteria**:
- AgentFlow workflows can use MCP tools
- LLM can auto-discover MCP tools
- All integration tests pass
- Security vulnerabilities addressed

---

## 4. Testing Strategy

### 4.1 Test Pyramid

```
                    E2E Tests (5%)
                   /             \
              Integration Tests (25%)
             /                       \
        Unit Tests (70%)
```

### 4.2 Test Categories

#### 4.2.1 Unit Tests
- Protocol type serialization/deserialization
- Error handling and conversion
- Transport layer methods
- Client/server builder logic
- Registry operations

**Target**: 70% of test effort, ~85% code coverage

#### 4.2.2 Integration Tests
- Client-server communication
- Tool calling end-to-end
- Resource access end-to-end
- HTTP transport with real server
- Stdio transport with mock process

**Target**: 25% of test effort

#### 4.2.3 Protocol Conformance Tests
- MCP spec 2024-11-05 compliance
- JSON-RPC 2.0 compliance
- Error code standards
- Capability negotiation

**Target**: Built-in test suite

#### 4.2.4 E2E Tests
- Integration with reference MCP servers
- Real-world workflow scenarios
- Performance under load

**Target**: 5% of test effort

### 4.3 Test Infrastructure

```rust
// tests/common/mod.rs

pub mod fixtures;
pub mod mock_transport;
pub mod test_server;
pub mod assertions;

/// Test fixture for MCP client
pub async fn create_test_client() -> MCPClient {
  let mut mock_transport = MockTransport::new();

  // Add initialize response
  mock_transport.add_response(json!({
    "jsonrpc": "2.0",
    "id": 1,
    "result": {
      "protocolVersion": "2024-11-05",
      "capabilities": {
        "tools": {}
      },
      "serverInfo": {
        "name": "test-server",
        "version": "1.0.0"
      }
    }
  }));

  MCPClientBuilder::new()
    .transport(Box::new(mock_transport))
    .build()
    .await
    .unwrap()
}
```

### 4.4 CI/CD Integration

**GitHub Actions Workflow**:
```yaml
name: MCP Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run unit tests
        run: cargo test --lib

      - name: Run integration tests
        run: cargo test --test '*'

      - name: Run conformance tests
        run: cargo test --features conformance

      - name: Check code coverage
        uses: actions-rs/tarpaulin@v0.1
        with:
          args: '--out Xml --output-dir coverage/'

      - name: Upload coverage to Codecov
        uses: codecov/codecov-action@v3
```

---

## 5. Quality Standards

### 5.1 Code Quality

**Metrics**:
- ✅ Test coverage: ≥ 80%
- ✅ Clippy warnings: 0 (all fixed)
- ✅ Rustfmt compliance: 100%
- ✅ Documentation coverage: ≥ 90% for public API
- ✅ No unsafe code (unless explicitly justified)
- ✅ No unwrap() or panic!() in production code

**Code Review Checklist**:
- [ ] All public APIs documented
- [ ] Error handling comprehensive
- [ ] No resource leaks (connections, files)
- [ ] Async operations properly cancelled
- [ ] Tests cover happy path and error cases
- [ ] Performance implications considered

### 5.2 Performance Benchmarks

**Target Metrics**:
- Stdio transport latency: < 10ms per message
- HTTP transport latency: < 50ms per message
- Memory usage: < 10MB baseline
- CPU usage: < 5% idle
- Throughput: > 1000 messages/sec

**Benchmark Suite**:
```rust
// benches/transport_bench.rs

#[bench]
fn bench_stdio_roundtrip(b: &mut Bencher) {
  // Measure stdio transport round-trip time
}

#[bench]
fn bench_tool_call_throughput(b: &mut Bencher) {
  // Measure tool calling throughput
}
```

### 5.3 Security Requirements

**Checklist**:
- [ ] Input validation on all external data
- [ ] No code injection vulnerabilities
- [ ] Rate limiting on server endpoints
- [ ] Timeout on all I/O operations
- [ ] Secrets never logged or exposed
- [ ] TLS support for HTTP transport (future)
- [ ] Authentication/authorization hooks (future)

---

## 6. Risk Assessment

### 6.1 Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Official Rust SDK released mid-project | Medium | High | Monitor SDK progress, design for easy migration |
| MCP spec changes | Low | Medium | Use versioned protocol, design for extensibility |
| Performance issues with stdio | Medium | Medium | Benchmark early, optimize transport layer |
| HTTP/SSE complexity | Medium | Medium | Start with basic HTTP, SSE as optional feature |
| Integration challenges with AgentFlow | Low | High | Early integration tests, continuous feedback |

### 6.2 Resource Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Underestimated complexity | Medium | Medium | Phase gates, adjust scope if needed |
| Insufficient testing resources | Low | High | Prioritize test automation, CI/CD early |
| Documentation lag | Medium | Low | Write docs alongside code, not after |

### 6.3 Adoption Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Limited MCP server ecosystem | Medium | Medium | Provide reference implementations, examples |
| Breaking changes needed | Low | Medium | Semantic versioning, migration guides |
| Performance not meeting expectations | Low | High | Early benchmarking, performance-focused design |

---

## 7. Success Metrics

### 7.1 Quantitative Metrics

**Code Metrics**:
- Lines of code: ~5,000-6,000 (production code)
- Test code ratio: 1:1 (equal to production)
- Test coverage: ≥ 80%
- Documentation: ≥ 90% public API coverage

**Performance Metrics**:
- Stdio latency: < 10ms
- HTTP latency: < 50ms
- Memory footprint: < 10MB
- Throughput: > 1,000 msg/sec

**Quality Metrics**:
- Zero clippy warnings
- Zero panics in tests
- Zero memory leaks (valgrind)
- Security audit: 0 high/critical issues

### 7.2 Qualitative Metrics

**Usability**:
- ✅ Fluent API that's intuitive to use
- ✅ Clear error messages with actionable guidance
- ✅ Comprehensive examples covering common use cases
- ✅ Documentation clarity rated ≥ 4/5 by reviewers

**Reliability**:
- ✅ Graceful handling of network failures
- ✅ Automatic reconnection with backoff
- ✅ No silent failures or data loss
- ✅ Predictable behavior under load

**Maintainability**:
- ✅ Clear module boundaries
- ✅ Minimal coupling between components
- ✅ Easy to extend with new transport types
- ✅ Easy to add new tool/resource handlers

---

## Appendix A: Dependencies

### A.1 Core Dependencies

```toml
[dependencies]
# Async runtime
tokio = { version = "1.0", features = ["full"] }
async-trait = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# HTTP client/server
reqwest = { version = "0.11", features = ["json", "stream"] }
axum = "0.7" # or actix-web
tower = "0.4"
tower-http = "0.5"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Utilities
uuid = { version = "1.0", features = ["v4", "serde"] }
url = "2.0"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
metrics = "0.21"
metrics-exporter-prometheus = "0.12"

# Validation
jsonschema = "0.17"

# SSE
async-sse = "5.1"
futures = "0.3"
```

### A.2 Dev Dependencies

```toml
[dev-dependencies]
tokio-test = "0.4"
wiremock = "0.5"
assert_matches = "1.5"
proptest = "1.0"
criterion = "0.5"
```

---

## Appendix B: Migration Path

### B.1 Current Code Migration

**Files to Refactor**:
1. `src/client.rs` → New client implementation
2. `src/server.rs` → New server implementation
3. `src/transport.rs` → Split into separate files
4. `src/tools.rs` → Move to `src/registry/`

**Breaking Changes**:
- MCPClient API changed to builder pattern
- Transport trait signature changed
- Error types reorganized

**Migration Guide for Users**:
```rust
// Old API
let client = MCPClient::stdio(vec!["cmd".to_string()]);
client.connect().await?;
let tools = client.list_tools().await?;

// New API
let client = MCPClientBuilder::new()
  .stdio(vec!["cmd".to_string()])
  .build()
  .await?;
let tools = client.list_tools().await?;
```

### B.2 Backward Compatibility

**Strategy**:
- Keep old API with deprecation warnings for 1 minor version
- Provide migration guide in CHANGELOG
- Automated migration tool (if feasible)

---

## Appendix C: Future Enhancements

**Post-v1.0 Features**:
1. **WebSocket Transport** - For bidirectional streaming
2. **gRPC Transport** - For high-performance scenarios
3. **Connection Pooling** - Reuse connections across requests
4. **Distributed Tracing** - OpenTelemetry integration
5. **Authentication** - OAuth, API keys, JWT support
6. **Rate Limiting** - Token bucket, sliding window
7. **Caching** - Tool results, resource content
8. **Compression** - gzip, brotli for large payloads
9. **Multiplexing** - Multiple concurrent requests per connection
10. **Service Mesh** - Istio/Linkerd integration

---

**Document Version History**:
- v1.0 (2025-10-27): Initial design document
