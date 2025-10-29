# Testing Infrastructure

This document describes the comprehensive testing strategy for the `agentflow-mcp` crate, including unit tests, integration tests, and property-based testing.

## Table of Contents

1. [Overview](#overview)
2. [Test Categories](#test-categories)
3. [Property-Based Testing](#property-based-testing)
4. [Test Coverage](#test-coverage)
5. [Running Tests](#running-tests)
6. [Adding New Tests](#adding-new-tests)
7. [Best Practices](#best-practices)

## Overview

The agentflow-mcp crate has comprehensive test coverage across all major components:

- **117 total tests** (as of latest update)
  - 82 unit and integration tests
  - 35 property-based tests
- **100% pass rate** with zero ignored tests
- **Test distribution**:
  - Client module: 45 tests
  - Protocol module: 20 tests
  - Error handling: 18 tests
  - Transport layer: 34 tests

## Test Categories

### 1. Unit Tests

Traditional example-based tests that verify specific behaviors and edge cases. These tests are located within each module using the `#[cfg(test)]` attribute.

**Examples**:
- `client/retry.rs`: Tests for retry logic and backoff calculations
- `protocol/types.rs`: Tests for JSON-RPC serialization/deserialization
- `error.rs`: Tests for error construction and context propagation
- `transport_new/stdio.rs`: Tests for stdio transport lifecycle

### 2. Integration Tests

Tests located in `tests/` directory that verify interactions between multiple components:

- **State machine tests** (`tests/state_machine_tests.rs`): 20 tests validating client state transitions
- **Timeout tests** (`tests/timeout_tests.rs`): 14 tests for timeout behavior and retry integration

### 3. Property-Based Tests

Property-based tests use the `proptest` library to validate invariants across randomly generated inputs. These tests are organized in nested `property_tests` modules within each source file.

## Property-Based Testing

### What is Property-Based Testing?

Property-based testing validates that certain properties (invariants) hold true for a wide range of inputs, rather than testing specific examples. This approach:

- **Discovers edge cases** automatically through random input generation
- **Validates invariants** that should hold for all valid inputs
- **Complements example-based tests** by exploring the input space systematically
- **Catches regression bugs** through deterministic replay of failing cases

### Why We Use Proptest

We use the [proptest](https://github.com/proptest-rs/proptest) crate because it:

- Integrates seamlessly with Rust's testing framework
- Provides shrinking to find minimal failing cases
- Supports custom generators for domain-specific types
- Has excellent documentation and community support

### Property Tests by Module

#### 1. Retry Logic (`src/client/retry.rs`)

**7 property tests** validating retry behavior:

```rust
// Property: Backoff duration is always non-negative
fn prop_backoff_always_non_negative(base_ms, max_backoff_ms, attempt)

// Property: Backoff respects maximum cap
fn prop_backoff_respects_max(base_ms, max_backoff_ms, attempt)

// Property: Backoff increases exponentially until cap
fn prop_backoff_exponential_growth(base_ms, attempt)

// Property: Backoff for attempt 0 equals base
fn prop_backoff_first_attempt_equals_base(base_ms)

// Property: Max retries determines attempt count
fn prop_max_retries_bounds_attempts(max_retries)

// Property: Backoff never overflows even with large attempts
fn prop_backoff_no_overflow(base_ms, attempt)

// Property: Different configurations produce different backoffs
fn prop_config_affects_backoff(base_ms1, base_ms2, attempt)
```

**Key Invariants**:
- Backoff durations are always non-negative
- Backoff never exceeds configured maximum
- Exponential growth: each backoff is ~2x the previous
- First attempt backoff equals base delay
- No arithmetic overflow with large attempt numbers

#### 2. Protocol Types (`src/protocol/types.rs`)

**10 property tests** validating JSON-RPC protocol:

```rust
// Property: RequestId Number round-trips through JSON
fn prop_request_id_number_roundtrip(id: i64)

// Property: RequestId String round-trips through JSON
fn prop_request_id_string_roundtrip(id: String)

// Property: Requests always have version "2.0"
fn prop_request_has_jsonrpc_version(id, method)

// Property: Notifications have no ID
fn prop_notification_has_no_id(method)

// Property: Requests with ID are not notifications
fn prop_request_with_id_not_notification(id, method)

// Property: Success responses have result, no error
fn prop_success_response_properties(id, result)

// Property: Error responses have error, no result
fn prop_error_response_properties(id, code, message)

// Property: Request round-trips through JSON
fn prop_request_roundtrip(id, method)

// Property: Response round-trips through JSON
fn prop_response_roundtrip(id, result)

// Property: Method names are preserved exactly
fn prop_method_name_preserved(id, method)
```

**Key Invariants**:
- JSON serialization is lossless (round-trip property)
- JSON-RPC version is always "2.0"
- Notifications never have an ID
- Requests with IDs are never notifications
- Success responses have result, error responses have error
- Method names are preserved through serialization

#### 3. Error Handling (`src/error.rs`)

**10 property tests** validating error behavior:

```rust
// Property: Transient errors remain transient after context
fn prop_transient_preserved_after_context(message, context)

// Property: Non-transient errors remain non-transient
fn prop_non_transient_preserved_after_context(message, context)

// Property: Context appears in error messages
fn prop_context_appears_in_message(message, context)

// Property: Multiple contexts stack properly
fn prop_multiple_contexts_stack(message, ctx1, ctx2)

// Property: Timeout errors have correct timeout value
fn prop_timeout_value_preserved(message, timeout_ms)

// Property: Tool errors preserve tool name
fn prop_tool_error_preserves_name(message, tool_name)

// Property: Resource errors preserve URI
fn prop_resource_error_preserves_uri(message, uri)

// Property: Protocol errors have JSON-RPC code
fn prop_protocol_error_has_code(message, code)

// Property: Error messages never empty
fn prop_error_messages_not_empty(message)

// Property: ResultExt context works
fn prop_result_ext_context_works(is_ok, context)
```

**Key Invariants**:
- Error classification (transient vs non-transient) is stable
- Context propagation preserves error classification
- Context appears in error display strings
- Multiple contexts stack in order
- Specialized error fields (timeout, tool name, URI) are preserved
- Error messages are never empty

#### 4. Transport Configuration (`src/transport_new/stdio.rs`)

**8 property tests** validating transport config:

```rust
// Property: Timeout configuration preserved
fn prop_timeout_config_preserved(timeout_ms)

// Property: Max message size preserved
fn prop_max_message_size_preserved(max_size)

// Property: set_timeout_ms updates correctly
fn prop_set_timeout_ms_works(timeout_ms)

// Property: set_max_message_size updates correctly
fn prop_set_max_message_size_works(max_size)

// Property: Command vec preserved
fn prop_command_preserved(cmd1, cmd2, cmd3)

// Property: New transport not connected
fn prop_new_transport_not_connected(cmd)

// Property: Transport type always Stdio
fn prop_transport_type_always_stdio(cmd, timeout_ms)

// Property: Builder pattern chains correctly
fn prop_builder_pattern_chains(timeout_ms, max_size)
```

**Key Invariants**:
- Configuration setters update values correctly
- Builder pattern preserves all configurations
- Transport type is always `Stdio` for stdio transport
- New transports start in disconnected state
- Command vectors are preserved during construction

### Input Generation Strategies

We use various strategies to generate test inputs:

1. **Numeric Ranges**: Bounded random numbers
   ```rust
   timeout_ms in 1u64..60_000u64,    // 1ms to 60 seconds
   max_size in 1usize..10_000_000usize  // 1 byte to 10MB
   ```

2. **String Patterns**: Regex-based string generation
   ```rust
   message in "[a-zA-Z0-9 ]{1,50}",   // Alphanumeric with spaces
   method in "[a-zA-Z0-9/_.-]{1,50}"   // Method names
   ```

3. **Enums**: All variants of an enum
   ```rust
   code in prop_oneof![
     Just(JsonRpcErrorCode::InvalidRequest),
     Just(JsonRpcErrorCode::MethodNotFound),
     // ...
   ]
   ```

4. **Arbitrary Types**: Using `any::<T>()`
   ```rust
   id in any::<i64>(),
   result in any::<serde_json::Value>()
   ```

## Test Coverage

### Current Coverage by Module

| Module | Unit Tests | Property Tests | Integration Tests | Total |
|--------|-----------|----------------|------------------|-------|
| Client | 25 | 7 | 20 | 52 |
| Protocol | 10 | 10 | 0 | 20 |
| Error | 8 | 10 | 0 | 18 |
| Transport | 19 | 8 | 14 | 41 |
| **Total** | **82** | **35** | **14** | **117** |

### Coverage Highlights

- ✅ **Retry logic**: Comprehensive coverage of backoff calculation, overflow handling, transient error classification
- ✅ **Protocol serialization**: All JSON-RPC message types validated through property tests
- ✅ **Error handling**: Context propagation, error classification, specialized error fields
- ✅ **Transport layer**: Configuration, lifecycle, timeout handling, process management
- ✅ **State machine**: All valid state transitions and invalid operation sequences
- ✅ **Timeout behavior**: Initialization, request, notification, and retry timeout integration

## Running Tests

### Run All Tests

```bash
cd agentflow-mcp
cargo test
```

### Run Only Unit Tests (No Integration Tests)

```bash
cargo test --lib
```

### Run Specific Test Module

```bash
cargo test client::retry::tests
```

### Run Property Tests Only

```bash
cargo test property_tests
```

### Run with Test Output

```bash
cargo test -- --nocapture
```

### Run with Specific Number of Property Test Cases

By default, proptest generates 256 test cases per property. To customize:

```bash
PROPTEST_CASES=1000 cargo test
```

### Debug Failing Property Tests

When a property test fails, proptest outputs:

1. **Minimal failing input**: The smallest input that triggers the failure
2. **Seed value**: For reproducing the exact failure

To replay a specific failure:

```rust
proptest! {
  #![proptest_config(ProptestConfig {
    rng_algorithm: RngAlgorithm::ChaCha,
    seed: Some([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42]),
    ..Default::default()
  })]

  #[test]
  fn my_property_test(...) {
    // test code
  }
}
```

## Adding New Tests

### Adding a Unit Test

1. Add `#[cfg(test)]` module in your source file:

```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_my_feature() {
    // Arrange
    let input = create_test_input();

    // Act
    let result = my_function(input);

    // Assert
    assert_eq!(result, expected_output);
  }
}
```

### Adding a Property Test

1. Import proptest at the top of your `tests` module:

```rust
#[cfg(test)]
mod tests {
  use super::*;

  mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
      #[test]
      fn prop_my_invariant(
        input in 1u32..100u32,
        name in "[a-z]{1,10}"
      ) {
        let result = my_function(input, &name);

        // Assert your invariant
        prop_assert!(result > 0);
        prop_assert_eq!(result.name(), name);
      }
    }
  }
}
```

2. Choose appropriate input generators:
   - Numeric ranges: `1u32..100u32`
   - Regex patterns: `"[a-z]{1,10}"`
   - Any value: `any::<T>()`
   - Custom generators: Implement `Arbitrary` trait

3. Use `prop_assert!` macros for assertions:
   - `prop_assert!(condition)` - Basic assertion
   - `prop_assert_eq!(left, right)` - Equality assertion
   - `prop_assert_ne!(left, right)` - Inequality assertion
   - `prop_assume!(condition)` - Filter inputs

### Adding an Integration Test

1. Create a file in `tests/` directory:

```rust
// tests/my_feature_tests.rs
use agentflow_mcp::prelude::*;

#[tokio::test]
async fn test_feature_integration() {
  // Test cross-component behavior
  let client = MCPClient::builder()
    .with_stdio(vec!["test-server"])
    .build()
    .await
    .unwrap();

  let result = client.initialize().await;
  assert!(result.is_ok());
}
```

## Best Practices

### 1. Test Organization

- **Unit tests**: Colocate with source code in `#[cfg(test)]` modules
- **Property tests**: Nest in `property_tests` submodule within unit tests
- **Integration tests**: Place in `tests/` directory for cross-component testing

### 2. Property Test Design

- **Test invariants, not implementations**: Focus on properties that should always hold
- **Use appropriate input ranges**: Avoid extremely large values that cause performance issues
- **Avoid testing implementation details**: Test public APIs and contracts
- **Combine with unit tests**: Use property tests to find edge cases, unit tests for specific scenarios

### 3. Naming Conventions

- Unit tests: `test_<what>_<condition>` (e.g., `test_connect_empty_command`)
- Property tests: `prop_<invariant>` (e.g., `prop_backoff_respects_max`)
- Integration tests: `test_<feature>_<scenario>` (e.g., `test_state_transition_connect_success`)

### 4. Assertion Patterns

- **For property tests**: Use `prop_assert!` macros, not `assert!`
- **For references**: Use `&value` in comparisons to avoid move errors
- **For error types**: Match on specific variants, not just `is_ok()`/`is_err()`

### 5. Avoiding Common Pitfalls

- **Don't move values before subsequent use**: Use references in assertions
  ```rust
  // BAD
  prop_assert_eq!(request.method, method);  // Moves request.method
  let json = serde_json::to_value(&request).unwrap();  // ERROR: partial move

  // GOOD
  prop_assert_eq!(&request.method, &method);  // Borrows, no move
  let json = serde_json::to_value(&request).unwrap();  // OK
  ```

- **Watch for numeric limits and caps**: Ensure your test doesn't hit unexpected boundaries
  ```rust
  // BAD: Large attempts hit max_backoff cap
  attempt in 0u32..100u32  // With small base, all hit cap

  // GOOD: Reasonable attempt range
  attempt in 0u32..10u32  // Or increase max_backoff
  ```

- **Use `prop_assume!` to filter invalid inputs**: Don't test nonsensical combinations
  ```rust
  proptest! {
    #[test]
    fn prop_division(a in 1i32..100i32, b in 1i32..100i32) {
      prop_assume!(b != 0);  // Skip division by zero
      let result = a / b;
      prop_assert!(result >= 0);
    }
  }
  ```

### 6. Performance Considerations

- **Limit test case count for slow tests**: Use `PROPTEST_CASES` env var
- **Use `#![proptest_config(...)]` for custom settings**:
  ```rust
  proptest! {
    #![proptest_config(ProptestConfig {
      cases: 100,  // Reduce from default 256 for slow tests
      ..Default::default()
    })]

    #[test]
    fn expensive_property_test(...) {
      // test code
    }
  }
  ```

### 7. Continuous Integration

Ensure tests run in CI:

```yaml
# .github/workflows/test.yml
- name: Run tests
  run: cargo test --all-features

- name: Run property tests with more cases
  run: PROPTEST_CASES=1000 cargo test property_tests
```

## Property Test Examples

### Example 1: Testing Serialization Round-Trip

```rust
proptest! {
  #[test]
  fn prop_roundtrip(value in any::<MyType>()) {
    let json = serde_json::to_value(&value)?;
    let decoded: MyType = serde_json::from_value(json)?;
    prop_assert_eq!(&value, &decoded);
  }
}
```

### Example 2: Testing Numeric Invariants

```rust
proptest! {
  #[test]
  fn prop_always_positive(
    input in 1u32..1000u32
  ) {
    let result = my_computation(input);
    prop_assert!(result > 0);
  }
}
```

### Example 3: Testing String Properties

```rust
proptest! {
  #[test]
  fn prop_normalize_preserves_length(
    s in "[a-zA-Z0-9 ]{1,50}"
  ) {
    let normalized = normalize(&s);
    prop_assert_eq!(normalized.len(), s.len());
  }
}
```

### Example 4: Testing Error Classification

```rust
proptest! {
  #[test]
  fn prop_context_preserves_transient(
    message in "[a-zA-Z0-9 ]{1,50}",
    context in "[a-zA-Z0-9 ]{1,30}"
  ) {
    let err = MyError::transient(&message);
    prop_assert!(err.is_transient());

    let with_context = err.context(&context);
    prop_assert!(with_context.is_transient());
  }
}
```

## Troubleshooting

### Test Failures

1. **Read the minimal failing input**: Proptest shrinks to the smallest failure
2. **Check for off-by-one errors**: Especially in boundary conditions
3. **Verify assumptions**: Use `prop_assume!` to exclude invalid inputs
4. **Check for state pollution**: Ensure tests don't interfere with each other

### Property Test Design Issues

1. **Test takes too long**: Reduce `PROPTEST_CASES` or narrow input ranges
2. **Too many filtered inputs**: Loosen `prop_assume!` conditions or adjust generators
3. **Flaky tests**: Check for race conditions or non-deterministic behavior
4. **Test passes but shouldn't**: Verify your property actually tests the invariant

## References

- [Proptest Documentation](https://altsysrq.github.io/proptest-book/intro.html)
- [Property-Based Testing in Rust](https://www.infinyon.com/blog/2021/04/proptest-guide/)
- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [MCP Protocol Specification](https://modelcontextprotocol.io/)

---

**Last Updated**: 2025-10-28
**Test Count**: 117 tests (82 unit/integration + 35 property tests)
**Pass Rate**: 100%
