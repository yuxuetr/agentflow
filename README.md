# AgentFlow V2

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **A modular, Rust-based AI workflow orchestration platform supporting both code-first and configuration-first paradigms with a focus on explicit, type-safe data flow.**

AgentFlow V2 is a complete architectural redesign focused on robustness, clarity, and extensibility. It replaces implicit shared state with a powerful, DAG-based execution model where data flow is explicit and traceable.

## 🏗️ V2 Architecture: Explicit Dataflow

AgentFlow V2 is built on three core principles:

1.  **Directed Acyclic Graph (DAG)**: Workflows are defined as a graph where nodes have explicit dependencies.
2.  **Explicit I/O**: Nodes are stateless and receive all data through typed inputs, producing typed outputs.
3.  **Layered API**: The framework provides different levels of abstraction for different users.

## 🎯 Two Ways to Build

### 💻 **Code-First Approach (Core SDK)**
For developers who need maximum power and type-safety.

```rust
use agentflow_core::{
    flow::{Flow, GraphNode, NodeType},
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;

#[tokio::main]
asyn fn main() {
    // 1. Define the nodes in the workflow
    let template_node = GraphNode {
        id: "get_topic".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new("get_topic", "A short poem about {{topic}}."))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("topic".to_string(), FlowValue::Json(json!("the moon")));
            map
        },
    };

    let llm_node = GraphNode {
        id: "generate_poem".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec!["get_topic".to_string()],
        input_mapping: Some({
            let mut map = HashMap::new();
            map.insert("prompt".to_string(), ("get_topic".to_string(), "output".to_string()));
            map
        }),
        run_if: None,
        initial_inputs: HashMap::new(),
    };

    // 2. Create and run the flow
    let flow = Flow::new(vec![template_node, llm_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    // 3. Safely access the result
    if let Some(Ok(llm_outputs)) = final_state.get("generate_poem") {
        if let Some(FlowValue::Json(serde_json::Value::String(poem))) = llm_outputs.get("output") {
            println!("Generated poem: {}", poem);
        }
    }
}
```

### 📋 **Configuration-First Approach (CLI)**
For users who prefer declarative, dynamic workflows.

```yaml
# workflow_v2.yml
name: "Calculation Pipeline"
nodes:
  - id: initial_values
    type: template
    parameters:
      template: "{{a}} + {{b}}"
      a: 10
      b: 5

  - id: step_1
    type: llm
    dependencies: ["initial_values"]
    input_mapping:
      prompt: "Calculate {{ nodes.initial_values.outputs.output }}"

  - id: step_2
    type: llm
    dependencies: ["step_1"]
    run_if: "{{ nodes.step_1.outputs.is_valid }}" # Conditional execution
    input_mapping:
      prompt: "Summarize the result: {{ nodes.step_1.outputs.result }}"
```

```bash
# Execute via CLI
agentflow workflow run workflow_v2.yml
```

## 🚀 Key V2 Features

- **`FlowValue` Enum**: A unified, type-safe wrapper for multi-modal data (JSON, files, URLs).
- **Stateless Nodes**: Nodes are self-contained and receive all data via an `inputs` map.
- **DAG Execution Engine**: Workflows are defined with explicit `dependencies` for clear, traceable execution.
- **Explicit Input Mapping**: The `input_mapping` field provides full control over data flow between nodes.
- **Powerful Control Flow**: Native support for conditional execution (`run_if`), `while` loops, and `map` iteration (with parallel execution support).
- **File-based Persistence**: Each workflow run is saved to a unique directory for debugging and auditing.

## ✨ New in v0.2.0: Production-Ready Stability

AgentFlow v0.2.0 introduces comprehensive stability and observability improvements for production workflows:

### 🔄 Retry Mechanism
Automatic retry with configurable strategies for handling transient failures.

```rust
use agentflow_core::{RetryPolicy, RetryStrategy, execute_with_retry};

let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::ExponentialBackoff {
        initial_delay_ms: 100,
        max_delay_ms: 5000,
        multiplier: 2.0,
        jitter: true,
    })
    .build();

let result = execute_with_retry(&policy, "api_call", || async {
    api_client.fetch_data().await
}).await?;
```

### 📝 Error Context Enhancement
Detailed error tracking with full execution context and history.

```rust
use agentflow_core::{execute_with_retry_and_context, ErrorContext};

let result = execute_with_retry_and_context(
    &policy, "run_123", "process_node", Some("processor"),
    || async { workflow.execute().await }
).await;

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err((error, context)) => {
        eprintln!("{}", context.detailed_report());
    }
}
```

### 🔍 Workflow Debugging Tools
Interactive workflow debugging and inspection via CLI.

```bash
# Validate workflow configuration
agentflow workflow debug workflow.yml --validate

# Visualize DAG structure
agentflow workflow debug workflow.yml --visualize

# Analyze complexity and bottlenecks
agentflow workflow debug workflow.yml --analyze

# Dry-run without execution
agentflow workflow debug workflow.yml --dry-run --verbose
```

### 💾 Resource Management
Configurable memory limits with automatic cleanup and monitoring.

```rust
use agentflow_core::{ResourceLimits, StateMonitor};

let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)  // 100 MB
    .max_value_size(10 * 1024 * 1024)   // 10 MB
    .cleanup_threshold(0.8)              // Clean at 80%
    .auto_cleanup(true)
    .build();

let monitor = StateMonitor::new(limits);

// Track allocations
monitor.record_allocation("data", data.len());

// Automatic cleanup when needed
if monitor.should_cleanup() {
    monitor.cleanup(0.5)?;  // Clean to 50%
}
```

### 📊 Performance Guarantees

All features meet strict performance targets:
- Retry overhead: **< 5ms** per retry ✅
- Resource limit enforcement: **< 100μs** per operation ✅
- Error context creation: **< 1ms** ✅
- State monitor operations: **< 10μs** ✅
- Combined overhead: **< 1ms** ✅

### 🎯 Production-Ready Metrics

- **74 tests** (49 unit + 12 integration + 9 benchmarks + 4 doc) - **100% passing**
- **Zero breaking changes** - Fully backward compatible
- **Zero compilation warnings**
- **3,600+ lines** of comprehensive documentation
- **4,670+ lines** of production-ready code

## 📚 Documentation

### Core Documentation
- **[V2 Architecture](ARCHITECTURE.md)**: The complete technical design for the new architecture.
- **[V1 to V2 Migration Guide](MIGRATION_V2.md)**: A step-by-step guide for updating old workflows and nodes.

### v0.2.0 Feature Guides
- **[Retry Mechanism](docs/RETRY_MECHANISM.md)**: Comprehensive guide to retry configuration and strategies
- **[Workflow Debugging](docs/WORKFLOW_DEBUGGING.md)**: CLI debugging tools and workflow visualization
- **[Resource Management](docs/RESOURCE_MANAGEMENT.md)**: Memory limits, monitoring, and automatic cleanup
- **[Migration Guide v0.2.0](docs/MIGRATION_GUIDE_v0.2.0.md)**: Upgrade guide from v0.1.0 to v0.2.0
- **[Release Notes v0.2.0](docs/RELEASE_NOTES_v0.2.0.md)**: Complete changelog and improvements

### Examples
- `agentflow-core/examples/retry_example.rs`: Retry mechanism demonstrations
- `agentflow-core/examples/resource_management_example.rs`: Resource monitoring examples
- `agentflow-cli/examples/workflows/`: Complete workflow examples including AI research assistant

## 📦 Installation

### For Developers (Core SDK)
```toml
[dependencies]
agentflow-core = { path = "agentflow-core" }
agentflow-nodes = { path = "agentflow-nodes" }
```

### For CLI Usage
```bash
cargo install --path agentflow-cli
agentflow --help
```

## 🛣️ Development Plan

See our progress and upcoming tasks in the [TODOs.md](TODOs.md) file (note: this file is in `.gitignore`).