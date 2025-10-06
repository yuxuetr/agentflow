# AgentFlow Nodes - Built-in Node Implementations

This crate provides a comprehensive set of ready-to-use nodes for AgentFlow workflows.

## Core Concepts

Each file in `src/nodes` corresponds to an `AsyncNode` implementation. These nodes are the fundamental building blocks of AgentFlow, each performing a specific task, such as calling an AI model, fetching data from a URL, or processing a file.

## Available Nodes

Here is a list of the primary nodes available in this crate:

-   **🤖 LLM & Chat**
    -   `LlmNode`: A versatile node for calling any standard language model for chat or text generation.

-   **🖼️ Image AI**
    -   `ImageUnderstandNode`: Analyzes and describes images using vision-capable models.
    -   `TextToImageNode`: Generates images from a text prompt.
    -   `ImageToImageNode`: Transforms an existing image based on a text prompt.
    -   `ImageEditNode`: Edits a specific region of an image using a text prompt.

-   **🗣️ Audio AI**
    -   `TtsNode`: Converts text to speech (Text-to-Speech).
    -   `AsrNode`: Transcribes audio files into text (Automatic Speech Recognition).

-   **⚙️ Utilities**
    -   `HttpNode`: Makes arbitrary HTTP requests to fetch data from APIs or websites.
    -   `FileNode`: Reads from or writes to the local filesystem.
    -   `TemplateNode`: Renders a string template with variables from the flow's state.

-   **📄 Specialized Content**
    -   `ArxivNode`: Fetches and parses scientific papers from arXiv.org.
    -   `MarkMapNode`: Converts Markdown into an interactive mind map HTML.

-   **🔁 Flow Control**
    -   `BatchNode`: A simple node for batch processing (superseded by the `map` node).
    -   `ConditionalNode`: A simple node for conditional logic (superseded by `run_if`).

## Control Flow Nodes

In addition to the nodes above, the core AgentFlow engine provides powerful control flow capabilities that are configured in your workflow YAML:

-   **`map`**: Executes a sub-workflow for each item in a list. Supports both `sequential` and `parallel` execution.
-   **`while`**: Executes a sub-workflow repeatedly as long as a condition is met.

## Usage Example (Code-First)

Nodes can be used directly in Rust to build workflows programmatically.

```rust
use agentflow_nodes::{LlmNode, TemplateNode};
use agentflow_core::{Flow, GraphNode, NodeType, value::FlowValue};
use std::sync::Arc;
use serde_json::json;

#[tokio::main]
async fn main() {
    // 1. Define the nodes in the workflow
    let template_node = GraphNode {
        id: "get_topic".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new("get_topic", "A short poem about {{topic}}."))),
        initial_inputs: {
            let mut map = std::collections::HashMap::new();
            map.insert("topic".to_string(), FlowValue::Json(json!("the moon")));
            map
        },
        ..
    };

    let llm_node = GraphNode {
        id: "generate_poem".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec!["get_topic".to_string()],
        input_mapping: Some({
            let mut map = std::collections::HashMap::new();
            map.insert("prompt".to_string(), ("get_topic".to_string(), "output".to_string()));
            map
        }),
        ..
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
