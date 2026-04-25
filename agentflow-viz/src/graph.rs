//! Core data structures for workflow visualization
//!
//! This module defines the `VisualGraph` structure which represents a workflow
//! as a directed graph suitable for visualization in various formats.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A visual representation of a workflow graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualGraph {
  /// Unique identifier for this graph
  pub id: String,
  /// Display name of the workflow
  pub name: String,
  /// All nodes in the graph
  pub nodes: Vec<VisualNode>,
  /// All edges (connections) between nodes
  pub edges: Vec<VisualEdge>,
  /// Additional metadata about the graph
  pub metadata: GraphMetadata,
}

impl VisualGraph {
  /// Create a new empty visual graph
  pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
    Self {
      id: id.into(),
      name: name.into(),
      nodes: Vec::new(),
      edges: Vec::new(),
      metadata: GraphMetadata::default(),
    }
  }

  /// Add a node to the graph
  pub fn add_node(&mut self, node: VisualNode) {
    self.nodes.push(node);
  }

  /// Add an edge to the graph
  pub fn add_edge(&mut self, edge: VisualEdge) {
    self.edges.push(edge);
  }

  /// Find a node by its ID
  pub fn get_node(&self, id: &str) -> Option<&VisualNode> {
    self.nodes.iter().find(|n| n.id == id)
  }

  /// Find a node by its ID (mutable)
  pub fn get_node_mut(&mut self, id: &str) -> Option<&mut VisualNode> {
    self.nodes.iter_mut().find(|n| n.id == id)
  }

  /// Update the status of a node
  pub fn update_node_status(&mut self, node_id: &str, status: NodeStatus) {
    if let Some(node) = self.get_node_mut(node_id) {
      node.status = status;
    }
  }

  /// Get all root nodes (nodes with no incoming edges)
  pub fn root_nodes(&self) -> Vec<&VisualNode> {
    let targets: std::collections::HashSet<_> = self.edges.iter().map(|e| &e.to).collect();
    self
      .nodes
      .iter()
      .filter(|n| !targets.contains(&n.id))
      .collect()
  }

  /// Get all leaf nodes (nodes with no outgoing edges)
  pub fn leaf_nodes(&self) -> Vec<&VisualNode> {
    let sources: std::collections::HashSet<_> = self.edges.iter().map(|e| &e.from).collect();
    self
      .nodes
      .iter()
      .filter(|n| !sources.contains(&n.id))
      .collect()
  }

  /// Calculate the depth of the graph (longest path from root to leaf)
  pub fn depth(&self) -> usize {
    if self.nodes.is_empty() {
      return 0;
    }

    // Build adjacency list
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for node in &self.nodes {
      adj.insert(&node.id, Vec::new());
    }
    for edge in &self.edges {
      if let Some(neighbors) = adj.get_mut(edge.from.as_str()) {
        neighbors.push(&edge.to);
      }
    }

    // Find max depth using DFS
    fn dfs<'a>(
      node: &'a str,
      adj: &HashMap<&'a str, Vec<&'a str>>,
      memo: &mut HashMap<&'a str, usize>,
    ) -> usize {
      if let Some(&depth) = memo.get(node) {
        return depth;
      }
      let neighbors = adj.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
      let max_child = neighbors
        .iter()
        .map(|n| dfs(n, adj, memo))
        .max()
        .unwrap_or(0);
      let depth = 1 + max_child;
      memo.insert(node, depth);
      depth
    }

    let mut memo = HashMap::new();
    self
      .nodes
      .iter()
      .map(|n| dfs(&n.id, &adj, &mut memo))
      .max()
      .unwrap_or(0)
  }
}

/// A node in the visual graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualNode {
  /// Unique identifier for this node
  pub id: String,
  /// Display label for this node
  pub label: String,
  /// Type of the node (affects visual representation)
  pub node_type: VisualNodeType,
  /// Current execution status
  pub status: NodeStatus,
  /// Optional position hint for layout
  pub position: Option<Position>,
  /// Visual styling options
  pub style: NodeStyle,
  /// Additional properties
  pub properties: HashMap<String, String>,
}

impl VisualNode {
  /// Create a new visual node with default styling
  pub fn new(id: impl Into<String>, label: impl Into<String>, node_type: VisualNodeType) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      node_type,
      status: NodeStatus::Pending,
      position: None,
      style: NodeStyle::default(),
      properties: HashMap::new(),
    }
  }

  /// Create a standard node
  pub fn standard(id: impl Into<String>, label: impl Into<String>) -> Self {
    Self::new(id, label, VisualNodeType::Standard)
  }

  /// Set the node status
  pub fn with_status(mut self, status: NodeStatus) -> Self {
    self.status = status;
    self
  }

  /// Add a property
  pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.properties.insert(key.into(), value.into());
    self
  }
}

/// Types of nodes with different visual representations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisualNodeType {
  /// Standard processing node (rectangle)
  Standard,
  /// LLM/AI node (rounded rectangle)
  Llm,
  /// Template node (document shape)
  Template,
  /// Map node for parallel iteration (folder shape)
  Map {
    /// Whether items are processed in parallel
    parallel: bool,
  },
  /// While loop node (diamond shape)
  While {
    /// Maximum iterations allowed
    max_iterations: u32,
  },
  /// Conditional branch node (diamond shape)
  Conditional,
  /// Input/Output node (parallelogram)
  InputOutput,
  /// Sub-workflow node (contains nested graph)
  SubGraph {
    /// The nested graph
    graph: Box<VisualGraph>,
  },
}

impl Default for VisualNodeType {
  fn default() -> Self {
    Self::Standard
  }
}

/// Execution status of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
  /// Not yet executed
  #[default]
  Pending,
  /// Currently executing
  Running,
  /// Successfully completed
  Completed,
  /// Failed with error
  Failed,
  /// Skipped due to condition
  Skipped,
}

impl NodeStatus {
  /// Get the color associated with this status
  pub fn color(&self) -> &'static str {
    match self {
      NodeStatus::Pending => "#f0f0f0",
      NodeStatus::Running => "#ffd700",
      NodeStatus::Completed => "#90ee90",
      NodeStatus::Failed => "#ff6b6b",
      NodeStatus::Skipped => "#d3d3d3",
    }
  }

  /// Get a human-readable label
  pub fn label(&self) -> &'static str {
    match self {
      NodeStatus::Pending => "Pending",
      NodeStatus::Running => "Running",
      NodeStatus::Completed => "Completed",
      NodeStatus::Failed => "Failed",
      NodeStatus::Skipped => "Skipped",
    }
  }
}

/// An edge (connection) between two nodes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisualEdge {
  /// Source node ID
  pub from: String,
  /// Target node ID
  pub to: String,
  /// Optional label on the edge
  pub label: Option<String>,
  /// Type of edge (affects visual style)
  pub edge_type: EdgeType,
}

impl VisualEdge {
  /// Create a new edge
  pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
    Self {
      from: from.into(),
      to: to.into(),
      label: None,
      edge_type: EdgeType::DataFlow,
    }
  }

  /// Create an edge with a label
  pub fn with_label(mut self, label: impl Into<String>) -> Self {
    self.label = Some(label.into());
    self
  }

  /// Set the edge type
  pub fn with_type(mut self, edge_type: EdgeType) -> Self {
    self.edge_type = edge_type;
    self
  }
}

/// Types of edges with different visual representations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
  /// Normal data flow (solid arrow)
  #[default]
  DataFlow,
  /// Conditional flow (dashed arrow)
  Conditional,
  /// Loop back edge (dotted arrow)
  LoopBack,
  /// Error handling edge (red)
  Error,
}

/// Position hint for node layout
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Position {
  pub x: f64,
  pub y: f64,
}

impl Position {
  pub fn new(x: f64, y: f64) -> Self {
    Self { x, y }
  }
}

/// Visual styling options for a node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStyle {
  /// Fill color (hex or named)
  pub fill_color: Option<String>,
  /// Border color
  pub border_color: Option<String>,
  /// Text color
  pub text_color: Option<String>,
  /// Border width
  pub border_width: Option<f64>,
}

impl Default for NodeStyle {
  fn default() -> Self {
    Self {
      fill_color: None,
      border_color: None,
      text_color: None,
      border_width: None,
    }
  }
}

/// Metadata about the graph
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GraphMetadata {
  /// Description of the workflow
  pub description: Option<String>,
  /// Version of the workflow
  pub version: Option<String>,
  /// Author information
  pub author: Option<String>,
  /// Creation timestamp
  pub created_at: Option<String>,
  /// Custom tags
  pub tags: Vec<String>,
  /// Custom key-value properties
  pub properties: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_create_visual_graph() {
    let mut graph = VisualGraph::new("test_graph", "Test Workflow");

    graph.add_node(VisualNode::standard("node1", "Start"));
    graph.add_node(VisualNode::standard("node2", "Process"));
    graph.add_node(VisualNode::standard("node3", "End"));

    graph.add_edge(VisualEdge::new("node1", "node2"));
    graph.add_edge(VisualEdge::new("node2", "node3"));

    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.depth(), 3);
  }

  #[test]
  fn test_root_and_leaf_nodes() {
    let mut graph = VisualGraph::new("test", "Test");

    graph.add_node(VisualNode::standard("a", "A"));
    graph.add_node(VisualNode::standard("b", "B"));
    graph.add_node(VisualNode::standard("c", "C"));

    graph.add_edge(VisualEdge::new("a", "b"));
    graph.add_edge(VisualEdge::new("b", "c"));

    let roots = graph.root_nodes();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, "a");

    let leaves = graph.leaf_nodes();
    assert_eq!(leaves.len(), 1);
    assert_eq!(leaves[0].id, "c");
  }

  #[test]
  fn test_update_node_status() {
    let mut graph = VisualGraph::new("test", "Test");
    graph.add_node(VisualNode::standard("node1", "Node 1"));

    assert_eq!(graph.get_node("node1").unwrap().status, NodeStatus::Pending);

    graph.update_node_status("node1", NodeStatus::Running);
    assert_eq!(graph.get_node("node1").unwrap().status, NodeStatus::Running);

    graph.update_node_status("node1", NodeStatus::Completed);
    assert_eq!(
      graph.get_node("node1").unwrap().status,
      NodeStatus::Completed
    );
  }

  #[test]
  fn test_node_status_colors() {
    assert_eq!(NodeStatus::Pending.color(), "#f0f0f0");
    assert_eq!(NodeStatus::Running.color(), "#ffd700");
    assert_eq!(NodeStatus::Completed.color(), "#90ee90");
    assert_eq!(NodeStatus::Failed.color(), "#ff6b6b");
    assert_eq!(NodeStatus::Skipped.color(), "#d3d3d3");
  }
}
