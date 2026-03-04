//! Mermaid diagram renderer
//!
//! Renders visual graphs to Mermaid format, which is widely supported in
//! Markdown renderers (GitHub, GitLab, etc.) and documentation tools.

use super::{GraphRenderer, OutputFormat, RenderConfig, RenderError};
use crate::graph::{EdgeType, NodeStatus, VisualGraph, VisualNodeType};

/// Renderer for Mermaid diagram format
pub struct MermaidRenderer {
    config: RenderConfig,
}

impl MermaidRenderer {
    /// Create a new Mermaid renderer with the given configuration
    pub fn new(config: RenderConfig) -> Self {
        Self { config }
    }

    /// Create a new Mermaid renderer with default configuration
    pub fn default_renderer() -> Self {
        Self::new(RenderConfig::default())
    }

    /// Escape special characters for Mermaid
    fn escape_label(label: &str) -> String {
        label
            .replace('"', "#quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br/>")
    }

    /// Get the Mermaid shape syntax for a node type
    fn node_shape(node_type: &VisualNodeType, id: &str, label: &str) -> String {
        let escaped_label = Self::escape_label(label);
        match node_type {
            VisualNodeType::Standard => format!("{}[{}]", id, escaped_label),
            VisualNodeType::Llm => format!("{}([{}])", id, escaped_label),
            VisualNodeType::Template => format!("{}[/{}\\]", id, escaped_label),
            VisualNodeType::Map { .. } => format!("{}[[{}]]", id, escaped_label),
            VisualNodeType::While { .. } => format!("{}(({}))", id, escaped_label),
            VisualNodeType::Conditional => format!("{}{{{}}}", id, escaped_label),
            VisualNodeType::InputOutput => format!("{}[/{}\\]", id, escaped_label),
            VisualNodeType::SubGraph { .. } => format!("{}[[{}]]", id, escaped_label),
        }
    }

    /// Get the Mermaid arrow syntax for an edge type
    fn edge_arrow(edge_type: &EdgeType) -> &'static str {
        match edge_type {
            EdgeType::DataFlow => "-->",
            EdgeType::Conditional => "-.->",
            EdgeType::LoopBack => "..>",
            EdgeType::Error => "-->",
        }
    }

    /// Get the CSS class name for a node status
    fn status_class(status: &NodeStatus) -> Option<&'static str> {
        match status {
            NodeStatus::Pending => None, // No class for default state
            NodeStatus::Running => Some("running"),
            NodeStatus::Completed => Some("completed"),
            NodeStatus::Failed => Some("failed"),
            NodeStatus::Skipped => Some("skipped"),
        }
    }
}

impl GraphRenderer for MermaidRenderer {
    fn render(&self, graph: &VisualGraph) -> Result<String, RenderError> {
        let mut output = String::new();

        // Graph header with direction
        output.push_str(&format!("graph {}\n", self.config.direction.mermaid_code()));

        // Add title as a comment
        output.push_str(&format!("    %% {}\n", graph.name));

        // Render nodes
        for node in &graph.nodes {
            let shape = Self::node_shape(&node.node_type, &node.id, &node.label);
            output.push_str(&format!("    {}\n", shape));
        }

        output.push('\n');

        // Render edges
        for edge in &graph.edges {
            let arrow = Self::edge_arrow(&edge.edge_type);

            if self.config.show_edge_labels {
                if let Some(ref label) = edge.label {
                    let escaped_label = Self::escape_label(label);
                    output.push_str(&format!(
                        "    {} {}|{}| {}\n",
                        edge.from, arrow, escaped_label, edge.to
                    ));
                } else {
                    output.push_str(&format!("    {} {} {}\n", edge.from, arrow, edge.to));
                }
            } else {
                output.push_str(&format!("    {} {} {}\n", edge.from, arrow, edge.to));
            }
        }

        // Add status styling if enabled
        if self.config.show_status {
            output.push('\n');
            
            // Collect nodes by status
            let mut status_classes: Vec<(&str, Vec<&str>)> = Vec::new();
            
            for node in &graph.nodes {
                if let Some(class) = Self::status_class(&node.status) {
                    if let Some(entry) = status_classes.iter_mut().find(|(c, _)| *c == class) {
                        entry.1.push(&node.id);
                    } else {
                        status_classes.push((class, vec![&node.id]));
                    }
                }
            }

            // Output class assignments
            for (class, nodes) in &status_classes {
                for node_id in nodes {
                    output.push_str(&format!("    class {} {}\n", node_id, class));
                }
            }

            // Add style definitions
            if !status_classes.is_empty() {
                output.push('\n');
                output.push_str("    %% Status styles\n");
                output.push_str("    classDef running fill:#ffd700,stroke:#ffb000,color:#000\n");
                output.push_str("    classDef completed fill:#90ee90,stroke:#228b22,color:#000\n");
                output.push_str("    classDef failed fill:#ff6b6b,stroke:#dc3545,color:#fff\n");
                output.push_str("    classDef skipped fill:#d3d3d3,stroke:#808080,color:#555\n");
            }
        }

        Ok(output)
    }

    fn format(&self) -> OutputFormat {
        OutputFormat::Mermaid
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
        graph.add_node(VisualNode::new("loop", "While Loop", VisualNodeType::While { max_iterations: 10 }));
        graph.add_node(
            VisualNode::new("end", "End", VisualNodeType::Standard)
                .with_status(NodeStatus::Completed),
        );

        graph.add_edge(VisualEdge::new("start", "llm"));
        graph.add_edge(VisualEdge::new("llm", "loop"));
        graph.add_edge(VisualEdge::new("loop", "end").with_type(EdgeType::Conditional));

        graph
    }

    #[test]
    fn test_render_basic_graph() {
        let graph = create_test_graph();
        let renderer = MermaidRenderer::default_renderer();
        let result = renderer.render(&graph).unwrap();

        // Check basic structure
        assert!(result.contains("graph TD"));
        assert!(result.contains("start[Start]"));
        assert!(result.contains("llm([LLM Process])"));
        assert!(result.contains("loop((While Loop))"));
        assert!(result.contains("start --> llm"));
        assert!(result.contains("loop -.-> end"));
    }

    #[test]
    fn test_render_with_status_styling() {
        let graph = create_test_graph();
        let renderer = MermaidRenderer::default_renderer();
        let result = renderer.render(&graph).unwrap();

        assert!(result.contains("class end completed"));
        assert!(result.contains("classDef completed"));
    }

    #[test]
    fn test_render_different_directions() {
        let graph = create_test_graph();
        
        let mut config = RenderConfig::default();
        config.direction = super::super::GraphDirection::LeftToRight;
        
        let renderer = MermaidRenderer::new(config);
        let result = renderer.render(&graph).unwrap();

        assert!(result.contains("graph LR"));
    }

    #[test]
    fn test_escape_special_characters() {
        assert_eq!(
            MermaidRenderer::escape_label("Line1\nLine2"),
            "Line1<br/>Line2"
        );
        assert_eq!(
            MermaidRenderer::escape_label("<script>"),
            "&lt;script&gt;"
        );
    }

    #[test]
    fn test_node_shapes() {
        // Standard node
        assert_eq!(
            MermaidRenderer::node_shape(&VisualNodeType::Standard, "n1", "Test"),
            "n1[Test]"
        );

        // LLM node (stadium)
        assert_eq!(
            MermaidRenderer::node_shape(&VisualNodeType::Llm, "n2", "LLM"),
            "n2([LLM])"
        );

        // While node (circle)
        assert_eq!(
            MermaidRenderer::node_shape(&VisualNodeType::While { max_iterations: 10 }, "n3", "Loop"),
            "n3((Loop))"
        );

        // Map node (subroutine)
        assert_eq!(
            MermaidRenderer::node_shape(&VisualNodeType::Map { parallel: true }, "n4", "Map"),
            "n4[[Map]]"
        );

        // Conditional node (diamond)
        assert_eq!(
            MermaidRenderer::node_shape(&VisualNodeType::Conditional, "n5", "If"),
            "n5{If}"
        );
    }
}
