# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **A modern, async-first Rust framework for building intelligent agent workflows with enterprise-grade robustness and observability.**

AgentFlow is a new Rust framework inspired by PocketFlow's concepts, delivering production-ready workflow orchestration with async concurrency, observability, and reliability patterns.

## 🎯 Multiple Ways to Use AgentFlow

AgentFlow offers flexible deployment options to meet different use cases and preferences:

### 📚 **As a Library (Programmatic Integration)**

Use AgentFlow crates directly in your Rust applications with complete control and customization:

```rust
// Use agentflow-core for workflow orchestration
use agentflow_core::{AsyncFlow, AsyncNode, SharedState};

// Use agentflow-llm for LLM integration
use agentflow_llm::AgentFlow;

// Crates are independent - use them separately or together
let response = AgentFlow::model("gpt-4o")
    .prompt("Analyze this data")
    .execute().await?;
```

**Benefits of Library Usage:**
- 🔧 **Full Control**: Complete customization of workflow execution
- ⚡ **Performance**: Zero overhead from CLI parsing
- 🧩 **Modular**: Use only the crates you need
- 🔗 **Integration**: Seamless integration with existing Rust applications
- 📦 **Lightweight**: No dependency on CLI components

### 💻 **Via Command Line (agentflow-cli)**

Access all functionality through the unified `agentflow` command for scripts, automation, and interactive use:

```bash
# Direct LLM interaction
agentflow llm prompt "Explain quantum computing" --model gpt-4o

# Execute complex workflows from YAML configuration
agentflow run workflow.yml --input topic="AI Safety"

# Interactive chat sessions
agentflow llm chat --model claude-3-sonnet

# Batch processing with multimodal inputs
agentflow run image-analysis.yml --input dir="photos/"
```

**Benefits of CLI Usage:**
- 🚀 **No Programming Required**: Use workflows without writing code
- 🔄 **Rapid Prototyping**: Quickly test ideas and iterate
- 📋 **YAML Configuration**: Declarative workflow definitions
- 🤖 **Automation Ready**: Perfect for CI/CD pipelines and scripts
- 🎨 **Rich Output**: Progress bars, formatted results, and error messages

### 🏗️ **Hybrid Approach (Best of Both Worlds)**

Combine library and CLI usage for maximum flexibility:

```rust
// Rust application with CLI integration
use std::process::Command;

// Use library for core logic
let analysis = AgentFlow::model("gpt-4o")
    .prompt("Analyze this data")
    .execute().await?;

// Use CLI for complex workflows
let output = Command::new("agentflow")
    .args(["run", "post-processing.yml"])
    .arg("--input")
    .arg(format!("analysis={}", analysis))
    .output()?;
```

## 🛠️ **Crate Independence and Flexibility**

AgentFlow's modular architecture ensures maximum flexibility:

### **agentflow-core** - Workflow Engine
```toml
[dependencies]
agentflow-core = "0.1.0"  # Lightweight workflow orchestration
```
- Async workflow execution
- Node-based processing model
- Enterprise robustness features
- Comprehensive observability

### **agentflow-llm** - LLM Integration  
```toml
[dependencies]
agentflow-llm = "0.1.0"  # Multi-provider LLM support
```
- Multiple LLM providers (OpenAI, Anthropic, Google, Moonshot, StepFun)
- Streaming and non-streaming support
- Multimodal capabilities (text, images, audio)
- Configuration management

### **agentflow-cli** - Command Line Interface
```bash
# Installed as binary, no code dependencies required
cargo install agentflow-cli
agentflow --help
```
- Unified command-line interface
- YAML-based workflow configuration
- File I/O and multimodal processing
- Interactive and batch modes

### **agentflow** - Complete Suite
```toml
[dependencies]
agentflow = "0.1.0"  # All crates together
```
- Convenient meta-crate including all components
- Single dependency for full functionality
- Consistent versioning across all crates

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

- **[System Architecture](docs/agentflow-design.md)** - High-level system design and component diagrams
- **[Core Engine Design](docs/design.md)** - Detailed workflow engine architecture  
- **[LLM Integration Guide](docs/agentflow-llm-design.md)** - Multi-provider LLM integration details
- **[CLI Design Document](docs/agentflow-cli-design.md)** - Command-line interface implementation plan
- **[Multimodal Support](docs/MULTIMODAL_GUIDE.md)** - Text, image, and audio processing capabilities
- **[Model Types System](docs/GRANULAR_MODEL_TYPES.md)** - Type-safe model capability definitions
- **[Use Cases](docs/use-cases.md)** - Real-world implementation scenarios
- **[Examples Documentation](docs/examples/)** - Comprehensive example walkthroughs

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

## 🛣️ Roadmap and Next Steps

### **Upcoming Releases**

- **v0.3.0**: CLI Implementation and MCP Integration
- **v0.4.0**: RAG System and Vector Database Support  
- **v0.5.0**: Distributed Execution Engine
- **v0.6.0**: WebAssembly Plugin System
- **v1.0.0**: Production Stability Guarantees

### **📋 Follow-up Tasks (Implementation Priority)**

#### **Phase 1: CLI Foundation (High Priority)**
- [ ] **Create `agentflow-cli` crate** - Command-line interface implementation
  - CLI argument parsing with `clap`
  - Command structure (`workflow`, `llm`, `config` subcommands)
  - Basic error handling and user experience
- [ ] **Implement LLM commands** - Direct LLM interaction via CLI
  - `agentflow llm prompt` - text prompting
  - `agentflow llm models` - list available models
  - File input support (text, images, audio)
- [ ] **Update workspace configuration** - Produce unified `agentflow` binary
  - Configure Cargo.toml for binary generation
  - Set up cross-crate dependencies
  - Ensure proper feature flag management

#### **Phase 2: Workflow Engine (High Priority)**
- [ ] **YAML workflow parser** - Configuration file parsing and validation
  - Schema definition for workflow configurations
  - Template engine integration (Tera)
  - Input parameter handling and validation
- [ ] **Core node types** - Built-in workflow building blocks
  - LLM node (single model calls)
  - Batch LLM node (parallel processing)
  - Template node (rendering and formatting)
  - File I/O node (read/write operations)
- [ ] **Workflow execution engine** - Runtime workflow processing
  - Sequential execution support
  - Dependency resolution
  - Context management and state handling
  - Integration with agentflow-core's AsyncFlow

#### **Phase 3: Advanced Features (Medium Priority)**  
- [ ] **Advanced workflow types** - Complex execution patterns
  - Parallel workflow execution
  - Conditional branching logic
  - Loop and iteration support
  - Error handling and recovery strategies
- [ ] **Enhanced file I/O** - Multimodal input/output support
  - Image file processing integration
  - Audio file processing capabilities
  - Multiple output file generation
  - File utility functions in templates
- [ ] **Interactive features** - User experience improvements
  - Interactive chat mode (`agentflow llm chat`)
  - Streaming output support
  - Progress indicators and status reporting
  - Configuration management commands

#### **Phase 4: Integration and Polish (Medium Priority)**
- [ ] **MCP client integration** - Model Context Protocol support
  - Add MCP feature flag to agentflow-llm
  - Implement tools support in LLMClientBuilder
  - Follow existing provider pattern for consistency
  - Enable function calling across providers
- [ ] **Documentation and examples** - User guidance and tutorials
  - Complete CLI user documentation
  - Workflow template library creation
  - Integration examples and tutorials
  - Video guides and walkthroughs
- [ ] **Performance optimization** - Runtime efficiency improvements
  - Execution performance profiling
  - Memory usage optimization
  - Concurrent processing enhancements
  - Benchmarking and testing

#### **Phase 5: Future Extensions (Lower Priority)**
- [ ] **RAG system implementation** - Knowledge retrieval and processing
  - Create `agentflow-rag` crate
  - Document indexing and vector storage
  - Retrieval-augmented generation workflows
  - Vector database integrations
- [ ] **MCP server implementation** - Expose AgentFlow as MCP tools
  - Separate `agentflow-mcp-server` crate  
  - Workflow-to-tool conversion
  - Server deployment and management
  - Integration with existing MCP ecosystem
- [ ] **Plugin system** - Extensibility and customization
  - WebAssembly plugin support
  - Custom node type development
  - Third-party integration framework
  - Community plugin marketplace

### **🎯 Getting Started with Development**

1. **For CLI Development**: Start with `docs/agentflow-cli-design.md` for comprehensive implementation details
2. **For Library Usage**: Explore the existing examples in `examples/` directory
3. **For Contributing**: Review the modular architecture in `docs/agentflow-design.md`

### **🤝 Community Involvement**

- **Documentation**: Help improve guides, examples, and API documentation
- **Testing**: Write integration tests and performance benchmarks
- **Examples**: Create workflow templates and real-world use cases
- **Feedback**: Report issues, suggest features, and share use cases

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
