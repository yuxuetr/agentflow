use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TemplateNode {
  pub name: String,
  pub template: String,
  pub output_format: String,
  pub variables: HashMap<String, String>,
}

use crate::common::utils::flow_value_to_string;

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
}

#[async_trait]
impl AsyncNode for TemplateNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut rendered = self.template.clone();

        // First, replace with node-defined variables
        for (key, value) in &self.variables {
            let pattern = format!("{{{{{}}}}}", key);
            rendered = rendered.replace(&pattern, value);
        }

        // Then, replace with inputs from the flow, which can override variables
        for (key, value) in inputs {
            let pattern = format!("{{{{{}}}}}", key);
            rendered = rendered.replace(&pattern, &flow_value_to_string(value));
        }

        println!("📝 Rendering template for node '{}'", self.name);

        let result_value = match self.output_format.as_str() {
            "json" => {
                match serde_json::from_str::<Value>(&rendered) {
                    Ok(json) => FlowValue::Json(json),
                    Err(_) => FlowValue::Json(Value::String(rendered)),
                }
            }
            _ => FlowValue::Json(Value::String(rendered)),
        };

        println!("✅ Template rendered successfully");

        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), result_value);
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_template_node_simple_rendering() {
        let node = TemplateNode::new("simple_test", "Hello {{name}}, welcome to {{place}}!");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("name".to_string(), FlowValue::Json(json!("Alice")));
        inputs.insert("place".to_string(), FlowValue::Json(json!("AgentFlow")));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!("Hello Alice, welcome to AgentFlow!")));
    }

    #[tokio::test]
    async fn test_template_node_with_variables() {
        let node = TemplateNode::new("var_test", "{{greeting}} {{name}}{{punctuation}}")
            .with_variable("greeting", "Hi")
            .with_variable("punctuation", "!");

        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("name".to_string(), FlowValue::Json(json!("Bob")));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!("Hi Bob!")));
    }

    #[tokio::test]
    async fn test_template_node_json_output_format() {
        let node = TemplateNode::new("json_test", r#"{"message": "Hello {{name}}"}"#).with_format("json");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("name".to_string(), FlowValue::Json(json!("World")));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!({ "message": "Hello World" })));
    }
}