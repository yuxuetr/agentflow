use crate::{AsyncNode, SharedState, NodeError};
use agentflow_core::{AgentFlowError, Result};
use async_trait::async_trait;
use serde_json::Value;

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
        Self::new(name, variable)
            .with_condition_type(ConditionType::Equals(expected.to_string()))
    }

    pub fn exists(name: &str, variable: &str) -> Self {
        Self::new(name, variable)
            .with_condition_type(ConditionType::Exists)
    }

    pub fn greater_than(name: &str, variable: &str, threshold: f64) -> Self {
        Self::new(name, variable)
            .with_condition_type(ConditionType::GreaterThan(threshold))
    }

    pub fn contains(name: &str, variable: &str, substring: &str) -> Self {
        Self::new(name, variable)
            .with_condition_type(ConditionType::Contains(substring.to_string()))
    }

    /// Evaluate the condition against shared state
    fn evaluate_condition(&self, shared: &SharedState) -> Result<bool, NodeError> {
        // Resolve condition variable name (could be a template)
        let var_name = shared.resolve_template_advanced(&self.condition);
        
        match &self.condition_type {
            ConditionType::Exists => {
                Ok(shared.get(&var_name).is_some())
            },
            ConditionType::Equals(expected) => {
                if let Some(value) = shared.get(&var_name) {
                    let actual = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => serde_json::to_string(value).unwrap_or_default(),
                    };
                    Ok(actual == *expected)
                } else {
                    Ok(false)
                }
            },
            ConditionType::GreaterThan(threshold) => {
                if let Some(value) = shared.get(&var_name) {
                    if let Some(number) = value.as_f64() {
                        Ok(number > *threshold)
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            },
            ConditionType::LessThan(threshold) => {
                if let Some(value) = shared.get(&var_name) {
                    if let Some(number) = value.as_f64() {
                        Ok(number < *threshold)
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            },
            ConditionType::Contains(substring) => {
                if let Some(value) = shared.get(&var_name) {
                    match value {
                        Value::String(s) => Ok(s.contains(substring)),
                        Value::Array(arr) => {
                            Ok(arr.iter().any(|item| {
                                if let Value::String(s) = item {
                                    s == substring
                                } else {
                                    false
                                }
                            }))
                        },
                        _ => Ok(false),
                    }
                } else {
                    Ok(false)
                }
            },
            ConditionType::Expression => {
                // For now, just check if the condition string evaluates to true
                // Future: could implement proper expression parsing
                Ok(!var_name.is_empty() && var_name != "false" && var_name != "0")
            },
        }
    }
}

#[async_trait]
impl AsyncNode for ConditionalNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        let condition_result = self.evaluate_condition(shared).map_err(|e| {
            AgentFlowError::AsyncExecutionError {
                message: format!("Condition evaluation failed: {}", e),
            }
        })?;

        println!("🔧 Conditional Node '{}' prepared:", self.name);
        println!("   Condition: {}", self.condition);
        println!("   Type: {:?}", self.condition_type);
        println!("   Result: {}", condition_result);

        Ok(serde_json::json!({
            "condition": self.condition,
            "condition_type": format!("{:?}", self.condition_type),
            "condition_result": condition_result,
            "true_value": self.true_value,
            "false_value": self.false_value
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let condition_result = prep_result["condition_result"].as_bool()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
                message: "Invalid condition result".to_string(),
            })?;

        println!("🔀 Conditional execution: {}", condition_result);

        let result = if condition_result {
            self.true_value.clone().unwrap_or(Value::Bool(true))
        } else {
            self.false_value.clone().unwrap_or(Value::Bool(false))
        };

        println!("✅ Conditional result: {}", result);

        Ok(serde_json::json!({
            "condition_result": condition_result,
            "selected_value": result,
            "branch": if condition_result { "true" } else { "false" }
        }))
    }

    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        // Store the conditional result
        let output_key = format!("{}_result", self.name);
        shared.insert(output_key.clone(), exec_result["selected_value"].clone());

        // Store metadata about the condition
        let metadata_key = format!("{}_metadata", self.name);
        shared.insert(metadata_key, serde_json::json!({
            "condition_result": exec_result["condition_result"],
            "branch_taken": exec_result["branch"]
        }));

        println!("💾 Stored conditional result in shared state as: {}", output_key);
        
        // Return the branch taken for potential flow control
        Ok(Some(exec_result["branch"].as_str().unwrap_or("false").to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}