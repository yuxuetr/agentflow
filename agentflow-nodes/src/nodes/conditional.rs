use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Conditional node for flow control based on conditions
#[derive(Debug, Clone)]
pub struct ConditionalNode {
  pub name: String,
  pub condition: String, // JavaScript-like expression or template
  pub true_value: Option<Value>,
  pub false_value: Option<Value>,
  pub condition_type: ConditionType,
}

#[derive(Debug, Clone)]
pub enum ConditionType {
  /// Simple variable existence check
  Exists,
  /// Equality check
  Equals(String),
  /// Greater than comparison (for numbers)
  GreaterThan(f64),
  /// Less than comparison (for numbers)
  LessThan(f64),
  /// Contains check (for strings/arrays)
  Contains(String),
  /// Custom expression (future: could support more complex logic)
  Expression,
}

impl ConditionalNode {
  pub fn new(name: &str, condition: &str) -> Self {
    Self {
      name: name.to_string(),
      condition: condition.to_string(),
      true_value: None,
      false_value: None,
      condition_type: ConditionType::Exists,
    }
  }

  pub fn with_condition_type(mut self, condition_type: ConditionType) -> Self {
    self.condition_type = condition_type;
    self
  }

  pub fn with_true_value(mut self, value: Value) -> Self {
    self.true_value = Some(value);
    self
  }

  pub fn with_false_value(mut self, value: Value) -> Self {
    self.false_value = Some(value);
    self
  }

  pub fn equals(name: &str, variable: &str, expected: &str) -> Self {
    Self::new(name, variable).with_condition_type(ConditionType::Equals(expected.to_string()))
  }

  pub fn exists(name: &str, variable: &str) -> Self {
    Self::new(name, variable).with_condition_type(ConditionType::Exists)
  }

  pub fn greater_than(name: &str, variable: &str, threshold: f64) -> Self {
    Self::new(name, variable).with_condition_type(ConditionType::GreaterThan(threshold))
  }

  pub fn contains(name: &str, variable: &str, substring: &str) -> Self {
    Self::new(name, variable).with_condition_type(ConditionType::Contains(substring.to_string()))
  }

  /// Evaluate the condition against inputs
  fn evaluate_condition(&self, inputs: &AsyncNodeInputs) -> Result<bool, AgentFlowError> {
    let var_name = self.condition.as_str();

    match &self.condition_type {
        ConditionType::Exists => Ok(inputs.contains_key(var_name)),
        ConditionType::Equals(expected) => {
            if let Some(value) = inputs.get(var_name) {
                let actual = match value {
                    FlowValue::Json(Value::String(s)) => s.clone(),
                    FlowValue::Json(v) => v.to_string(),
                    FlowValue::String(s) => s.clone(),
                    _ => "".to_string(),
                };
                Ok(actual == *expected)
            } else {
                Ok(false)
            }
        }
        ConditionType::GreaterThan(threshold) => {
            if let Some(value) = inputs.get(var_name) {
                if let Some(number) = value.as_f64() {
                    Ok(number > *threshold)
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        }
        ConditionType::LessThan(threshold) => {
            if let Some(value) = inputs.get(var_name) {
                if let Some(number) = value.as_f64() {
                    Ok(number < *threshold)
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        }
        ConditionType::Contains(substring) => {
            if let Some(value) = inputs.get(var_name) {
                match value {
                    FlowValue::String(s) => Ok(s.contains(substring)),
                    FlowValue::Json(Value::String(s)) => Ok(s.contains(substring)),
                    FlowValue::Json(Value::Array(arr)) => Ok(arr.iter().any(|item| {
                        if let Value::String(s) = item {
                            s == substring
                        } else {
                            false
                        }
                    })),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        ConditionType::Expression => {
            // For now, just check if the condition string evaluates to true
            // Future: could implement proper expression parsing
            if let Some(FlowValue::Json(Value::Bool(b))) = inputs.get(var_name) {
                Ok(*b)
            } else {
                Ok(false)
            }
        }
    }
  }
}

#[async_trait]
impl AsyncNode for ConditionalNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let condition_result = self.evaluate_condition(inputs)?;

        println!("🔧 Conditional Node '{}' prepared:", self.name);
        println!("   Condition: {}", self.condition);
        println!("   Type: {:?}", self.condition_type);
        println!("   Result: {}", condition_result);

        let result = if condition_result {
            self.true_value.clone().unwrap_or(Value::Bool(true))
        } else {
            self.false_value.clone().unwrap_or(Value::Bool(false))
        };

        println!("✅ Conditional result: {}", result);

        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), FlowValue::Json(result));
        outputs.insert("branch".to_string(), FlowValue::String(if condition_result { "true".to_string() } else { "false".to_string() }));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_conditional_node_exists() {
        let node = ConditionalNode::exists("test", "my_var");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("my_var".to_string(), FlowValue::String("some_value".to_string()));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!(true)));
    }

    #[tokio::test]
    async fn test_conditional_node_equals() {
        let node = ConditionalNode::equals("test", "my_var", "expected_value");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("my_var".to_string(), FlowValue::String("expected_value".to_string()));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!(true)));
    }

    #[tokio::test]
    async fn test_conditional_node_greater_than() {
        let node = ConditionalNode::greater_than("test", "my_var", 10.0);
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("my_var".to_string(), FlowValue::Json(json!(15.0)));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!(true)));
    }

    #[tokio::test]
    async fn test_conditional_node_contains() {
        let node = ConditionalNode::contains("test", "my_var", "sub");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("my_var".to_string(), FlowValue::String("substring".to_string()));

        let result = node.execute(&inputs).await.unwrap();
        let output = result.get("output").unwrap();
        assert_eq!(output, &FlowValue::Json(json!(true)));
    }
}