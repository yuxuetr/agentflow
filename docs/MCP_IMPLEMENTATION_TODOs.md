# AgentFlow MCP Module - Production Implementation TODOs

**Project**: AgentFlow MCP Production Implementation
**Version**: 1.0
**Start Date**: 2025-10-27
**Target Completion**: 2025-12-22 (8 weeks)
**Status**: Planning Phase

---

## Overview

This document tracks the implementation of production-grade MCP (Model Context Protocol) support in AgentFlow. The project is divided into 5 phases over 8-10 weeks.

**Reference Documents**:
- Design: `/docs/MCP_PRODUCTION_DESIGN.md`
- Current Status: `/IMPLEMENTATION_STATUS.md`
- MCP Spec: https://spec.modelcontextprotocol.io/specification/2024-11-05/

---

## Progress Summary

**Overall Progress**: 0/137 tasks completed (0%)

### Phase Breakdown

| Phase | Duration | Tasks | Status | Progress |
|-------|----------|-------|--------|----------|
| Phase 1: Foundation | Week 1-2 | 28 | TODO | 0% |
| Phase 2: Client | Week 3-4 | 32 | TODO | 0% |
| Phase 3: Server | Week 5-6 | 30 | TODO | 0% |
| Phase 4: Advanced | Week 7-8 | 27 | TODO | 0% |
| Phase 5: Integration | Week 9-10 | 20 | TODO | 0% |

---

## Phase 1: Foundation (Week 1-2)

**Goal**: Refactor and strengthen core protocol and transport layers
**Duration**: 10 working days
**Progress**: 0/28 tasks (0%)

### 1.1 Error Handling Enhancement

- [ ] **P0** Refactor `src/error.rs` with enhanced error types
  - Add `source` field for error chaining
  - Add `backtrace` support (with feature flag)
  - Implement `context()` helper methods
  - Add error code constants for JSON-RPC
  - **Files**: `agentflow-mcp/src/error.rs`
  - **Tests**: Unit tests for error conversion
  - **Estimate**: 4 hours

- [ ] **P0** Create error context utilities
  - Implement `ErrorContext` trait
  - Add contextual error wrapping macros
  - **Files**: `agentflow-mcp/src/error.rs`
  - **Tests**: Unit tests for context wrapping
  - **Estimate**: 2 hours

### 1.2 Protocol Types Implementation

- [ ] **P0** Implement core JSON-RPC types
  - Create `JsonRpcRequest` struct
  - Create `JsonRpcResponse` struct
  - Create `JsonRpcError` struct
  - Implement `RequestId` enum (String/Number)
  - Add serde serialization/deserialization
  - **Files**: `agentflow-mcp/src/protocol/types.rs`
  - **Tests**: Serde round-trip tests
  - **Estimate**: 3 hours

- [ ] **P0** Implement MCP lifecycle types
  - Create `InitializeParams` struct
  - Create `InitializeResult` struct
  - Create `Implementation` struct
  - **Files**: `agentflow-mcp/src/protocol/types.rs`
  - **Tests**: Serialization tests
  - **Estimate**: 2 hours

- [ ] **P0** Implement capability types
  - Create `ServerCapabilities` struct
  - Create `ClientCapabilities` struct
  - Create specific capability structs (Tools, Resources, Prompts, Sampling)
  - **Files**: `agentflow-mcp/src/protocol/types.rs`
  - **Tests**: Capability negotiation tests
  - **Estimate**: 3 hours

- [ ] **P1** Implement protocol lifecycle module
  - Create `initialize()` request builder
  - Create `initialized` notification builder
  - Add lifecycle state machine
  - **Files**: `agentflow-mcp/src/protocol/lifecycle.rs`
  - **Tests**: Lifecycle state transition tests
  - **Estimate**: 4 hours

- [ ] **P1** Implement capability negotiation
  - Add capability intersection logic
  - Add capability validation
  - **Files**: `agentflow-mcp/src/protocol/capabilities.rs`
  - **Tests**: Capability negotiation scenarios
  - **Estimate**: 3 hours

### 1.3 Transport Layer Refactoring

- [ ] **P0** Create transport abstraction trait
  - Define `Transport` trait with all methods
  - Add `TransportType` enum
  - Document transport contract
  - **Files**: `agentflow-mcp/src/transport/traits.rs`
  - **Tests**: Trait documentation examples
  - **Estimate**: 2 hours

- [ ] **P0** Refactor stdio transport - Part 1 (Reading)
  - Replace byte-by-byte reading with `BufReader::read_line()`
  - Add timeout support with `tokio::time::timeout()`
  - Add process health checking
  - **Files**: `agentflow-mcp/src/transport/stdio.rs`
  - **Tests**: Stdio reading tests with mock process
  - **Estimate**: 4 hours

- [ ] **P0** Refactor stdio transport - Part 2 (Writing)
  - Use `BufWriter` for buffered writing
  - Add write timeout support
  - Implement proper error handling
  - **Files**: `agentflow-mcp/src/transport/stdio.rs`
  - **Tests**: Stdio writing tests
  - **Estimate**: 3 hours

- [ ] **P0** Refactor stdio transport - Part 3 (Connection Management)
  - Implement proper connect/disconnect lifecycle
  - Add process monitoring (check if process alive)
  - Add automatic reconnection (optional)
  - Implement graceful shutdown
  - **Files**: `agentflow-mcp/src/transport/stdio.rs`
  - **Tests**: Connection lifecycle tests
  - **Estimate**: 5 hours

- [ ] **P1** Add stdio transport configuration
  - Create `StdioConfig` struct
  - Add configurable timeout
  - Add configurable buffer sizes
  - **Files**: `agentflow-mcp/src/transport/stdio.rs`
  - **Tests**: Configuration tests
  - **Estimate**: 2 hours

### 1.4 Testing Infrastructure

- [ ] **P0** Create mock transport for testing
  - Implement `MockTransport` struct
  - Add request recording
  - Add response queueing
  - Implement error injection
  - **Files**: `agentflow-mcp/tests/common/mock_transport.rs`
  - **Tests**: Mock transport self-tests
  - **Estimate**: 4 hours

- [ ] **P0** Create test fixtures and utilities
  - Add common test data builders
  - Create assertion helpers
  - Add test server fixtures
  - **Files**: `agentflow-mcp/tests/common/fixtures.rs`
  - **Tests**: Fixture validation tests
  - **Estimate**: 3 hours

- [ ] **P1** Write protocol conformance tests
  - Test JSON-RPC 2.0 compliance
  - Test MCP initialize/initialized flow
  - Test error code standards
  - **Files**: `agentflow-mcp/tests/protocol_conformance/`
  - **Tests**: 10+ conformance test cases
  - **Estimate**: 6 hours

### 1.5 Observability

- [ ] **P1** Add structured logging with tracing
  - Add tracing subscriber setup
  - Add log statements to protocol layer
  - Add log statements to transport layer
  - Use spans for request tracking
  - **Files**: All source files
  - **Tests**: Log output validation tests
  - **Estimate**: 4 hours

- [ ] **P2** Add metrics collection framework
  - Define key metrics (latency, throughput, errors)
  - Add metrics trait abstraction
  - Create prometheus exporter (feature-gated)
  - **Files**: `agentflow-mcp/src/observability/metrics.rs`
  - **Tests**: Metrics collection tests
  - **Estimate**: 5 hours

### 1.6 Documentation

- [ ] **P1** Write protocol layer documentation
  - Document all protocol types
  - Add usage examples
  - Document error cases
  - **Files**: `agentflow-mcp/src/protocol/*.rs`
  - **Tests**: Doc tests for examples
  - **Estimate**: 3 hours

- [ ] **P1** Write transport layer documentation
  - Document Transport trait
  - Document stdio transport usage
  - Add troubleshooting guide
  - **Files**: `agentflow-mcp/src/transport/*.rs`
  - **Tests**: Doc tests for examples
  - **Estimate**: 2 hours

### Phase 1 Milestone Checklist

- [ ] All Phase 1 tasks completed
- [ ] Test coverage ≥ 50%
- [ ] No clippy warnings
- [ ] Documentation builds without errors
- [ ] Protocol conformance tests pass
- [ ] Stdio transport passes reliability tests

---

## Phase 2: Client Implementation (Week 3-4)

**Goal**: Complete production-ready MCP client
**Duration**: 10 working days
**Progress**: 0/32 tasks (0%)

### 2.1 Client Core

- [ ] **P0** Create client builder foundation
  - Implement `MCPClientBuilder` struct
  - Add fluent API methods
  - Add transport configuration
  - Add timeout/retry configuration
  - **Files**: `agentflow-mcp/src/client/builder.rs`
  - **Tests**: Builder API tests
  - **Estimate**: 4 hours

- [ ] **P0** Implement client session management
  - Create `MCPClient` struct
  - Add session ID generation
  - Implement connect and initialize flow
  - Add connection state tracking
  - **Files**: `agentflow-mcp/src/client/session.rs`
  - **Tests**: Session lifecycle tests
  - **Estimate**: 5 hours

- [ ] **P0** Implement request/response handling
  - Create request ID generation
  - Implement message sending
  - Add response correlation
  - Implement timeout handling
  - **Files**: `agentflow-mcp/src/client/session.rs`
  - **Tests**: Request/response matching tests
  - **Estimate**: 4 hours

- [ ] **P1** Add automatic retry with backoff
  - Implement exponential backoff strategy
  - Add retry configuration
  - Add retry budget limiting
  - **Files**: `agentflow-mcp/src/client/retry.rs`
  - **Tests**: Retry behavior tests
  - **Estimate**: 4 hours

### 2.2 Tool Calling Interface

- [ ] **P0** Implement tool calling types
  - Create `Tool` struct
  - Create `CallToolRequest` struct
  - Create `CallToolResult` struct
  - Create `Content` enum (Text, Image, Resource)
  - **Files**: `agentflow-mcp/src/client/tools.rs`
  - **Tests**: Type serialization tests
  - **Estimate**: 3 hours

- [ ] **P0** Implement tool calling interface
  - Add `list_tools()` method
  - Add `call_tool()` method
  - Add tool result parsing
  - **Files**: `agentflow-mcp/src/client/tools.rs`
  - **Tests**: Tool calling integration tests
  - **Estimate**: 4 hours

- [ ] **P1** Add tool input validation
  - Integrate JSON Schema validation
  - Validate tool arguments before calling
  - Add validation error messages
  - **Files**: `agentflow-mcp/src/client/tools.rs`
  - **Tests**: Input validation tests
  - **Estimate**: 3 hours

- [ ] **P1** Add tool calling helpers
  - Create fluent tool call builder
  - Add type-safe parameter helpers
  - Add result extraction helpers
  - **Files**: `agentflow-mcp/src/client/tools.rs`
  - **Tests**: Helper method tests
  - **Estimate**: 3 hours

### 2.3 Resource Access Interface

- [ ] **P0** Implement resource types
  - Create `Resource` struct
  - Create `ReadResourceResult` struct
  - Create `ResourceContent` struct
  - **Files**: `agentflow-mcp/src/client/resources.rs`
  - **Tests**: Type serialization tests
  - **Estimate**: 2 hours

- [ ] **P0** Implement resource interface
  - Add `list_resources()` method
  - Add `read_resource()` method
  - Add resource content parsing
  - **Files**: `agentflow-mcp/src/client/resources.rs`
  - **Tests**: Resource access integration tests
  - **Estimate**: 4 hours

- [ ] **P1** Implement resource subscriptions
  - Add `subscribe_resource()` method
  - Add `unsubscribe_resource()` method
  - Add notification handling for resource updates
  - **Files**: `agentflow-mcp/src/client/resources.rs`
  - **Tests**: Subscription tests
  - **Estimate**: 5 hours

- [ ] **P2** Add resource caching
  - Implement in-memory cache
  - Add cache invalidation on updates
  - Add TTL support
  - **Files**: `agentflow-mcp/src/client/cache.rs`
  - **Tests**: Cache behavior tests
  - **Estimate**: 4 hours

### 2.4 Prompt Template Interface

- [ ] **P0** Implement prompt types
  - Create `Prompt` struct
  - Create `GetPromptRequest` struct
  - Create `GetPromptResult` struct
  - **Files**: `agentflow-mcp/src/client/prompts.rs`
  - **Tests**: Type serialization tests
  - **Estimate**: 2 hours

- [ ] **P0** Implement prompt interface
  - Add `list_prompts()` method
  - Add `get_prompt()` method
  - Add argument substitution
  - **Files**: `agentflow-mcp/src/client/prompts.rs`
  - **Tests**: Prompt access integration tests
  - **Estimate**: 4 hours

- [ ] **P1** Add prompt template rendering
  - Support parameter substitution
  - Add template validation
  - **Files**: `agentflow-mcp/src/client/prompts.rs`
  - **Tests**: Template rendering tests
  - **Estimate**: 3 hours

### 2.5 Client Examples and Documentation

- [ ] **P1** Create simple client example
  - Example connecting to stdio server
  - Example listing and calling tools
  - **Files**: `agentflow-mcp/examples/simple_client.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Create resource access example
  - Example listing and reading resources
  - **Files**: `agentflow-mcp/examples/resource_client.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Write client API documentation
  - Document all public client APIs
  - Add usage examples
  - Add troubleshooting guide
  - **Files**: `agentflow-mcp/src/client/*.rs`
  - **Tests**: Doc tests pass
  - **Estimate**: 4 hours

### 2.6 Integration Testing

- [ ] **P0** Write client integration tests - Tools
  - Test tool discovery
  - Test tool calling
  - Test error handling
  - **Files**: `agentflow-mcp/tests/client/tools_test.rs`
  - **Tests**: 10+ test cases
  - **Estimate**: 4 hours

- [ ] **P0** Write client integration tests - Resources
  - Test resource discovery
  - Test resource reading
  - Test subscriptions
  - **Files**: `agentflow-mcp/tests/client/resources_test.rs`
  - **Tests**: 8+ test cases
  - **Estimate**: 3 hours

- [ ] **P1** Test with real MCP servers
  - Test against reference MCP implementations
  - Document compatibility matrix
  - **Files**: `agentflow-mcp/tests/client/compatibility_test.rs`
  - **Tests**: Compatibility tests
  - **Estimate**: 4 hours

### Phase 2 Milestone Checklist

- [ ] All Phase 2 tasks completed
- [ ] Test coverage ≥ 70%
- [ ] Client API complete and documented
- [ ] Can connect to reference MCP servers
- [ ] All client operations functional
- [ ] Integration tests pass

---

## Phase 3: Server Implementation (Week 5-6)

**Goal**: Complete production-ready MCP server
**Duration**: 10 working days
**Progress**: 0/30 tasks (0%)

### 3.1 Server Core

- [ ] **P0** Create server builder foundation
  - Implement `MCPServerBuilder` struct
  - Add fluent API methods
  - Add transport type selection
  - Add handler registration
  - **Files**: `agentflow-mcp/src/server/builder.rs`
  - **Tests**: Builder API tests
  - **Estimate**: 4 hours

- [ ] **P0** Implement server router
  - Create request routing logic
  - Route to appropriate handlers
  - Handle unknown methods
  - **Files**: `agentflow-mcp/src/server/router.rs`
  - **Tests**: Router tests
  - **Estimate**: 4 hours

- [ ] **P0** Implement server lifecycle
  - Handle initialize request
  - Handle initialized notification
  - Handle shutdown
  - **Files**: `agentflow-mcp/src/server/lifecycle.rs`
  - **Tests**: Lifecycle tests
  - **Estimate**: 3 hours

### 3.2 Handler System

- [ ] **P0** Create handler traits
  - Define `ToolHandler` trait
  - Define `ResourceHandler` trait
  - Define `PromptHandler` trait
  - **Files**: `agentflow-mcp/src/server/handlers/traits.rs`
  - **Tests**: Trait documentation
  - **Estimate**: 2 hours

- [ ] **P0** Implement tool handler registry
  - Create `ToolRegistry` struct
  - Add tool registration
  - Add tool discovery
  - Add tool execution dispatch
  - **Files**: `agentflow-mcp/src/server/handlers/tools.rs`
  - **Tests**: Tool registry tests
  - **Estimate**: 4 hours

- [ ] **P0** Implement resource handler registry
  - Create `ResourceRegistry` struct
  - Add resource registration
  - Add resource discovery
  - Add resource access dispatch
  - **Files**: `agentflow-mcp/src/server/handlers/resources.rs`
  - **Tests**: Resource registry tests
  - **Estimate**: 4 hours

- [ ] **P0** Implement prompt handler registry
  - Create `PromptRegistry` struct
  - Add prompt registration
  - Add prompt discovery
  - Add prompt retrieval dispatch
  - **Files**: `agentflow-mcp/src/server/handlers/prompts.rs`
  - **Tests**: Prompt registry tests
  - **Estimate**: 4 hours

### 3.3 Middleware System

- [ ] **P1** Create middleware trait
  - Define middleware interface
  - Add middleware chain execution
  - **Files**: `agentflow-mcp/src/server/middleware.rs`
  - **Tests**: Middleware chain tests
  - **Estimate**: 3 hours

- [ ] **P1** Implement logging middleware
  - Log incoming requests
  - Log outgoing responses
  - Log errors
  - **Files**: `agentflow-mcp/src/server/middleware/logging.rs`
  - **Tests**: Logging middleware tests
  - **Estimate**: 2 hours

- [ ] **P2** Implement rate limiting middleware
  - Token bucket algorithm
  - Per-client rate limits
  - **Files**: `agentflow-mcp/src/server/middleware/rate_limit.rs`
  - **Tests**: Rate limiting tests
  - **Estimate**: 4 hours

- [ ] **P2** Implement auth middleware stub
  - Define auth trait
  - Add no-op implementation
  - Document future auth strategies
  - **Files**: `agentflow-mcp/src/server/middleware/auth.rs`
  - **Tests**: Auth trait tests
  - **Estimate**: 2 hours

### 3.4 HTTP Server

- [ ] **P0** Implement HTTP transport (basic)
  - Create HTTP client implementation
  - Add POST /messages endpoint logic
  - Add error handling
  - **Files**: `agentflow-mcp/src/transport/http.rs`
  - **Tests**: HTTP client tests
  - **Estimate**: 5 hours

- [ ] **P0** Create HTTP server with Axum
  - Set up Axum router
  - Add POST /messages handler
  - Add error handling
  - **Files**: `agentflow-mcp/src/server/http_server.rs`
  - **Tests**: HTTP server tests
  - **Estimate**: 5 hours

- [ ] **P1** Add SSE support
  - Implement SSE transport
  - Add SSE endpoint in server
  - Handle server-initiated messages
  - **Files**: `agentflow-mcp/src/transport/sse.rs`
  - **Tests**: SSE tests
  - **Estimate**: 6 hours

### 3.5 Server Examples

- [ ] **P1** Create simple stdio server example
  - Example serving tools via stdio
  - **Files**: `agentflow-mcp/examples/simple_server.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Create custom tool handler example
  - Example implementing custom tool
  - **Files**: `agentflow-mcp/examples/custom_tool.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Create HTTP server example
  - Example serving MCP over HTTP
  - **Files**: `agentflow-mcp/examples/http_server.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

### 3.6 Server Testing

- [ ] **P0** Write server integration tests - Tools
  - Test tool handler registration
  - Test tool calling via server
  - Test error handling
  - **Files**: `agentflow-mcp/tests/server/tools_test.rs`
  - **Tests**: 10+ test cases
  - **Estimate**: 4 hours

- [ ] **P0** Write server integration tests - Resources
  - Test resource handler registration
  - Test resource access via server
  - **Files**: `agentflow-mcp/tests/server/resources_test.rs`
  - **Tests**: 8+ test cases
  - **Estimate**: 3 hours

- [ ] **P0** Write server conformance tests
  - Test protocol compliance
  - Test with reference MCP clients
  - **Files**: `agentflow-mcp/tests/server/conformance_test.rs`
  - **Tests**: Protocol conformance
  - **Estimate**: 4 hours

- [ ] **P1** Write HTTP server tests
  - Test HTTP endpoint
  - Test SSE streaming
  - **Files**: `agentflow-mcp/tests/server/http_test.rs`
  - **Tests**: HTTP server tests
  - **Estimate**: 3 hours

### Phase 3 Milestone Checklist

- [ ] All Phase 3 tasks completed
- [ ] Test coverage ≥ 70%
- [ ] Server API complete and documented
- [ ] Can serve MCP via stdio and HTTP
- [ ] Reference clients can connect
- [ ] Server conformance tests pass

---

## Phase 4: Advanced Features (Week 7-8)

**Goal**: Add production-grade features and polish
**Duration**: 10 working days
**Progress**: 0/27 tasks (0%)

### 4.1 JSON Schema Validation

- [ ] **P0** Integrate jsonschema crate
  - Add dependency
  - Create validation helpers
  - **Files**: `agentflow-mcp/src/protocol/validator.rs`
  - **Tests**: Schema validation tests
  - **Estimate**: 2 hours

- [ ] **P0** Implement tool input validation
  - Validate against tool input_schema
  - Add validation error messages
  - **Files**: `agentflow-mcp/src/server/handlers/tools.rs`
  - **Tests**: Input validation tests
  - **Estimate**: 3 hours

- [ ] **P1** Add schema validation for resources
  - Validate resource URIs
  - Validate resource content types
  - **Files**: `agentflow-mcp/src/server/handlers/resources.rs`
  - **Tests**: Resource validation tests
  - **Estimate**: 2 hours

### 4.2 Observability Enhancement

- [ ] **P1** Add Prometheus metrics
  - Instrument client with metrics
  - Instrument server with metrics
  - Track latency, throughput, errors
  - **Files**: `agentflow-mcp/src/observability/metrics.rs`
  - **Tests**: Metrics collection tests
  - **Estimate**: 4 hours

- [ ] **P1** Add distributed tracing support
  - Add OpenTelemetry integration
  - Add trace context propagation
  - Add span creation for operations
  - **Files**: `agentflow-mcp/src/observability/tracing.rs`
  - **Tests**: Tracing tests
  - **Estimate**: 5 hours

- [ ] **P2** Create observability example
  - Example with metrics and tracing
  - **Files**: `agentflow-mcp/examples/observability.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

### 4.3 Resource Subscriptions

- [ ] **P0** Implement resource change notifications
  - Add notification sending from server
  - Handle notifications in client
  - **Files**: Multiple files
  - **Tests**: Notification tests
  - **Estimate**: 5 hours

- [ ] **P0** Add subscription management in server
  - Track subscriptions per client
  - Clean up on disconnect
  - **Files**: `agentflow-mcp/src/server/subscriptions.rs`
  - **Tests**: Subscription management tests
  - **Estimate**: 4 hours

### 4.4 Sampling Support (Optional)

- [ ] **P2** Implement sampling types
  - Create sampling request/response types
  - **Files**: `agentflow-mcp/src/client/sampling.rs`
  - **Tests**: Type tests
  - **Estimate**: 2 hours

- [ ] **P2** Implement sampling interface
  - Add sampling methods to client
  - Add sampling handler to server
  - **Files**: Multiple files
  - **Tests**: Sampling tests
  - **Estimate**: 4 hours

### 4.5 Performance Optimization

- [ ] **P1** Create performance benchmarks
  - Benchmark stdio transport
  - Benchmark HTTP transport
  - Benchmark tool calling
  - **Files**: `agentflow-mcp/benches/transport_bench.rs`
  - **Tests**: Benchmarks run
  - **Estimate**: 4 hours

- [ ] **P1** Optimize transport layer
  - Profile and identify bottlenecks
  - Optimize buffer sizes
  - Reduce allocations
  - **Files**: Transport files
  - **Tests**: Benchmark improvements
  - **Estimate**: 6 hours

- [ ] **P2** Add connection pooling for HTTP
  - Reuse HTTP connections
  - Configure pool size
  - **Files**: `agentflow-mcp/src/transport/http.rs`
  - **Tests**: Connection pool tests
  - **Estimate**: 4 hours

### 4.6 Documentation

- [ ] **P0** Write comprehensive user guide
  - Getting started
  - Client usage guide
  - Server usage guide
  - Best practices
  - **Files**: `agentflow-mcp/docs/USER_GUIDE.md`
  - **Tests**: Documentation review
  - **Estimate**: 8 hours

- [ ] **P0** Write API documentation
  - Document all public APIs
  - Add examples to all public methods
  - **Files**: All source files
  - **Tests**: Doc tests pass
  - **Estimate**: 6 hours

- [ ] **P1** Create migration guide
  - Document breaking changes
  - Provide migration examples
  - **Files**: `agentflow-mcp/docs/MIGRATION.md`
  - **Tests**: Migration examples work
  - **Estimate**: 3 hours

- [ ] **P1** Write troubleshooting guide
  - Common issues and solutions
  - Debug logging
  - **Files**: `agentflow-mcp/docs/TROUBLESHOOTING.md`
  - **Tests**: Documentation review
  - **Estimate**: 3 hours

### 4.7 Examples

- [ ] **P1** Create HTTP with SSE client example
  - Example using SSE transport
  - **Files**: `agentflow-mcp/examples/http_sse_client.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Create resource provider example
  - Example implementing custom resource handler
  - **Files**: `agentflow-mcp/examples/resource_provider.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 2 hours

- [ ] **P1** Create advanced workflow example
  - Example combining tools, resources, and prompts
  - **Files**: `agentflow-mcp/examples/advanced_workflow.rs`
  - **Tests**: Example runs successfully
  - **Estimate**: 3 hours

### Phase 4 Milestone Checklist

- [ ] All Phase 4 tasks completed
- [ ] Test coverage ≥ 80%
- [ ] Full MCP spec compliance
- [ ] Performance benchmarks meet targets
- [ ] Documentation complete
- [ ] All examples work

---

## Phase 5: Integration and Stabilization (Week 9-10)

**Goal**: Integrate with AgentFlow and stabilize
**Duration**: 10 working days
**Progress**: 0/20 tasks (0%)

### 5.1 AgentFlow Integration

- [ ] **P0** Update MCPToolNode to use new client
  - Replace old client usage
  - Use new builder API
  - Update error handling
  - **Files**: `agentflow-agents/src/nodes/mcp_tool_node.rs`
  - **Tests**: Node tests pass
  - **Estimate**: 4 hours

- [ ] **P0** Create MCPServerNode
  - Expose AgentFlow workflows as MCP tools
  - Implement tool handler for workflow execution
  - **Files**: `agentflow-agents/src/nodes/mcp_server_node.rs`
  - **Tests**: Server node tests
  - **Estimate**: 5 hours

- [ ] **P1** Add MCP tool discovery to LLMNode
  - Integrate MCP client with LLM operations
  - Auto-discover available tools
  - Pass tools to LLM as function calling
  - **Files**: `agentflow-nodes/src/nodes/llm.rs`
  - **Tests**: LLM + MCP integration tests
  - **Estimate**: 6 hours

- [ ] **P1** Create hybrid context strategy (initial)
  - Add configuration for MCP tool discovery
  - Implement basic auto-discovery
  - **Files**: `agentflow-llm/src/mcp_integration.rs`
  - **Tests**: Context strategy tests
  - **Estimate**: 5 hours

### 5.2 CLI Integration

- [ ] **P1** Add `agentflow mcp` command group
  - Add `mcp client` subcommand
  - Add `mcp server` subcommand
  - Add `mcp tools list` subcommand
  - **Files**: `agentflow-cli/src/commands/mcp/mod.rs`
  - **Tests**: CLI tests
  - **Estimate**: 4 hours

- [ ] **P1** Add MCP configuration to config file
  - Add MCP server registry
  - Add default MCP servers
  - **Files**: `agentflow-cli/src/config/mod.rs`
  - **Tests**: Config tests
  - **Estimate**: 2 hours

### 5.3 Examples and Workflows

- [ ] **P0** Create workflow example with MCP tools
  - Example workflow using MCP tool node
  - **Files**: `agentflow-cli/examples/mcp_workflow.yml`
  - **Tests**: Workflow runs successfully
  - **Estimate**: 2 hours

- [ ] **P0** Create LLM + MCP example
  - Example of LLM discovering and calling MCP tools
  - **Files**: `agentflow-cli/examples/llm_mcp_integration.yml`
  - **Tests**: Workflow runs successfully
  - **Estimate**: 3 hours

- [ ] **P1** Create AgentFlow server example
  - Expose AgentFlow as MCP server
  - **Files**: `agentflow-cli/examples/agentflow_mcp_server.yml`
  - **Tests**: Server runs and accepts connections
  - **Estimate**: 3 hours

### 5.4 Testing and Stabilization

- [ ] **P0** Write integration tests for MCPToolNode
  - Test node execution
  - Test error handling
  - **Files**: `agentflow-agents/tests/mcp_tool_node_test.rs`
  - **Tests**: Integration tests pass
  - **Estimate**: 3 hours

- [ ] **P0** Write end-to-end workflow tests
  - Test complete workflows with MCP
  - **Files**: `agentflow-cli/tests/mcp_workflows_test.rs`
  - **Tests**: E2E tests pass
  - **Estimate**: 4 hours

- [ ] **P1** Perform load testing
  - Test under concurrent load
  - Identify and fix race conditions
  - **Files**: Various
  - **Tests**: Load tests pass
  - **Estimate**: 4 hours

- [ ] **P1** Fix bugs discovered in integration
  - Address issues from integration testing
  - **Files**: Various
  - **Tests**: All tests pass
  - **Estimate**: 6 hours

### 5.5 Security and Quality

- [ ] **P0** Security audit
  - Review input validation
  - Review error handling
  - Check for injection vulnerabilities
  - Review authentication hooks
  - **Files**: All files
  - **Tests**: Security checklist complete
  - **Estimate**: 6 hours

- [ ] **P1** Performance tuning
  - Optimize based on profiling
  - Tune buffer sizes
  - Optimize allocations
  - **Files**: Various
  - **Tests**: Performance benchmarks improved
  - **Estimate**: 4 hours

- [ ] **P1** Code review and cleanup
  - Remove dead code
  - Fix clippy warnings
  - Improve documentation
  - **Files**: All files
  - **Tests**: Clippy passes, docs build
  - **Estimate**: 4 hours

### 5.6 Release Preparation

- [ ] **P0** Update CHANGELOG.md
  - Document all changes
  - Highlight breaking changes
  - **Files**: `agentflow-mcp/CHANGELOG.md`
  - **Tests**: Changelog review
  - **Estimate**: 2 hours

- [ ] **P0** Update README.md
  - Update installation instructions
  - Add quick start guide
  - Link to documentation
  - **Files**: `agentflow-mcp/README.md`
  - **Tests**: README review
  - **Estimate**: 2 hours

- [ ] **P0** Prepare release announcement
  - Write blog post or release notes
  - Highlight key features
  - **Files**: `docs/MCP_RELEASE_ANNOUNCEMENT.md`
  - **Tests**: Announcement review
  - **Estimate**: 2 hours

### Phase 5 Milestone Checklist

- [ ] All Phase 5 tasks completed
- [ ] Test coverage ≥ 85%
- [ ] All integration tests pass
- [ ] Security review complete
- [ ] Performance targets met
- [ ] Documentation complete
- [ ] Ready for production release

---

## Post-Implementation Tasks

### CI/CD Setup

- [ ] **P1** Create GitHub Actions workflow
  - Add unit tests job
  - Add integration tests job
  - Add conformance tests job
  - Add code coverage reporting
  - **Files**: `.github/workflows/mcp_tests.yml`
  - **Estimate**: 3 hours

- [ ] **P1** Add automated releases
  - Version bumping
  - Changelog generation
  - GitHub releases
  - **Files**: `.github/workflows/release.yml`
  - **Estimate**: 2 hours

### Community and Ecosystem

- [ ] **P2** Create MCP server examples for ecosystem
  - File system MCP server
  - Database MCP server
  - API MCP server
  - **Files**: `examples/servers/`
  - **Estimate**: 8 hours

- [ ] **P2** Write blog post about MCP integration
  - Use cases
  - Architecture decisions
  - Performance insights
  - **Files**: Blog post
  - **Estimate**: 4 hours

---

## Risk Mitigation Plan

### Technical Risks

**Risk**: Official Rust MCP SDK released mid-project
- **Mitigation**: Design for easy migration, monitor SDK progress
- **Contingency**: Evaluate SDK, create migration plan if beneficial

**Risk**: Performance doesn't meet targets
- **Mitigation**: Early benchmarking, iterative optimization
- **Contingency**: Adjust targets or extend Phase 4

**Risk**: Integration issues with AgentFlow
- **Mitigation**: Early integration testing, continuous feedback
- **Contingency**: Adjust interfaces, refactor if needed

### Schedule Risks

**Risk**: Tasks take longer than estimated
- **Mitigation**: Buffer in schedule, prioritize P0 tasks
- **Contingency**: Defer P2 tasks to post-v1.0

**Risk**: Scope creep
- **Mitigation**: Strict task tracking, phase gate reviews
- **Contingency**: Move non-critical features to Phase 6

---

## Success Criteria

### Functional Requirements

- ✅ Full MCP spec 2024-11-05 compliance
- ✅ Stdio transport working and reliable
- ✅ HTTP transport with SSE working
- ✅ All protocol operations functional (tools, resources, prompts)
- ✅ AgentFlow integration complete

### Quality Requirements

- ✅ Test coverage ≥ 85%
- ✅ Zero clippy warnings
- ✅ Zero memory leaks (valgrind)
- ✅ Documentation coverage ≥ 90%
- ✅ Security audit complete

### Performance Requirements

- ✅ Stdio latency < 10ms
- ✅ HTTP latency < 50ms
- ✅ Memory footprint < 10MB
- ✅ Throughput > 1,000 msg/sec

### Documentation Requirements

- ✅ User guide complete
- ✅ API documentation complete
- ✅ Examples working and documented
- ✅ Migration guide complete

---

## Notes

### Development Guidelines

1. **Follow TODO-Driven Development**:
   - Mark tasks as TODO → DONE only after commit
   - Update progress percentages daily
   - Keep this document as single source of truth

2. **Testing Requirements**:
   - Write tests before or alongside code
   - No merging without passing tests
   - Maintain test coverage targets

3. **Documentation**:
   - Document public APIs as you write them
   - Add examples for complex features
   - Update user guide as features are completed

4. **Code Review**:
   - Self-review before marking DONE
   - Check against quality standards
   - Run clippy and fmt before committing

### Progress Tracking

Update this file after each task completion:
1. Change `[ ]` to `[x]` for completed tasks
2. Update progress percentages
3. Update milestone completion status
4. Commit with message referencing this file

### Questions and Decisions

Record major decisions and their rationale:
- **2025-10-27**: Chose Axum over Actix-web for HTTP server (better ecosystem fit)
- **TBD**: Decision on sampling support (may defer to post-v1.0)

---

**Last Updated**: 2025-10-27
**Next Review**: After Phase 1 completion
**Status**: Ready to begin implementation
