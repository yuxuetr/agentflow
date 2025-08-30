//! Node factories for configuration-first workflow support

use crate::{AsyncNode, NodeConfig, NodeError, NodeFactory, NodeResult, ResolvedNodeConfig};
use serde_json::Value;
use std::sync::Arc;

// Import node types based on features
#[cfg(feature = "llm")]
use crate::nodes::llm::LlmNode;

#[cfg(feature = "http")]
use crate::nodes::http::HttpNode;

#[cfg(feature = "file")]
use crate::nodes::file::FileNode;

#[cfg(feature = "template")]
use crate::nodes::template::TemplateNode;

#[cfg(feature = "batch")]
use crate::nodes::batch::BatchNode;

#[cfg(feature = "conditional")]
use crate::nodes::conditional::{ConditionType, ConditionalNode};

// Factory implementations
#[cfg(all(feature = "factories", feature = "llm"))]
pub struct LlmNodeFactory;

#[cfg(all(feature = "factories", feature = "http"))]
pub struct HttpNodeFactory;

#[cfg(all(feature = "factories", feature = "file"))]
pub struct FileNodeFactory;

#[cfg(all(feature = "factories", feature = "template"))]
pub struct TemplateNodeFactory;

#[cfg(all(feature = "factories", feature = "batch"))]
pub struct BatchNodeFactory;

#[cfg(all(feature = "factories", feature = "conditional"))]
pub struct ConditionalNodeFactory;

// Factory trait implementations
#[cfg(all(feature = "factories", feature = "llm"))]
impl NodeFactory for LlmNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let mut node = LlmNode::new(&config.name, "gpt-3.5-turbo"); // Default model

    // Set model if specified
    if let Some(model) = config.parameters.get("model").and_then(|v| v.as_str()) {
      node.model = model.to_string();
    }

    // Set prompt template
    if let Some(prompt) = &config.resolved_prompt {
      node = node.with_prompt(prompt);
    }

    // Set system template
    if let Some(system) = &config.resolved_system {
      node = node.with_system(system);
    }

    // Set optional parameters
    if let Some(temp) = config
      .parameters
      .get("temperature")
      .and_then(|v| v.as_f64())
    {
      node = node.with_temperature(temp as f32);
    }

    if let Some(tokens) = config.parameters.get("max_tokens").and_then(|v| v.as_u64()) {
      node = node.with_max_tokens(tokens as u32);
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, _config: &NodeConfig) -> NodeResult<()> {
    // Basic validation - could check required fields, parameter ranges, etc.
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "model": {"type": "string", "default": "gpt-3.5-turbo"},
            "prompt": {"type": "string", "description": "Prompt template"},
            "system": {"type": "string", "description": "System message template"},
            "temperature": {"type": "number", "minimum": 0.0, "maximum": 2.0},
            "max_tokens": {"type": "integer", "minimum": 1}
        },
        "required": ["prompt"]
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "response": {"type": "string", "description": "LLM response text"}
        }
    })
  }
}

#[cfg(all(feature = "factories", feature = "http"))]
impl NodeFactory for HttpNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let url = config
      .parameters
      .get("url")
      .and_then(|v| v.as_str())
      .ok_or_else(|| NodeError::ValidationError {
        message: "HTTP node requires 'url' parameter".to_string(),
      })?;

    let mut node = HttpNode::new(&config.name, url);

    // Set method if specified
    if let Some(method) = config.parameters.get("method").and_then(|v| v.as_str()) {
      node = node.with_method(method);
    }

    // Set headers if specified
    if let Some(headers_obj) = config.parameters.get("headers").and_then(|v| v.as_object()) {
      let mut headers = std::collections::HashMap::new();
      for (key, value) in headers_obj {
        if let Some(header_value) = value.as_str() {
          headers.insert(key.clone(), header_value.to_string());
        }
      }
      node = node.with_headers(headers);
    }

    // Set body if specified
    if let Some(body) = config.parameters.get("body").and_then(|v| v.as_str()) {
      node = node.with_body(body);
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, config: &NodeConfig) -> NodeResult<()> {
    if config
      .parameters
      .as_ref()
      .and_then(|p| p.get("url"))
      .and_then(|v| v.as_str())
      .is_none()
    {
      return Err(NodeError::ValidationError {
        message: "HTTP node requires 'url' parameter".to_string(),
      });
    }
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "url": {"type": "string", "format": "uri"},
            "method": {"type": "string", "enum": ["GET", "POST", "PUT", "DELETE", "PATCH"], "default": "GET"},
            "headers": {"type": "object", "additionalProperties": {"type": "string"}},
            "body": {"type": "string"}
        },
        "required": ["url"]
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": {"type": "integer", "description": "HTTP status code"},
            "body": {"type": "string", "description": "Response body"},
            "success": {"type": "boolean", "description": "Whether request was successful"}
        }
    })
  }
}

#[cfg(all(feature = "factories", feature = "file"))]
impl NodeFactory for FileNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let operation = config
      .parameters
      .get("operation")
      .and_then(|v| v.as_str())
      .unwrap_or("read");

    let path = config
      .parameters
      .get("path")
      .and_then(|v| v.as_str())
      .ok_or_else(|| NodeError::ValidationError {
        message: "File node requires 'path' parameter".to_string(),
      })?;

    let mut node = FileNode::new(&config.name, operation, path);

    // Set content for write/append operations
    if let Some(content) = config.parameters.get("content").and_then(|v| v.as_str()) {
      node = node.with_content(content);
    }

    // Set encoding if specified
    if let Some(encoding) = config.parameters.get("encoding").and_then(|v| v.as_str()) {
      node = node.with_encoding(encoding);
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, config: &NodeConfig) -> NodeResult<()> {
    if config
      .parameters
      .as_ref()
      .and_then(|p| p.get("path"))
      .and_then(|v| v.as_str())
      .is_none()
    {
      return Err(NodeError::ValidationError {
        message: "File node requires 'path' parameter".to_string(),
      });
    }
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "operation": {"type": "string", "enum": ["read", "write", "append"], "default": "read"},
            "path": {"type": "string", "description": "File path"},
            "content": {"type": "string", "description": "Content for write/append operations"},
            "encoding": {"type": "string", "default": "utf-8"}
        },
        "required": ["path"]
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "operation": {"type": "string"},
            "path": {"type": "string"},
            "content": {"type": "string", "description": "File content (for read operations)"},
            "size": {"type": "integer", "description": "File size in bytes"}
        }
    })
  }
}

#[cfg(all(feature = "factories", feature = "template"))]
impl NodeFactory for TemplateNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let template = config
      .parameters
      .get("template")
      .and_then(|v| v.as_str())
      .ok_or_else(|| NodeError::ValidationError {
        message: "Template node requires 'template' parameter".to_string(),
      })?;

    let mut node = TemplateNode::new(&config.name, template);

    // Set output format if specified
    if let Some(format) = config
      .parameters
      .get("output_format")
      .and_then(|v| v.as_str())
    {
      node = node.with_format(format);
    }

    // Set variables if specified
    if let Some(vars_obj) = config
      .parameters
      .get("variables")
      .and_then(|v| v.as_object())
    {
      let mut variables = std::collections::HashMap::new();
      for (key, value) in vars_obj {
        if let Some(var_value) = value.as_str() {
          variables.insert(key.clone(), var_value.to_string());
        }
      }
      node = node.with_variables(variables);
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, config: &NodeConfig) -> NodeResult<()> {
    if config
      .parameters
      .as_ref()
      .and_then(|p| p.get("template"))
      .and_then(|v| v.as_str())
      .is_none()
    {
      return Err(NodeError::ValidationError {
        message: "Template node requires 'template' parameter".to_string(),
      });
    }
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "template": {"type": "string", "description": "Template string with {{variable}} placeholders"},
            "output_format": {"type": "string", "enum": ["text", "json", "yaml"], "default": "text"},
            "variables": {"type": "object", "additionalProperties": {"type": "string"}}
        },
        "required": ["template"]
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "rendered": {"description": "Rendered template output"}
        }
    })
  }
}

// Utility function to register all built-in factories
#[cfg(feature = "factories")]
pub fn register_builtin_factories(registry: &mut crate::NodeRegistry) {
  #[cfg(feature = "llm")]
  registry.register("llm", Box::new(LlmNodeFactory));

  #[cfg(feature = "http")]
  registry.register("http", Box::new(HttpNodeFactory));

  #[cfg(feature = "file")]
  registry.register("file", Box::new(FileNodeFactory));

  #[cfg(feature = "template")]
  registry.register("template", Box::new(TemplateNodeFactory));

  #[cfg(feature = "batch")]
  registry.register("batch", Box::new(BatchNodeFactory));

  #[cfg(feature = "conditional")]
  registry.register("conditional", Box::new(ConditionalNodeFactory));
}

// Batch factory implementation (simplified - doesn't support child nodes from config yet)
#[cfg(all(feature = "factories", feature = "batch"))]
impl NodeFactory for BatchNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let items_key = config
      .parameters
      .get("items_key")
      .and_then(|v| v.as_str())
      .unwrap_or("items");

    let mut node = BatchNode::new(&config.name, items_key);

    if let Some(batch_size) = config.parameters.get("batch_size").and_then(|v| v.as_u64()) {
      node = node.with_batch_size(batch_size as usize);
    }

    if let Some(max_concurrent) = config
      .parameters
      .get("max_concurrent")
      .and_then(|v| v.as_u64())
    {
      node = node.with_max_concurrent(max_concurrent as usize);
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, _config: &NodeConfig) -> NodeResult<()> {
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "items_key": {"type": "string", "default": "items", "description": "Key in shared state containing items array"},
            "batch_size": {"type": "integer", "default": 10, "minimum": 1},
            "max_concurrent": {"type": "integer", "default": 4, "minimum": 1}
        }
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "results": {"type": "array", "description": "Array of processing results"},
            "processed_count": {"type": "integer"},
            "batch_count": {"type": "integer"}
        }
    })
  }
}

// Conditional factory implementation
#[cfg(all(feature = "factories", feature = "conditional"))]
impl NodeFactory for ConditionalNodeFactory {
  fn create_node(&self, config: ResolvedNodeConfig) -> NodeResult<Box<dyn AsyncNode>> {
    let condition = config
      .parameters
      .get("condition")
      .and_then(|v| v.as_str())
      .ok_or_else(|| NodeError::ValidationError {
        message: "Conditional node requires 'condition' parameter".to_string(),
      })?;

    let mut node = ConditionalNode::new(&config.name, condition);

    // Determine condition type from parameters
    if let Some(expected) = config.parameters.get("equals").and_then(|v| v.as_str()) {
      node = node.with_condition_type(ConditionType::Equals(expected.to_string()));
    } else if let Some(threshold) = config
      .parameters
      .get("greater_than")
      .and_then(|v| v.as_f64())
    {
      node = node.with_condition_type(ConditionType::GreaterThan(threshold));
    } else if let Some(threshold) = config.parameters.get("less_than").and_then(|v| v.as_f64()) {
      node = node.with_condition_type(ConditionType::LessThan(threshold));
    } else if let Some(substring) = config.parameters.get("contains").and_then(|v| v.as_str()) {
      node = node.with_condition_type(ConditionType::Contains(substring.to_string()));
    }

    // Set true/false values
    if let Some(true_value) = config.parameters.get("true_value") {
      node = node.with_true_value(true_value.clone());
    }

    if let Some(false_value) = config.parameters.get("false_value") {
      node = node.with_false_value(false_value.clone());
    }

    Ok(Box::new(node))
  }

  fn validate_config(&self, config: &NodeConfig) -> NodeResult<()> {
    if config
      .parameters
      .as_ref()
      .and_then(|p| p.get("condition"))
      .and_then(|v| v.as_str())
      .is_none()
    {
      return Err(NodeError::ValidationError {
        message: "Conditional node requires 'condition' parameter".to_string(),
      });
    }
    Ok(())
  }

  fn get_input_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "condition": {"type": "string", "description": "Variable name or expression to evaluate"},
            "equals": {"type": "string", "description": "Expected value for equality check"},
            "greater_than": {"type": "number", "description": "Threshold for greater than check"},
            "less_than": {"type": "number", "description": "Threshold for less than check"},
            "contains": {"type": "string", "description": "Substring to search for"},
            "true_value": {"description": "Value to return when condition is true"},
            "false_value": {"description": "Value to return when condition is false"}
        },
        "required": ["condition"]
    })
  }

  fn get_output_schema(&self) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "condition_result": {"type": "boolean"},
            "selected_value": {"description": "The selected true or false value"},
            "branch": {"type": "string", "enum": ["true", "false"]}
        }
    })
  }
}
