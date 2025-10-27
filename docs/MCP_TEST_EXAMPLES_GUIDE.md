# AgentFlow MCP - Testing & Examples Guide

**Date**: 2025-10-27
**Status**: ✅ COMPLETED
**Purpose**: Comprehensive testing infrastructure and usage examples for MCP client

---

## Overview

This guide documents the testing infrastructure and examples created for the AgentFlow MCP client. It includes:
- **MockTransport** for testing without real servers
- **11 integration tests** covering all client functionality
- **2 practical examples** demonstrating real-world usage
- **Testing patterns** and best practices

---

## MockTransport

**File**: `src/transport_new/mock.rs` (371 lines)

### Purpose

MockTransport simulates MCP server responses for testing purposes, allowing you to:
- Test client behavior without running a real MCP server
- Control exact responses for predictable testing
- Verify requests sent by the client
- Test error scenarios

### Key Features

1. **Response Queue**
   - Pre-configure responses with `add_response()`
   - Responses returned in FIFO order
   - `add_responses()` for multiple responses

2. **Message Tracking**
   - Records all sent messages and notifications
   - `sent_messages()` returns all sent messages
   - `last_sent_message()` returns the most recent

3. **Standard Response Helpers**
   - `standard_initialize_response()` - MCP initialization
   - `tools_list_response(tools)` - Tool discovery
   - `tool_call_response(content)` - Tool execution
   - `resources_list_response(resources)` - Resource listing
   - `resource_read_response(contents)` - Resource reading
   - `prompts_list_response(prompts)` - Prompt listing
   - `prompt_get_response(messages)` - Prompt retrieval

### Usage Example

```rust
use agentflow_mcp::transport_new::MockTransport;
use serde_json::json;

// Create mock transport
let mut transport = MockTransport::new();

// Add initialize response
transport.add_response(MockTransport::standard_initialize_response());

// Add tool list response
transport.add_response(MockTransport::tools_list_response(vec![
  json!({
    "name": "add_numbers",
    "description": "Add two numbers",
    "inputSchema": {
      "type": "object",
      "properties": {
        "a": {"type": "number"},
        "b": {"type": "number"}
      }
    }
  })
]));

// Use with client
let client = ClientBuilder::new()
  .with_transport(transport)
  .build()
  .await?;

client.connect().await?;
let tools = client.list_tools().await?;
```

### Tests

MockTransport includes 4 unit tests:
- `test_mock_transport_connect` - Connection lifecycle
- `test_mock_transport_send_message` - Message sending/receiving
- `test_mock_transport_multiple_responses` - Response queuing
- `test_standard_responses` - Helper methods

---

## Integration Tests

**File**: `tests/client_integration.rs` (400+ lines)

### Test Coverage

**11 comprehensive integration tests** covering all client functionality:

#### 1. Client Initialization (`test_client_initialization`)
- Tests connection and initialization handshake
- Verifies server info extraction
- Checks connection state

#### 2. Tool Discovery (`test_list_tools`)
- Lists tools from mock server
- Verifies tool metadata parsing
- Checks input schema deserialization

#### 3. Tool Calling (`test_call_tool`)
- Executes tool with arguments
- Extracts result content
- Verifies text extraction helpers

#### 4. Tool Error Handling (`test_call_tool_error`)
- Tests error responses from tools
- Checks `isError` flag handling
- Verifies error detection

#### 5. Resource Discovery (`test_list_resources`)
- Lists resources from mock server
- Parses resource metadata
- Checks URI and MIME types

#### 6. Resource Reading (`test_read_resource`)
- Reads resource content
- Extracts text content
- Verifies content type detection

#### 7. Prompt Discovery (`test_list_prompts`)
- Lists prompts from mock server
- Parses prompt arguments
- Checks required argument detection

#### 8. Prompt Retrieval (`test_get_prompt`)
- Gets prompt with arguments
- Parses multi-turn conversations
- Verifies message roles

#### 9. Builder Configuration (`test_builder_configuration`)
- Tests fluent builder API
- Verifies configuration setting
- Checks session ID generation

#### 10. Builder Validation (`test_builder_missing_transport`)
- Tests validation errors
- Ensures proper error types
- Verifies build() validation

#### 11. Disconnection (`test_disconnect`)
- Tests graceful disconnection
- Verifies state cleanup
- Checks is_connected() state

### Running Integration Tests

```bash
# Run all integration tests
cargo test --test client_integration

# Run specific test
cargo test --test client_integration test_call_tool

# With output
cargo test --test client_integration -- --nocapture
```

### Test Statistics

- **Tests**: 11 integration tests
- **Pass Rate**: 100%
- **Coverage**: All major client features
- **Execution Time**: <1 second

---

## Examples

### 1. Simple Client Example

**File**: `examples/simple_client.rs` (280 lines)

#### Purpose

Demonstrates complete client usage workflow:
- Connecting to MCP servers (real or mock)
- Listing and calling tools
- Listing and reading resources
- Listing and getting prompts
- Graceful disconnection

#### Usage

```bash
# With mock transport (for testing)
cargo run --example simple_client -- --mock

# With real MCP server
cargo run --example simple_client -- npx -y @modelcontextprotocol/server-everything

# With local server
cargo run --example simple_client -- node server.js
```

#### Output Sample

```
=== AgentFlow MCP Client Example ===

Using mock transport

Connecting to server...
✓ Connected and initialized

Server: mock-server v1.0.0
Capabilities: {"resources":{},"tools":{}}

--- Tools ---
  • add_numbers - Some("Add two numbers together")

Calling tool 'add_numbers'...
Result: The sum is 8

--- Resources ---
  • example.txt - Some("An example file")

Reading resource 'file:///example.txt'...
Content (first 200 chars): This is an example file...

--- Prompts ---
  • code_review - Some("Review code for best practices")
    Arguments:
      - code (required) - Some("The code to review")

Getting prompt 'code_review'...
Messages: 2 total
  Message 1: User - Please review this code...
  Message 2: Assistant - I'll review the code...

--- Disconnecting ---
✓ Disconnected
```

#### Key Features Demonstrated

1. **Flexible Transport**
   - Can use mock or real servers
   - Command-line argument parsing
   - Error handling for missing args

2. **Complete Workflow**
   - Connection and initialization
   - Server info display
   - All MCP operations (tools, resources, prompts)
   - Proper cleanup

3. **Error Handling**
   - Graceful error messages
   - Continues on errors
   - User-friendly output

---

### 2. Retry Example

**File**: `examples/retry_example.rs` (250 lines)

#### Purpose

Demonstrates retry and error handling:
- Exponential backoff configuration
- Error classification (transient vs fatal)
- Custom retry strategies
- Integration with client operations

#### Usage

```bash
cargo run --example retry_example
```

#### Output Sample

```
=== AgentFlow MCP Retry Example ===

--- Example 1: Default Retry Configuration ---
Retry config: max_retries=3, backoff_base=100ms
  Attempt 1
  Attempt 2
  Attempt 3
✓ Success after 3 attempts: 42

--- Example 2: Custom Retry Configuration ---
Custom retry config: max_retries=5, backoff_base=50ms, max_backoff=2000ms
Backoff progression:
  Attempt 0: 50ms backoff
  Attempt 1: 100ms backoff
  Attempt 2: 200ms backoff
  Attempt 3: 400ms backoff
  Attempt 4: 800ms backoff
  Attempt 5: 1600ms backoff
✓ Success: Success!

--- Example 3: Error Classification ---
Testing transient vs non-transient errors

Test 1: Transient error (Timeout)
  Attempts: 4
  Result: Failed (expected: 4 attempts - initial + 3 retries)

Test 2: Non-transient error (Protocol)
  Attempts: 1
  Result: Failed (expected: 1 attempt - no retries for protocol errors)
```

#### Key Concepts Demonstrated

1. **Retry Configuration**
   - Default configuration (3 retries, 100ms base)
   - Custom configuration with max backoff
   - Backoff progression calculation

2. **Error Classification**
   - Transient errors retry automatically
   - Non-transient errors fail immediately
   - Error type detection

3. **Integration Patterns**
   - Wrapping client operations in retry logic
   - Configuring retry per operation
   - Handling retry exhaustion

---

## Testing Patterns

### Pattern 1: Basic Client Test

```rust
#[tokio::test]
async fn test_basic_operation() {
  // Setup mock transport
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(/* operation response */);

  // Build and connect client
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  // Perform operation
  let result = client.some_operation().await.unwrap();

  // Verify result
  assert_eq!(result, expected);
}
```

### Pattern 2: Error Testing

```rust
#[tokio::test]
async fn test_error_handling() {
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(json!({
    "jsonrpc": "2.0",
    "id": 2,
    "error": {
      "code": -32600,
      "message": "Invalid request"
    }
  }));

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();

  let result = client.some_operation().await;
  assert!(result.is_err());
}
```

### Pattern 3: Retry Testing

```rust
#[tokio::test]
async fn test_retry_behavior() {
  let attempt_count = Arc::new(AtomicU32::new(0));
  let attempt_count_clone = attempt_count.clone();

  let config = RetryConfig::new(3, 10);
  let result = retry_with_backoff(&config, || {
    let count = attempt_count_clone.clone();
    async move {
      let current = count.fetch_add(1, Ordering::SeqCst);
      if current < 2 {
        Err(MCPError::timeout("Retry me", None))
      } else {
        Ok(42)
      }
    }
  })
  .await;

  assert!(result.is_ok());
  assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
}
```

---

## Best Practices

### For Testing

1. **Use MockTransport for Unit Tests**
   - Fast, predictable, no external dependencies
   - Test error scenarios easily
   - Verify request structure

2. **Test Error Paths**
   - Test both success and failure cases
   - Verify error messages
   - Check error type classification

3. **Keep Tests Focused**
   - One concept per test
   - Clear test names
   - Minimal setup code

4. **Use Test Helpers**
   - Standard response builders
   - Common setup functions
   - Shared fixtures

### For Examples

1. **Make Examples Runnable**
   - Clear usage instructions
   - Default to mock mode for testing
   - Handle command-line arguments

2. **Show Real Workflows**
   - Complete end-to-end scenarios
   - Error handling
   - Proper cleanup

3. **Document Output**
   - Show expected output
   - Explain what's happening
   - Highlight key points

4. **Keep Examples Simple**
   - Focus on one concept
   - Minimal dependencies
   - Clear code structure

---

## Summary Statistics

### Test Infrastructure
- **MockTransport**: 371 lines, 4 unit tests
- **Integration Tests**: 11 tests, 400+ lines
- **Total Test Coverage**: 72 tests (61 unit + 11 integration)
- **Pass Rate**: 100%

### Examples
- **Simple Client**: 280 lines, full workflow demo
- **Retry Example**: 250 lines, error handling demo
- **Both Examples**: Working with mock and real servers

### Quality Metrics
- ✅ All tests passing
- ✅ Examples compile without errors
- ✅ Comprehensive documentation
- ✅ Multiple usage patterns demonstrated

---

## Next Steps

1. **Add More Examples**
   - Resource subscription example
   - Multi-server example
   - Error recovery patterns

2. **Performance Tests**
   - Benchmark retry overhead
   - Measure latency
   - Test concurrent operations

3. **Integration with Real Servers**
   - Test against official MCP servers
   - Document compatibility
   - Create server examples

---

**Document Author**: Claude (AgentFlow MCP Team)
**Last Updated**: 2025-10-27
**Related Documents**:
- MCP_PHASE2_COMPLETION_REPORT.md
- MCP_PRODUCTION_DESIGN.md
- MCP_IMPLEMENTATION_TODOs.md
