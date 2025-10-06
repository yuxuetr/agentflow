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

  fn render_template(
    &self,
    template: &str,
    context: &HashMap<String, FlowValue>,
  ) -> Result<String, AgentFlowError> {
    let mut result = template.to_string();

    for (key, value) in context {
      let pattern = format!("{{{{{}}}}}", key);
      let replacement = match value {
        FlowValue::Json(Value::String(s)) => s.clone(),
        FlowValue::Json(v) => v.to_string().trim_matches('"').to_string(),
        FlowValue::File { path, .. } => path.to_string_lossy().to_string(),
        FlowValue::Url { url, .. } => url.clone(),
      };
      result = result.replace(&pattern, &replacement);
    }

    for (key, value) in &self.variables {
      let pattern = format!("{{{{{}}}}}", key);
      result = result.replace(&pattern, value);
    }

    Ok(result)
  }
}

#[async_trait]
impl AsyncNode for TemplateNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut context = inputs.clone();
        for (key, value) in &self.variables {
            context.insert(key.clone(), FlowValue::Json(Value::String(value.clone())));
        }

        let template = self.template.clone();
        let output_format = self.output_format.clone();

        println!("📝 Rendering template for node '{}'", self.name);

        let rendered = self.render_template(&template, &context)?;

        let result_value = match output_format.as_str() {
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