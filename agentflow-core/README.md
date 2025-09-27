# AgentFlow Core (V2)

This crate provides the foundational building blocks for the V2 AgentFlow architecture. It contains the traits, structs, and execution engine for creating and running Directed Acyclic Graph (DAG) based workflows with explicit I/O.

## Core Concepts

- **`Flow`**: The main orchestrator. It holds a collection of nodes and is responsible for executing them in the correct order based on their dependencies.

- **`GraphNode`**: A wrapper struct that represents a node within the `Flow`'s graph. It contains the node's implementation, its dependencies, and the mapping of its inputs to the outputs of other nodes.

- **`AsyncNode` Trait**: The core trait that all executable units must implement. It defines a single, stateless `execute` method.

- **`FlowValue` Enum**: A unified data wrapper for all data passed between nodes. It supports simple JSON values as well as references to large data like local files or URLs, preventing unnecessary memory usage.

## V2 Usage Example

```rust
use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    flow::{Flow, GraphNode, NodeType},
    value::FlowValue,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

// 1. Define a custom node that implements the AsyncNode trait.
struct MyNode;
#[async_trait]
impl AsyncNode for MyNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        println!("MyNode executed with inputs: {:?}", inputs);
        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), FlowValue::Json("Hello from MyNode".into()));
        Ok(outputs)
    }
}

// 2. Build a flow using GraphNode.
#[tokio::main]
async fn main() {
    let mut flow = Flow::new();

    let my_node = GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Standard(Arc::new(MyNode)),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: HashMap::new(),
    };

    flow.add_node(my_node);

    // 3. Run the flow.
    let final_state = flow.run().await.unwrap();
    println!("Final state: {:?}", final_state);
}
```
