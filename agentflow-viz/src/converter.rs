//! Converters for transforming workflow definitions into visual graphs
//!
//! This module provides utilities to convert various workflow representations
//! into the `VisualGraph` format for visualization.

use crate::graph::{EdgeType, GraphMetadata, VisualEdge, VisualGraph, VisualNode, VisualNodeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A simplified workflow definition that can be converted to a visual graph.
/// This is designed to be compatible with agentflow-cli's YAML format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
  /// Name of the workflow
  pub name: String,
  /// Optional description
  #[serde(default)]
  pub description: Option<String>,
  /// Optional version
  #[serde(default)]
  pub version: Option<String>,
  /// List of nodes in the workflow
  pub nodes: Vec<NodeDefinition>,
}

/// A node definition from a workflow file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
  /// Unique identifier for this node
  pub id: String,
  /// Type of the node (e.g., "llm", "template", "map", "while")
  #[serde(rename = "type")]
  pub node_type: String,
  /// Dependencies (IDs of nodes that must complete before this one)
  #[serde(default)]
  pub dependencies: Vec<String>,
  /// Optional run condition
  #[serde(default)]
  pub run_if: Option<String>,
  /// Node parameters
  #[serde(default)]
  pub parameters: HashMap<String, serde_json::Value>,
  /// Input mappings
  #[serde(default)]
  pub input_mapping: HashMap<String, String>,
}

/// Converter for transforming workflow definitions to visual graphs
pub struct WorkflowConverter;

impl WorkflowConverter {
  /// Convert a workflow definition to a visual graph
  pub fn convert(workflow: &WorkflowDefinition) -> VisualGraph {
    let mut graph = VisualGraph::new(sanitize_id(&workflow.name), &workflow.name);

    // Set metadata
    graph.metadata = GraphMetadata {
      description: workflow.description.clone(),
      version: workflow.version.clone(),
      ..Default::default()
    };

    // Convert nodes
    for node_def in &workflow.nodes {
      let visual_node = Self::convert_node(node_def);
      graph.add_node(visual_node);
    }

    // Convert dependencies to edges
    for node_def in &workflow.nodes {
      for dep_id in &node_def.dependencies {
        let edge = if node_def.run_if.is_some() {
          VisualEdge::new(dep_id, &node_def.id).with_type(EdgeType::Conditional)
        } else {
          VisualEdge::new(dep_id, &node_def.id)
        };
        graph.add_edge(edge);
      }

      // Also create edges from input_mapping references
      for mapping in node_def.input_mapping.values() {
        // Parse mapping like "{{ nodes.other_node.outputs.result }}"
        if let Some(source_node) = Self::parse_node_reference(mapping) {
          // Only add edge if not already a dependency
          if !node_def.dependencies.contains(&source_node) {
            let edge = VisualEdge::new(&source_node, &node_def.id).with_type(EdgeType::DataFlow);
            graph.add_edge(edge);
          }
        }
      }
    }

    graph
  }

  /// Convert a node definition to a visual node
  fn convert_node(node_def: &NodeDefinition) -> VisualNode {
    let node_type = Self::parse_node_type(&node_def.node_type, &node_def.parameters);

    let label = if node_def.node_type == node_def.id {
      node_def.id.clone()
    } else {
      format!("{}\n({})", node_def.id, node_def.node_type)
    };

    let mut visual_node = VisualNode::new(&node_def.id, label, node_type);

    // Add properties
    visual_node
      .properties
      .insert("type".to_string(), node_def.node_type.clone());

    if let Some(ref run_if) = node_def.run_if {
      visual_node
        .properties
        .insert("run_if".to_string(), run_if.clone());
    }

    visual_node
  }

  /// Parse node type string to VisualNodeType
  fn parse_node_type(
    type_str: &str,
    parameters: &HashMap<String, serde_json::Value>,
  ) -> VisualNodeType {
    match type_str.to_lowercase().as_str() {
      "llm" => VisualNodeType::Llm,
      "template" => VisualNodeType::Template,
      "map" => {
        let parallel = parameters
          .get("parallel")
          .and_then(|v| v.as_bool())
          .unwrap_or(false);
        VisualNodeType::Map { parallel }
      }
      "while" => {
        let max_iterations = parameters
          .get("max_iterations")
          .and_then(|v| v.as_u64())
          .unwrap_or(100) as u32;
        VisualNodeType::While { max_iterations }
      }
      "conditional" | "branch" | "if" => VisualNodeType::Conditional,
      "input" | "output" | "io" => VisualNodeType::InputOutput,
      _ => VisualNodeType::Standard,
    }
  }

  /// Parse a node reference from an input mapping string
  /// e.g., "{{ nodes.other_node.outputs.result }}" -> Some("other_node")
  fn parse_node_reference(mapping: &str) -> Option<String> {
    let trimmed = mapping.trim();

    // Handle "{{ nodes.xxx.outputs.yyy }}" format
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
      let inner = trimmed
        .trim_start_matches("{{")
        .trim_end_matches("}}")
        .trim();

      let parts: Vec<&str> = inner.split('.').collect();
      if parts.len() >= 2 && parts[0] == "nodes" {
        return Some(parts[1].to_string());
      }
    }

    // Handle direct reference "nodes.xxx.outputs.yyy"
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() >= 2 && parts[0] == "nodes" {
      return Some(parts[1].to_string());
    }

    None
  }
}

/// Convert a YAML string to a visual graph
pub fn from_yaml(yaml: &str) -> Result<VisualGraph, ConversionError> {
  let workflow: WorkflowDefinition =
    serde_yaml::from_str(yaml).map_err(|e| ConversionError::ParseError(e.to_string()))?;
  Ok(WorkflowConverter::convert(&workflow))
}

/// Convert a JSON string to a visual graph
pub fn from_json(json: &str) -> Result<VisualGraph, ConversionError> {
  let workflow: WorkflowDefinition =
    serde_json::from_str(json).map_err(|e| ConversionError::ParseError(e.to_string()))?;
  Ok(WorkflowConverter::convert(&workflow))
}

/// Sanitize a string to be used as an ID (remove spaces and special characters)
fn sanitize_id(s: &str) -> String {
  s.chars()
    .map(|c| {
      if c.is_alphanumeric() || c == '_' {
        c
      } else {
        '_'
      }
    })
    .collect()
}

/// Errors that can occur during conversion
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
  #[error("Failed to parse workflow: {0}")]
  ParseError(String),

  #[error("Invalid workflow structure: {0}")]
  InvalidStructure(String),
}

/// Builder for creating visual graphs programmatically
pub struct VisualGraphBuilder {
  graph: VisualGraph,
}

impl VisualGraphBuilder {
  /// Create a new builder
  pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
    Self {
      graph: VisualGraph::new(id, name),
    }
  }

  /// Add a standard node
  pub fn add_node(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
    self.graph.add_node(VisualNode::standard(id, label));
    self
  }

  /// Add a node with a specific type
  pub fn add_typed_node(
    mut self,
    id: impl Into<String>,
    label: impl Into<String>,
    node_type: VisualNodeType,
  ) -> Self {
    self.graph.add_node(VisualNode::new(id, label, node_type));
    self
  }

  /// Add an edge between two nodes
  pub fn add_edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
    self.graph.add_edge(VisualEdge::new(from, to));
    self
  }

  /// Add a labeled edge
  pub fn add_labeled_edge(
    mut self,
    from: impl Into<String>,
    to: impl Into<String>,
    label: impl Into<String>,
  ) -> Self {
    self
      .graph
      .add_edge(VisualEdge::new(from, to).with_label(label));
    self
  }

  /// Set description
  pub fn with_description(mut self, desc: impl Into<String>) -> Self {
    self.graph.metadata.description = Some(desc.into());
    self
  }

  /// Build the graph
  pub fn build(self) -> VisualGraph {
    self.graph
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_from_yaml() {
    let yaml = r#"
name: Test Workflow
description: A test workflow
nodes:
  - id: start
    type: template
    parameters:
      template: "Hello {{name}}"
  - id: process
    type: llm
    dependencies: [start]
  - id: end
    type: template
    dependencies: [process]
"#;

    let graph = from_yaml(yaml).unwrap();

    assert_eq!(graph.name, "Test Workflow");
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 2);

    // Check node types
    let start = graph.get_node("start").unwrap();
    assert!(matches!(start.node_type, VisualNodeType::Template));

    let process = graph.get_node("process").unwrap();
    assert!(matches!(process.node_type, VisualNodeType::Llm));
  }

  #[test]
  fn test_builder() {
    let graph = VisualGraphBuilder::new("test", "Test Workflow")
      .add_node("a", "Node A")
      .add_node("b", "Node B")
      .add_node("c", "Node C")
      .add_edge("a", "b")
      .add_edge("b", "c")
      .with_description("A simple test workflow")
      .build();

    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(
      graph.metadata.description,
      Some("A simple test workflow".to_string())
    );
  }

  #[test]
  fn test_parse_node_reference() {
    assert_eq!(
      WorkflowConverter::parse_node_reference("{{ nodes.other_node.outputs.result }}"),
      Some("other_node".to_string())
    );

    assert_eq!(
      WorkflowConverter::parse_node_reference("nodes.foo.outputs.bar"),
      Some("foo".to_string())
    );

    assert_eq!(WorkflowConverter::parse_node_reference("some_value"), None);
  }

  #[test]
  fn test_map_and_while_nodes() {
    let yaml = r#"
name: Loop Workflow
nodes:
  - id: map_node
    type: map
    parameters:
      parallel: true
  - id: while_node
    type: while
    parameters:
      max_iterations: 50
    dependencies: [map_node]
"#;

    let graph = from_yaml(yaml).unwrap();

    let map_node = graph.get_node("map_node").unwrap();
    assert!(matches!(
      map_node.node_type,
      VisualNodeType::Map { parallel: true }
    ));

    let while_node = graph.get_node("while_node").unwrap();
    assert!(matches!(
      while_node.node_type,
      VisualNodeType::While { max_iterations: 50 }
    ));
  }
}
