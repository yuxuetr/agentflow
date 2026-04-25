//! Graphviz DOT renderer
//!
//! Renders visual graphs to Graphviz DOT format for generating high-quality
//! images using the Graphviz toolchain (dot, neato, fdp, etc.).

use super::{GraphRenderer, OutputFormat, RenderConfig, RenderError};
use crate::graph::{EdgeType, NodeStatus, VisualGraph, VisualNodeType};

/// Renderer for Graphviz DOT format
pub struct DotRenderer {
  config: RenderConfig,
}

impl DotRenderer {
  /// Create a new DOT renderer with the given configuration
  pub fn new(config: RenderConfig) -> Self {
    Self { config }
  }

  /// Create a new DOT renderer with default configuration
  pub fn default_renderer() -> Self {
    Self::new(RenderConfig::default())
  }

  /// Escape special characters for DOT labels
  fn escape_label(label: &str) -> String {
    label
      .replace('\\', "\\\\")
      .replace('"', "\\\"")
      .replace('\n', "\\n")
  }

  /// Get the DOT shape for a node type
  fn node_shape(node_type: &VisualNodeType) -> &'static str {
    match node_type {
      VisualNodeType::Standard => "box",
      VisualNodeType::Llm => "box", // with rounded corners via style
      VisualNodeType::Template => "note",
      VisualNodeType::Map { .. } => "folder",
      VisualNodeType::While { .. } => "diamond",
      VisualNodeType::Conditional => "diamond",
      VisualNodeType::InputOutput => "parallelogram",
      VisualNodeType::SubGraph { .. } => "box3d",
    }
  }

  /// Get additional DOT style for a node type
  fn node_style(node_type: &VisualNodeType) -> &'static str {
    match node_type {
      VisualNodeType::Llm => "rounded,filled",
      VisualNodeType::Map { parallel: true } => "filled,bold",
      VisualNodeType::Map { parallel: false } => "filled",
      _ => "filled",
    }
  }

  /// Get the fill color for a node based on its status
  fn status_color(status: &NodeStatus) -> &'static str {
    match status {
      NodeStatus::Pending => "#f0f0f0",
      NodeStatus::Running => "#ffd700",
      NodeStatus::Completed => "#90ee90",
      NodeStatus::Failed => "#ff6b6b",
      NodeStatus::Skipped => "#d3d3d3",
    }
  }

  /// Get the border color for a node based on its status
  fn status_border_color(status: &NodeStatus) -> &'static str {
    match status {
      NodeStatus::Pending => "#cccccc",
      NodeStatus::Running => "#ffb000",
      NodeStatus::Completed => "#228b22",
      NodeStatus::Failed => "#dc3545",
      NodeStatus::Skipped => "#808080",
    }
  }

  /// Get the DOT edge style for an edge type
  fn edge_style(edge_type: &EdgeType) -> &'static str {
    match edge_type {
      EdgeType::DataFlow => "solid",
      EdgeType::Conditional => "dashed",
      EdgeType::LoopBack => "dotted",
      EdgeType::Error => "solid",
    }
  }

  /// Get the edge color for an edge type
  fn edge_color(edge_type: &EdgeType) -> &'static str {
    match edge_type {
      EdgeType::DataFlow => "#333333",
      EdgeType::Conditional => "#666666",
      EdgeType::LoopBack => "#999999",
      EdgeType::Error => "#dc3545",
    }
  }
}

impl GraphRenderer for DotRenderer {
  fn render(&self, graph: &VisualGraph) -> Result<String, RenderError> {
    let mut output = String::new();

    // Graph header
    output.push_str("digraph workflow {\n");

    // Graph attributes
    output.push_str(&format!(
      "    rankdir={};\n",
      self.config.direction.dot_rankdir()
    ));
    output.push_str("    splines=ortho;\n");
    output.push_str("    nodesep=0.8;\n");
    output.push_str("    ranksep=0.8;\n");
    output.push('\n');

    // Default node attributes
    output.push_str("    node [\n");
    output.push_str("        fontname=\"Arial\"\n");
    output.push_str("        fontsize=12\n");
    output.push_str("        margin=\"0.2,0.1\"\n");
    output.push_str("    ];\n");
    output.push('\n');

    // Default edge attributes
    output.push_str("    edge [\n");
    output.push_str("        fontname=\"Arial\"\n");
    output.push_str("        fontsize=10\n");
    output.push_str("        arrowsize=0.8\n");
    output.push_str("    ];\n");
    output.push('\n');

    // Add title as a label
    output.push_str(&format!(
      "    label=\"{}\";\n",
      Self::escape_label(&graph.name)
    ));
    output.push_str("    labelloc=t;\n");
    output.push_str("    fontsize=16;\n");
    output.push_str("    fontname=\"Arial Bold\";\n");
    output.push('\n');

    // Render nodes
    for node in &graph.nodes {
      let shape = Self::node_shape(&node.node_type);
      let style = Self::node_style(&node.node_type);
      let label = Self::escape_label(&node.label);

      let (fill_color, border_color) = if self.config.show_status {
        (
          Self::status_color(&node.status),
          Self::status_border_color(&node.status),
        )
      } else {
        ("#ffffff", "#333333")
      };

      output.push_str(&format!(
                "    {} [\n        label=\"{}\"\n        shape={}\n        style=\"{}\"\n        fillcolor=\"{}\"\n        color=\"{}\"\n    ];\n",
                node.id, label, shape, style, fill_color, border_color
            ));
    }

    output.push('\n');

    // Render edges
    for edge in &graph.edges {
      let style = Self::edge_style(&edge.edge_type);
      let color = Self::edge_color(&edge.edge_type);

      let mut attrs = vec![format!("style={}", style), format!("color=\"{}\"", color)];

      if self.config.show_edge_labels {
        if let Some(ref label) = edge.label {
          attrs.push(format!("label=\"{}\"", Self::escape_label(label)));
        }
      }

      output.push_str(&format!(
        "    {} -> {} [{}];\n",
        edge.from,
        edge.to,
        attrs.join(" ")
      ));
    }

    output.push_str("}\n");

    Ok(output)
  }

  fn format(&self) -> OutputFormat {
    OutputFormat::Dot
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::graph::{VisualEdge, VisualNode};

  fn create_test_graph() -> VisualGraph {
    let mut graph = VisualGraph::new("test", "Test Workflow");

    graph.add_node(VisualNode::new("start", "Start", VisualNodeType::Standard));
    graph.add_node(VisualNode::new("llm", "LLM Process", VisualNodeType::Llm));
    graph.add_node(VisualNode::new(
      "loop",
      "While Loop",
      VisualNodeType::While { max_iterations: 10 },
    ));
    graph.add_node(
      VisualNode::new("end", "End", VisualNodeType::Standard).with_status(NodeStatus::Completed),
    );

    graph.add_edge(VisualEdge::new("start", "llm"));
    graph.add_edge(VisualEdge::new("llm", "loop"));
    graph.add_edge(VisualEdge::new("loop", "end").with_type(EdgeType::Conditional));

    graph
  }

  #[test]
  fn test_render_basic_graph() {
    let graph = create_test_graph();
    let renderer = DotRenderer::default_renderer();
    let result = renderer.render(&graph).unwrap();

    // Check basic structure
    assert!(result.contains("digraph workflow"));
    assert!(result.contains("rankdir=TB"));
    assert!(result.contains("start ["));
    assert!(result.contains("shape=box"));
    assert!(result.contains("start -> llm"));
    assert!(result.contains("loop -> end"));
  }

  #[test]
  fn test_render_with_status_colors() {
    let graph = create_test_graph();
    let renderer = DotRenderer::default_renderer();
    let result = renderer.render(&graph).unwrap();

    // The completed node should have green fill
    assert!(result.contains("#90ee90"));
  }

  #[test]
  fn test_render_edge_styles() {
    let graph = create_test_graph();
    let renderer = DotRenderer::default_renderer();
    let result = renderer.render(&graph).unwrap();

    // Conditional edge should be dashed
    assert!(result.contains("style=dashed"));
  }

  #[test]
  fn test_render_different_directions() {
    let graph = create_test_graph();

    let mut config = RenderConfig::default();
    config.direction = super::super::GraphDirection::LeftToRight;

    let renderer = DotRenderer::new(config);
    let result = renderer.render(&graph).unwrap();

    assert!(result.contains("rankdir=LR"));
  }

  #[test]
  fn test_escape_special_characters() {
    assert_eq!(DotRenderer::escape_label("Line1\nLine2"), "Line1\\nLine2");
    assert_eq!(
      DotRenderer::escape_label("Say \"Hello\""),
      "Say \\\"Hello\\\""
    );
  }

  #[test]
  fn test_node_shapes() {
    assert_eq!(DotRenderer::node_shape(&VisualNodeType::Standard), "box");
    assert_eq!(
      DotRenderer::node_shape(&VisualNodeType::While { max_iterations: 10 }),
      "diamond"
    );
    assert_eq!(
      DotRenderer::node_shape(&VisualNodeType::Map { parallel: true }),
      "folder"
    );
    assert_eq!(DotRenderer::node_shape(&VisualNodeType::Template), "note");
  }
}
