use crate::{AsyncNode, NodeResult, SharedState};
use agentflow_core::AgentFlowError;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Template node for text processing and template rendering
#[derive(Debug, Clone)]
pub struct TemplateNode {
  pub name: String,
  pub template: String,
  pub output_format: String, // "text", "json", "yaml"
  pub variables: HashMap<String, String>,
}

impl TemplateNode {
  pub fn new(name: &str, template: &str) -> Self {
    Self {
      name: name.to_string(),
      template: template.to_string(),
      output_format: "text".to_string(),
      variables: HashMap::new(),
    }
  }

  pub fn with_format(mut self, format: &str) -> Self {
    self.output_format = format.to_string();
    self
  }

  pub fn with_variables(mut self, variables: HashMap<String, String>) -> Self {
    self.variables = variables;
    self
  }

  pub fn with_variable(mut self, key: &str, value: &str) -> Self {
    self.variables.insert(key.to_string(), value.to_string());
    self
  }

  /// Simple template rendering (basic variable substitution)
  fn render_template(
    &self,
    template: &str,
    context: &HashMap<String, Value>,
  ) -> NodeResult<String> {
    let mut result = template.to_string();

    // Replace {{variable}} patterns with values from context
    for (key, value) in context {
      let pattern = format!("{{{{{}}}}}", key);
      let replacement = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
      };
      result = result.replace(&pattern, &replacement);
    }

    // Replace variables from node configuration
    for (key, value) in &self.variables {
      let pattern = format!("{{{{{}}}}}", key);
      result = result.replace(&pattern, value);
    }

    Ok(result)
  }
}

#[async_trait]
impl AsyncNode for TemplateNode {
  async fn prep_async(&self, shared: &SharedState) -> std::result::Result<Value, AgentFlowError> {
    // Resolve template variables from shared state
    let mut context = HashMap::new();

    // Copy all shared state values to context
    for (key, value) in shared.iter() {
      context.insert(key.clone(), value.clone());
    }

    // Add explicit variables
    for (key, value) in &self.variables {
      context.insert(key.clone(), Value::String(value.clone()));
    }

    Ok(serde_json::json!({
        "template": self.template,
        "output_format": self.output_format,
        "context": context
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> std::result::Result<Value, AgentFlowError> {
    let template = prep_result["template"].as_str().unwrap_or(&self.template);
    let output_format = prep_result["output_format"]
      .as_str()
      .unwrap_or(&self.output_format);
    let context = prep_result["context"]
      .as_object()
      .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
      .unwrap_or_default();

    println!("📝 Rendering template for node '{}'", self.name);

    let rendered = self.render_template(template, &context).map_err(|e| {
      AgentFlowError::AsyncExecutionError {
        message: format!("Template rendering failed: {}", e),
      }
    })?;

    let result = match output_format {
      "json" => {
        // Try to parse as JSON
        match serde_json::from_str::<Value>(&rendered) {
          Ok(json) => json,
          Err(_) => Value::String(rendered),
        }
      }
      "yaml" => {
        // For now, just return as string (could add serde_yaml support)
        Value::String(rendered)
      }
      _ => Value::String(rendered),
    };

    println!("✅ Template rendered successfully");
    Ok(result)
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> std::result::Result<Option<String>, AgentFlowError> {
    // Store the rendered result in shared state
    let output_key = format!("{}_output", self.name);
    shared.insert(output_key.clone(), exec_result.clone());

    // If the result is a string, also store it as "rendered_text"
    if let Value::String(text) = &exec_result {
      shared.insert("rendered_text".to_string(), Value::String(text.clone()));
    }

    println!(
      "💾 Stored template output in shared state as: {}",
      output_key
    );
    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_template_node_creation() {
    let node = TemplateNode::new("test_template", "Hello {{name}}!");
    assert_eq!(node.name, "test_template");
    assert_eq!(node.template, "Hello {{name}}!");
    assert_eq!(node.output_format, "text");
    assert!(node.variables.is_empty());
  }

  #[tokio::test]
  async fn test_template_node_builder_pattern() {
    let mut vars = HashMap::new();
    vars.insert("greeting".to_string(), "Hello".to_string());
    vars.insert("punctuation".to_string(), "!".to_string());

    let node = TemplateNode::new("builder_test", "{{greeting}} {{name}}{{punctuation}}")
      .with_format("json")
      .with_variables(vars.clone())
      .with_variable("extra", "value");

    assert_eq!(node.output_format, "json");
    assert_eq!(node.variables.get("greeting"), Some(&"Hello".to_string()));
    assert_eq!(node.variables.get("extra"), Some(&"value".to_string()));
  }

  #[tokio::test]
  async fn test_template_node_simple_rendering() {
    let node = TemplateNode::new("simple_test", "Hello {{name}}, welcome to {{place}}!");
    let shared = SharedState::new();
    shared.insert("name".to_string(), Value::String("Alice".to_string()));
    shared.insert("place".to_string(), Value::String("AgentFlow".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("simple_test_output").unwrap();
    assert_eq!(
      output.as_str().unwrap(),
      "Hello Alice, welcome to AgentFlow!"
    );

    let rendered_text = shared.get("rendered_text").unwrap();
    assert_eq!(
      rendered_text.as_str().unwrap(),
      "Hello Alice, welcome to AgentFlow!"
    );
  }

  #[tokio::test]
  async fn test_template_node_with_variables() {
    let node = TemplateNode::new("var_test", "{{greeting}} {{name}}{{punctuation}}")
      .with_variable("greeting", "Hi")
      .with_variable("punctuation", "!");

    let shared = SharedState::new();
    shared.insert("name".to_string(), Value::String("Bob".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("var_test_output").unwrap();
    assert_eq!(output.as_str().unwrap(), "Hi Bob!");
  }

  #[tokio::test]
  async fn test_template_node_numeric_values() {
    let node = TemplateNode::new("numeric_test", "Count: {{count}}, Price: ${{price}}");
    let shared = SharedState::new();
    shared.insert(
      "count".to_string(),
      Value::Number(serde_json::Number::from(42)),
    );
    shared.insert(
      "price".to_string(),
      Value::Number(serde_json::Number::from_f64(19.99).unwrap()),
    );

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("numeric_test_output").unwrap();
    assert_eq!(output.as_str().unwrap(), "Count: 42, Price: $19.99");
  }

  #[tokio::test]
  async fn test_template_node_boolean_values() {
    let node = TemplateNode::new(
      "bool_test",
      "Active: {{is_active}}, Complete: {{is_complete}}",
    );
    let shared = SharedState::new();
    shared.insert("is_active".to_string(), Value::Bool(true));
    shared.insert("is_complete".to_string(), Value::Bool(false));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("bool_test_output").unwrap();
    assert_eq!(output.as_str().unwrap(), "Active: true, Complete: false");
  }

  #[tokio::test]
  async fn test_template_node_json_output_format() {
    let node =
      TemplateNode::new("json_test", r#"{"message": "Hello {{name}}"}"#).with_format("json");

    let shared = SharedState::new();
    shared.insert("name".to_string(), Value::String("World".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("json_test_output").unwrap();
    assert_eq!(output["message"].as_str().unwrap(), "Hello World");
  }

  #[tokio::test]
  async fn test_template_node_invalid_json_fallback() {
    let node = TemplateNode::new("invalid_json", "This is not JSON: {{value}}").with_format("json");

    let shared = SharedState::new();
    shared.insert("value".to_string(), Value::String("test".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("invalid_json_output").unwrap();
    assert_eq!(output.as_str().unwrap(), "This is not JSON: test");
  }

  #[tokio::test]
  async fn test_template_node_yaml_output_format() {
    let node = TemplateNode::new("yaml_test", "message: Hello {{name}}\nstatus: {{status}}")
      .with_format("yaml");

    let shared = SharedState::new();
    shared.insert("name".to_string(), Value::String("YAML".to_string()));
    shared.insert("status".to_string(), Value::String("active".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let output = shared.get("yaml_test_output").unwrap();
    assert_eq!(
      output.as_str().unwrap(),
      "message: Hello YAML\nstatus: active"
    );
  }

  #[tokio::test]
  async fn test_template_node_prep_async() {
    let node = TemplateNode::new("prep_test", "Template: {{value}}")
      .with_variable("static_var", "static_value");

    let shared = SharedState::new();
    shared.insert(
      "shared_var".to_string(),
      Value::String("shared_value".to_string()),
    );

    let prep_result = node.prep_async(&shared).await.unwrap();

    assert_eq!(
      prep_result["template"].as_str().unwrap(),
      "Template: {{value}}"
    );
    assert_eq!(prep_result["output_format"].as_str().unwrap(), "text");

    let context = prep_result["context"].as_object().unwrap();
    assert_eq!(context["shared_var"].as_str().unwrap(), "shared_value");
    assert_eq!(context["static_var"].as_str().unwrap(), "static_value");
  }

  #[tokio::test]
  async fn test_template_node_exec_async() {
    let node = TemplateNode::new("exec_test", "Result: {{result}}");

    let prep_data = serde_json::json!({
        "template": "Result: {{result}}",
        "output_format": "text",
        "context": {
            "result": "success"
        }
    });

    let exec_result = node.exec_async(prep_data).await.unwrap();
    assert_eq!(exec_result.as_str().unwrap(), "Result: success");
  }

  #[tokio::test]
  async fn test_template_node_post_async() {
    let node = TemplateNode::new("post_test", "Template");
    let shared = SharedState::new();

    let exec_result = Value::String("Processed template result".to_string());
    let prep_result = Value::Object(serde_json::Map::new());

    let result = node
      .post_async(&shared, prep_result, exec_result.clone())
      .await
      .unwrap();
    assert!(result.is_none());

    // Verify shared state was updated
    assert_eq!(shared.get("post_test_output").unwrap(), exec_result);
    assert_eq!(shared.get("rendered_text").unwrap(), exec_result);
  }

  #[tokio::test]
  async fn test_template_node_get_node_id() {
    let node = TemplateNode::new("id_test", "Template");
    assert_eq!(node.get_node_id().unwrap(), "id_test");
  }

  #[tokio::test]
  async fn test_template_node_complex_template() {
    let template = r#"
# Report for {{name}}

## Summary
Status: {{status}}
Items processed: {{count}}
Success rate: {{success_rate}}%

## Details
{{#if has_errors}}
Errors encountered: {{error_count}}
{{/if}}

Generated on: {{date}}
"#;

    let node = TemplateNode::new("complex_test", template);
    let shared = SharedState::new();
    shared.insert("name".to_string(), Value::String("Test Report".to_string()));
    shared.insert("status".to_string(), Value::String("Complete".to_string()));
    shared.insert(
      "count".to_string(),
      Value::Number(serde_json::Number::from(100)),
    );
    shared.insert(
      "success_rate".to_string(),
      Value::Number(serde_json::Number::from_f64(95.5).unwrap()),
    );
    shared.insert("date".to_string(), Value::String("2024-01-01".to_string()));

    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    let binding = shared.get("complex_test_output").unwrap();
    let output = binding.as_str().unwrap();
    assert!(output.contains("# Report for Test Report"));
    assert!(output.contains("Status: Complete"));
    assert!(output.contains("Items processed: 100"));
    assert!(output.contains("Success rate: 95.5%"));
    assert!(output.contains("Generated on: 2024-01-01"));
  }
}
