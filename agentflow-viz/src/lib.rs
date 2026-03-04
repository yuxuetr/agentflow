//! AgentFlow Visualization Library
//!
//! This crate provides workflow visualization capabilities for AgentFlow,
//! supporting multiple output formats including Mermaid, Graphviz DOT, and JSON.
//!
//! # Quick Start
//!
//! ```rust
//! use agentflow_viz::{VisualGraph, VisualNode, VisualEdge, VisualNodeType};
//! use agentflow_viz::renderers::{render, OutputFormat};
//!
//! // Create a graph programmatically
//! let mut graph = VisualGraph::new("my_workflow", "My Workflow");
//! graph.add_node(VisualNode::standard("start", "Start"));
//! graph.add_node(VisualNode::standard("end", "End"));
//! graph.add_edge(VisualEdge::new("start", "end"));
//!
//! // Render to Mermaid format
//! let mermaid = render(&graph, OutputFormat::Mermaid).unwrap();
//! println!("{}", mermaid);
//! ```
//!
//! # Converting from YAML Workflow Definitions
//!
//! ```rust
//! use agentflow_viz::converter::from_yaml;
//! use agentflow_viz::renderers::{render, OutputFormat};
//!
//! let yaml = r#"
//! name: My Workflow
//! nodes:
//!   - id: start
//!     type: template
//!   - id: process
//!     type: llm
//!     dependencies: [start]
//! "#;
//!
//! let graph = from_yaml(yaml).unwrap();
//! let dot = render(&graph, OutputFormat::Dot).unwrap();
//! ```
//!
//! # Output Formats
//!
//! - **Mermaid**: For embedding in Markdown (GitHub, GitLab, documentation)
//! - **Graphviz DOT**: For generating high-quality PNG/PDF/SVG images
//! - **JSON**: For web frontends and programmatic access
//!
//! # Node Types
//!
//! The library supports various node types with distinct visual representations:
//!
//! - `Standard`: Basic processing node (rectangle)
//! - `Llm`: AI/LLM node (rounded rectangle)
//! - `Template`: Template processing (document shape)
//! - `Map`: Iteration node (folder shape)
//! - `While`: Loop node (diamond)
//! - `Conditional`: Branch node (diamond)
//!
//! # Node Status
//!
//! Nodes can have execution status for real-time visualization:
//!
//! - `Pending`: Not yet executed
//! - `Running`: Currently executing
//! - `Completed`: Successfully completed
//! - `Failed`: Failed with error
//! - `Skipped`: Skipped due to condition

pub mod graph;
pub mod converter;
pub mod renderers;

// Re-export main types for convenience
pub use graph::{
    EdgeType,
    GraphMetadata,
    NodeStatus,
    NodeStyle,
    Position,
    VisualEdge,
    VisualGraph,
    VisualNode,
    VisualNodeType,
};

pub use converter::{
    ConversionError,
    NodeDefinition,
    VisualGraphBuilder,
    WorkflowConverter,
    WorkflowDefinition,
    from_json,
    from_yaml,
};

pub use renderers::{
    GraphDirection,
    GraphRenderer,
    OutputFormat,
    RenderConfig,
    RenderError,
    create_renderer,
    create_renderer_with_config,
    render,
    render_with_config,
};

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_workflow() {
        // Create a workflow graph
        let graph = VisualGraphBuilder::new("test", "Integration Test")
            .add_node("start", "Start")
            .add_typed_node("llm", "LLM Process", VisualNodeType::Llm)
            .add_typed_node("loop", "While", VisualNodeType::While { max_iterations: 10 })
            .add_node("end", "End")
            .add_edge("start", "llm")
            .add_edge("llm", "loop")
            .add_edge("loop", "end")
            .build();

        // Test Mermaid rendering
        let mermaid = render(&graph, OutputFormat::Mermaid).unwrap();
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("start[Start]"));

        // Test DOT rendering
        let dot = render(&graph, OutputFormat::Dot).unwrap();
        assert!(dot.contains("digraph workflow"));
        assert!(dot.contains("start ->"));

        // Test JSON rendering
        let json = render(&graph, OutputFormat::Json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "Integration Test");
    }

    #[test]
    fn test_yaml_to_mermaid() {
        let yaml = r#"
name: Test Pipeline
nodes:
  - id: input
    type: template
  - id: process
    type: llm
    dependencies: [input]
  - id: output
    type: template
    dependencies: [process]
"#;

        let graph = from_yaml(yaml).unwrap();
        let mermaid = render(&graph, OutputFormat::Mermaid).unwrap();

        assert!(mermaid.contains("input"));
        assert!(mermaid.contains("process"));
        assert!(mermaid.contains("output"));
        assert!(mermaid.contains("-->"));
    }
}
