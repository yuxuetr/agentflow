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
    flow::{Flow, GraphNode},
    node::{Node, NodeInputs, NodeResult},
    value::FlowValue,
};
use std::collections::HashMap;

// 1. Define a custom node
struct AddNode;
impl Node for AddNode {
    fn execute(&self, inputs: &NodeInputs) -> NodeResult {
        let a = inputs.get("a").unwrap().as_i64().unwrap();
        let b = inputs.get("b").unwrap().as_i64().unwrap();
        let mut outputs = HashMap::new();
        outputs.insert("sum".to_string(), FlowValue::from(a + b));
        Ok(outputs)
    }
}

// 2. Build the flow
let mut flow = Flow::new();
flow.add_node(GraphNode { id: "add_1", node: Box::new(AddNode), .. });
flow.add_node(GraphNode { id: "add_2", node: Box::new(AddNode), dependencies: vec!["add_1"], .. });

// 3. Run the flow
let final_state = flow.run()?;
```

### 📋 **Configuration-First Approach (CLI)**
For users who prefer declarative, dynamic workflows.

```yaml
# workflow_v2.yml
name: "Calculation Pipeline"
nodes:
  - id: initial_values
    type: start # A special node that provides initial values
    parameters:
      a: 10
      b: 5

  - id: step_1
    type: llm
    dependencies: ["initial_values"]
    input_mapping:
      prompt: "Calculate {{ nodes.initial_values.outputs.a }} + {{ nodes.initial_values.outputs.b }}"

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
- **Structured Control Flow**: Native support for conditional execution (`run_if`) and loops (`map` nodes).
- **File-based Persistence**: Each workflow run is saved to a unique directory for debugging and auditing.

## 📚 Documentation

- **[V2 Architecture](ARCHITECTURE.md)**: The complete technical design for the new architecture.
- **[V1 to V2 Migration Guide](MIGRATION_V2.md)**: A step-by-step guide for updating old workflows and nodes.

## 📦 Installation

### For Developers (Core SDK)
```toml
[dependencies]
agentflow-core = { path = "agentflow-core" }
```

### For CLI Usage
```bash
cargo install --path agentflow-cli
agentflow --help
```

## 🛣️ Development Plan

See our progress and upcoming tasks in the [TODOs.md](TODOs.md) file.