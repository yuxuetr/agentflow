# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **A modern, async-first Rust framework for building intelligent agent workflows with enterprise-grade robustness and observability.**

AgentFlow is a new Rust framework inspired by PocketFlow's concepts, delivering production-ready workflow orchestration with async concurrency, observability, and reliability patterns.

## 🚀 Key Features

### ⚡ **Async-First Architecture**

- Built on Tokio runtime for high-performance async execution
- Native support for parallel and batch processing
- Zero-cost abstractions with Rust's ownership model
- Send + Sync compliance for safe concurrency

### 🛡️ **Enterprise Robustness**

- **Circuit Breakers**: Automatic failure detection and recovery
- **Rate Limiting**: Sliding window algorithms for traffic control
- **Retry Policies**: Exponential backoff with jitter
- **Timeout Management**: Graceful degradation under load
- **Resource Pooling**: RAII guards for safe resource management
- **Load Shedding**: Adaptive capacity management

### 📊 **Comprehensive Observability**

- Real-time metrics collection at flow and node levels
- Structured event logging with timestamps and durations
- Performance profiling and bottleneck detection
- Configurable alerting system
- Distributed tracing support
- Integration-ready for monitoring platforms

### 🔄 **Flexible Execution Models**

- **Sequential Flows**: Traditional node-to-node execution
- **Parallel Execution**: Concurrent node processing with `futures::join_all`
- **Batch Processing**: Configurable batch sizes with concurrent batch execution
- **Nested Flows**: Hierarchical workflow composition
- **Conditional Routing**: Dynamic flow control based on runtime state

## 📦 Installation

Add AgentFlow to your `Cargo.toml`:

```toml
[dependencies]
agentflow-core = "0.2.0"
tokio = { version = "1.0", features = ["full"] }
```

## 🎯 Quick Start

### Basic Sequential Flow

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// Define a custom node
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("Preparing greeting for {}", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("Hello, {}!", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // End flow
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let node = Box::new(GreetingNode {
        name: "AgentFlow".to_string()
    });

    let flow = AsyncFlow::new(node);
    let shared = SharedState::new();

    let result = flow.run_async(&shared).await?;
    println!("Flow completed: {:?}", result);

    Ok(())
}
```

### Parallel Execution with Observability

```rust
use agentflow_core::{AsyncFlow, MetricsCollector};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Create nodes for parallel execution
    let nodes = vec![
        Box::new(ProcessingNode { id: "worker_1".to_string() }),
        Box::new(ProcessingNode { id: "worker_2".to_string() }),
        Box::new(ProcessingNode { id: "worker_3".to_string() }),
    ];

    // Set up observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("parallel_processing".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await?;

    // Check metrics
    let execution_count = metrics.get_metric("parallel_processing.execution_count");
    println!("Executions: {:?}", execution_count);

    Ok(())
}
```

### Robust Flow with Circuit Breaker

```rust
use agentflow_core::{CircuitBreaker, TimeoutManager};
use std::time::Duration;

async fn robust_workflow() -> Result<()> {
    // Set up robustness patterns
    let circuit_breaker = CircuitBreaker::new(
        "api_calls".to_string(),
        3, // failure threshold
        Duration::from_secs(30) // recovery timeout
    );

    let timeout_manager = TimeoutManager::new(
        "operations".to_string(),
        Duration::from_secs(10) // default timeout
    );

    // Use in your workflow logic
    let result = circuit_breaker.call(async {
        timeout_manager.execute_with_timeout("api_call", async {
            // Your business logic here
            Ok("Success")
        }).await
    }).await?;

    Ok(())
}
```

## 🏗️ Architecture

AgentFlow is built on four core pillars:

1. **Execution Model**: AsyncNode trait with prep/exec/post lifecycle
2. **Concurrency Control**: Parallel, batch, and nested execution patterns
3. **Robustness Guarantees**: Circuit breakers, retries, timeouts, and resource management
4. **Observability**: Metrics, events, alerting, and distributed tracing

For detailed architecture information, see [docs/design.md](docs/design.md).

## 📚 Documentation

- **[Design Document](docs/design.md)** - System architecture and component diagrams
- **[Functional Specification](docs/functional-spec.md)** - Feature requirements and API specifications
- **[Use Cases](docs/use-cases.md)** - Real-world implementation scenarios
- **[API Reference](docs/api/)** - Complete API documentation
- **[Migration Guide](docs/migration.md)** - Upgrading from PocketFlow

## 🧪 Testing

AgentFlow maintains 100% test coverage with comprehensive test suites:

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific module tests
cargo test async_flow
cargo test robustness
cargo test observability
```

**Current Status**: 67/67 tests passing ✅

## 🚢 Production Readiness

AgentFlow is designed for production environments with:

- **Memory Safety**: Rust's ownership model prevents data races and memory leaks
- **Performance**: Zero-cost abstractions and efficient async runtime
- **Reliability**: Comprehensive error handling and graceful degradation
- **Scalability**: Built-in support for horizontal scaling patterns
- **Monitoring**: Full observability stack for production insights

## 📚 Examples

### 🤖 Featured Example: LLM-Powered Agent Flow

```bash
# Run the comprehensive LLM integration example
cargo run --example simple_agent_llm_flow
```

This example demonstrates:
- **LLM Integration**: Using moonshot API within AgentFlow workflows
- **Intelligent Routing**: Dynamic flow branching based on AI response analysis
- **Response Processing**: Automated sentiment and complexity analysis
- **Context Management**: Dynamic prompt templates with state substitution
- **Full Observability**: Real-time metrics and execution tracing

### 📂 All Examples

See the [examples directory](./examples) for working examples:
- `hello_world.rs` - Basic AsyncNode functionality
- `batch_processing.rs` - Parallel processing patterns  
- `workflow.rs` - Sequential multi-stage workflows
- `chat.rs` - Interactive conversation flows
- `simple_agent_llm_flow.rs` - **LLM integration with intelligent routing**

📚 **[Complete Documentation](./docs/examples/)** with workflow diagrams and detailed guides.

## 🛣️ Roadmap

- **v0.3.0**: MCP (Model Context Protocol) integration
- **v0.4.0**: Distributed execution engine
- **v0.5.0**: WebAssembly plugin system
- **v1.0.0**: Production stability guarantees

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built on the foundation of the original PocketFlow concept
- Inspired by modern distributed systems patterns
- Powered by the Rust ecosystem and Tokio runtime

---

**AgentFlow**: Where intelligent workflows meet enterprise reliability. 🦀✨
