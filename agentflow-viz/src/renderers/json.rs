//! JSON renderer
//!
//! Renders visual graphs to JSON format for use with web frontends
//! and other programmatic consumers.

use super::{GraphRenderer, OutputFormat, RenderConfig, RenderError};
use crate::graph::VisualGraph;

/// Renderer for JSON format
pub struct JsonRenderer {
  config: RenderConfig,
}

impl JsonRenderer {
  /// Create a new JSON renderer with the given configuration
  pub fn new(config: RenderConfig) -> Self {
    Self { config }
  }

  /// Create a new JSON renderer with default configuration
  pub fn default_renderer() -> Self {
    Self::new(RenderConfig::default())
  }
}

impl GraphRenderer for JsonRenderer {
  fn render(&self, graph: &VisualGraph) -> Result<String, RenderError> {
    let output = if self.config.pretty_print {
      serde_json::to_string_pretty(graph)
    } else {
      serde_json::to_string(graph)
    };

    output.map_err(|e| RenderError::SerializationError(e.to_string()))
  }

  fn format(&self) -> OutputFormat {
    OutputFormat::Json
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::graph::{NodeStatus, VisualEdge, VisualNode, VisualNodeType};

  fn create_test_graph() -> VisualGraph {
    let mut graph = VisualGraph::new("test", "Test Workflow");

    graph.add_node(VisualNode::new("start", "Start", VisualNodeType::Standard));
    graph.add_node(VisualNode::new("llm", "LLM Process", VisualNodeType::Llm));
    graph.add_node(
      VisualNode::new("end", "End", VisualNodeType::Standard).with_status(NodeStatus::Completed),
    );

    graph.add_edge(VisualEdge::new("start", "llm"));
    graph.add_edge(VisualEdge::new("llm", "end"));

    graph
  }

  #[test]
  fn test_render_json() {
    let graph = create_test_graph();
    let renderer = JsonRenderer::default_renderer();
    let result = renderer.render(&graph).unwrap();

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_eq!(parsed["name"], "Test Workflow");
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(parsed["edges"].as_array().unwrap().len(), 2);
  }

  #[test]
  fn test_render_compact_json() {
    let graph = create_test_graph();

    let mut config = RenderConfig::default();
    config.pretty_print = false;

    let renderer = JsonRenderer::new(config);
    let result = renderer.render(&graph).unwrap();

    // Compact JSON should not have newlines
    assert!(!result.contains('\n'));
  }

  #[test]
  fn test_render_preserves_status() {
    let graph = create_test_graph();
    let renderer = JsonRenderer::default_renderer();
    let result = renderer.render(&graph).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    // Find the "end" node and check its status
    let nodes = parsed["nodes"].as_array().unwrap();
    let end_node = nodes.iter().find(|n| n["id"] == "end").unwrap();
    assert_eq!(end_node["status"], "completed");
  }
}
