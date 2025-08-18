use crate::nodes::LlmNode; // Import our code-first LLM node
use crate::{
  config::{NodeConfig, WorkflowConfig},
  AsyncNode, SharedState,
};
use serde_json::Value;
use std::collections::HashMap;

/// Configuration-first workflow runner that builds code-first nodes
pub struct ConfigWorkflowRunner {
  config: WorkflowConfig,
}

impl ConfigWorkflowRunner {
  /// Create a new workflow runner from configuration
  pub async fn from_file(config_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
    let config = WorkflowConfig::from_file(config_path).await?;
    config
      .validate()
      .map_err(|e| format!("Configuration validation failed: {}", e))?;

    Ok(Self { config })
  }

  /// Execute the workflow with the given inputs
  pub async fn run(
    &self,
    inputs: HashMap<String, String>,
  ) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    println!(
      "🚀 Running configuration-first workflow: {}",
      self.config.name
    );

    // Initialize shared state with inputs
    let shared_state = SharedState::new();
    self.populate_inputs(&shared_state, inputs)?;

    // Execute nodes in sequence
    for node_config in &self.config.workflow {
      let node = self.create_node(node_config)?;

      println!("▶️  Executing node: {}", self.get_node_name(node_config));
      match node.run_async(&shared_state).await {
        Ok(_) => {
          println!("✅ Node completed successfully");
        }
        Err(e) => {
          println!("❌ Node failed: {:?}", e);
          return Err(format!("Node '{}' failed: {:?}", self.get_node_name(node_config), e).into());
        }
      }
    }

    // Collect outputs
    let outputs = self.collect_outputs(&shared_state)?;

    println!("🎯 Workflow completed successfully!");
    Ok(outputs)
  }

  /// Populate shared state with validated inputs
  fn populate_inputs(
    &self,
    shared_state: &SharedState,
    inputs: HashMap<String, String>,
  ) -> Result<(), String> {
    if let Some(input_configs) = &self.config.inputs {
      for (key, input_config) in input_configs {
        if let Some(value_str) = inputs.get(key) {
          // Parse based on type
          let value = self.parse_input_value(value_str, &input_config.input_type)?;
          shared_state.insert(key.clone(), value);
        } else if input_config.required.unwrap_or(false) {
          return Err(format!("Required input '{}' not provided", key));
        } else if let Some(default_value) = &input_config.default {
          shared_state.insert(key.clone(), default_value.clone());
        }
      }
    }

    // Add raw inputs for convenience
    for (key, value) in inputs {
      shared_state.insert(format!("input_{}", key), Value::String(value));
    }

    Ok(())
  }

  /// Parse input value based on type
  fn parse_input_value(&self, value_str: &str, input_type: &str) -> Result<Value, String> {
    match input_type {
      "string" => Ok(Value::String(value_str.to_string())),
      "number" => {
        if let Ok(int_val) = value_str.parse::<i64>() {
          Ok(Value::Number(serde_json::Number::from(int_val)))
        } else if let Ok(float_val) = value_str.parse::<f64>() {
          Ok(Value::Number(
            serde_json::Number::from_f64(float_val).unwrap(),
          ))
        } else {
          Err(format!("Invalid number: {}", value_str))
        }
      }
      "boolean" => match value_str.to_lowercase().as_str() {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ => Err(format!("Invalid boolean: {}", value_str)),
      },
      _ => Ok(Value::String(value_str.to_string())), // Default to string
    }
  }

  /// Create a code-first node from configuration
  fn create_node(&self, node_config: &NodeConfig) -> Result<Box<dyn AsyncNode>, String> {
    match node_config {
      NodeConfig::Llm {
        name,
        model,
        prompt,
        system,
        temperature,
        max_tokens,
        ..
      } => {
        let mut llm_node = LlmNode::new(name, model).with_prompt(prompt);

        if let Some(sys) = system {
          llm_node = llm_node.with_system(sys);
        }

        if let Some(temp) = temperature {
          if let Some(temp_f64) = temp.as_f64() {
            llm_node = llm_node.with_temperature(temp_f64 as f32);
          }
          // If it's a string template, it will be resolved during execution
        }

        if let Some(tokens) = max_tokens {
          if let Some(tokens_u64) = tokens.as_u64() {
            llm_node = llm_node.with_max_tokens(tokens_u64 as u32);
          }
          // If it's a string template, it will be resolved during execution
        }

        Ok(Box::new(llm_node))
      }

      NodeConfig::Http { name, .. } => {
        // TODO: Implement HTTP node
        Err(format!("HTTP node '{}' not yet implemented", name))
      }

      NodeConfig::File { name, .. } => {
        // TODO: Implement File node
        Err(format!("File node '{}' not yet implemented", name))
      }
    }
  }

  /// Get node name from configuration
  fn get_node_name<'a>(&self, node_config: &'a NodeConfig) -> &'a str {
    match node_config {
      NodeConfig::Llm { name, .. } => name,
      NodeConfig::Http { name, .. } => name,
      NodeConfig::File { name, .. } => name,
    }
  }

  /// Collect outputs based on configuration
  fn collect_outputs(&self, shared_state: &SharedState) -> Result<HashMap<String, Value>, String> {
    let mut outputs = HashMap::new();

    if let Some(output_configs) = &self.config.outputs {
      for (output_name, output_config) in output_configs {
        let parts: Vec<&str> = output_config.from.split('.').collect();
        if parts.len() != 2 {
          return Err(format!("Invalid output reference: {}", output_config.from));
        }

        let node_name = parts[0];
        let field_name = parts[1];

        // Look for the output in shared state
        let output_key = if field_name == "response" {
          // Special case: "response" maps to the main output
          format!("{}_output", node_name)
        } else {
          format!("{}_{}", node_name, field_name)
        };

        if let Some(value) = shared_state.get(&output_key) {
          outputs.insert(output_name.clone(), value);
        } else {
          return Err(format!(
            "Output '{}' not found in shared state (looking for key '{}')",
            output_name, output_key
          ));
        }
      }
    }

    Ok(outputs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio;

  #[tokio::test]
  async fn test_input_parsing() {
    let runner = ConfigWorkflowRunner {
      config: WorkflowConfig {
        name: "Test".to_string(),
        description: None,
        inputs: Some(HashMap::from([
          (
            "question".to_string(),
            InputConfig {
              input_type: "string".to_string(),
              required: Some(true),
              default: None,
              description: None,
            },
          ),
          (
            "temperature".to_string(),
            InputConfig {
              input_type: "number".to_string(),
              required: Some(false),
              default: Some(Value::Number(serde_json::Number::from_f64(0.7).unwrap())),
              description: None,
            },
          ),
        ])),
        workflow: vec![],
        outputs: None,
      },
    };

    let shared_state = SharedState::new();
    let inputs = HashMap::from([("question".to_string(), "What is 2+2?".to_string())]);

    let result = runner.populate_inputs(&shared_state, inputs);
    assert!(result.is_ok());

    assert_eq!(
      shared_state.get("question"),
      Some(Value::String("What is 2+2?".to_string()))
    );
    assert_eq!(
      shared_state.get("temperature"),
      Some(Value::Number(serde_json::Number::from_f64(0.7).unwrap()))
    );
  }
}
