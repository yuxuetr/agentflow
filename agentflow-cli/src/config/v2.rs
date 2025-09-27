use serde::Deserialize;
use std::collections::HashMap;

/// Defines the structure of a V2 workflow YAML file.
#[derive(Debug, Deserialize)]
pub struct FlowDefinitionV2 {
    pub name: String,
    #[serde(default)]
    pub inputs: HashMap<String, InputDefinitionV2>,
    pub nodes: Vec<NodeDefinitionV2>,
}

/// Defines a required input for the workflow.
#[derive(Debug, Deserialize)]
pub struct InputDefinitionV2 {
    pub description: Option<String>,
    pub required: bool,
    pub default: Option<serde_yaml::Value>,
}

/// Defines a single node in the V2 workflow graph.
#[derive(Debug, Deserialize)]
pub struct NodeDefinitionV2 {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub input_mapping: HashMap<String, String>,
    #[serde(default)]
    pub run_if: Option<String>,
    #[serde(default)]
    pub parameters: HashMap<String, serde_yaml::Value>,
}
