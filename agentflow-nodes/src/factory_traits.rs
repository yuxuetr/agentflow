//! Factory traits for configuration support
//! These traits allow agentflow-config to create node instances from configuration

use crate::{AsyncNode, NodeResult, NodeError};
use serde_json::Value;
use std::collections::HashMap;

/// Configuration for a node factory
#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub name: String,
    pub node_type: String,
    pub prompt: Option<String>,
    pub system: Option<String>,
    pub parameters: Option<HashMap<String, Value>>,
    pub dependencies: Option<Vec<String>>,
    pub condition: Option<String>,
}

/// Resolved node configuration with templates processed
#[derive(Debug, Clone)]
pub struct ResolvedNodeConfig {
    pub name: String,
    pub node_type: String,
    pub resolved_prompt: Option<String>,
    pub resolved_system: Option<String>,
    pub parameters: HashMap<String, Value>,
}

/// Factory trait for creating nodes from configuration
pub trait NodeFactory: Send + Sync {
    fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>>;
    fn validate_config(&self, config: &NodeConfig) -> NodeResult<()>;
    fn get_input_schema(&self) -> Value;
    fn get_output_schema(&self) -> Value;
    fn supports_streaming(&self) -> bool { false }
    fn supports_batch(&self) -> bool { false }
}

/// Registry for node factories  
pub struct NodeRegistry {
    factories: HashMap<String, Box<dyn NodeFactory>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a node factory
    pub fn register(&mut self, node_type: &str, factory: Box<dyn NodeFactory>) {
        self.factories.insert(node_type.to_string(), factory);
    }

    /// Check if a node type is supported
    pub fn supports_node_type(&self, node_type: &str) -> bool {
        self.factories.contains_key(node_type)
    }

    /// Create a node from configuration
    pub fn create_node(
        &self, 
        node_type: &str, 
        config: ResolvedNodeConfig
    ) -> NodeResult<Box<dyn AsyncNode>> {
        let factory = self.factories.get(node_type)
            .ok_or_else(|| NodeError::ConfigurationError {
                message: format!("Unknown node type: {}", node_type),
            })?;

        factory.create_node(config)
    }

    /// Get list of supported node types
    pub fn supported_types(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

impl Default for NodeRegistry {
    fn default() -> Self {
        let registry = Self::new();
        #[cfg(feature = "factories")]
        crate::factories::register_builtin_factories(&mut registry);
        registry
    }
}