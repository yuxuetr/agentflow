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

## 📚 Documentation

- **[V2 Architecture](ARCHITECTURE.md)**: The complete technical design for the new architecture.
- **[V1 to V2 Migration Guide](MIGRATION_V2.md)**: A step-by-step guide for updating old workflows and nodes.

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