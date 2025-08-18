// Execution context management
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::config::workflow::WorkflowConfig;

pub struct ExecutionContext {
  pub variables: HashMap<String, Value>,
  pub working_directory: String,
  pub environment: HashMap<String, String>,
}

impl ExecutionContext {
  pub fn new(config: &WorkflowConfig) -> Result<Self> {
    let mut context = Self {
      variables: HashMap::new(),
      working_directory: std::env::current_dir()?.to_string_lossy().to_string(),
      environment: HashMap::new(),
    };

    // Load environment variables from config
    if let Some(env) = &config.environment {
      for (key, value) in env {
        context.environment.insert(key.clone(), value.clone());
      }
    }

    // Add workflow metadata to variables
    context.variables.insert(
      "workflow_name".to_string(),
      Value::String(config.name.clone()),
    );
    context.variables.insert(
      "workflow_version".to_string(),
      Value::String(config.version.clone()),
    );

    if let Some(description) = &config.description {
      context.variables.insert(
        "workflow_description".to_string(),
        Value::String(description.clone()),
      );
    }

    Ok(context)
  }

  pub fn get_variable(&self, key: &str) -> Option<&Value> {
    self.variables.get(key)
  }

  pub fn set_variable(&mut self, key: String, value: Value) {
    self.variables.insert(key, value);
  }

  pub fn get_environment(&self, key: &str) -> Option<&String> {
    self.environment.get(key)
  }

  pub fn expand_template(&self, template: &str) -> String {
    let mut result = template.to_string();

    // Simple template expansion - replace {{ key }} with values
    for (key, value) in &self.variables {
      let placeholder = format!("{{{{{}}}}}", key);
      let replacement = match value {
        Value::String(s) => s.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
      };
      result = result.replace(&placeholder, &replacement);
    }

    // Expand environment variables
    for (key, value) in &self.environment {
      let placeholder = format!("{{{{{}}}}}", key);
      result = result.replace(&placeholder, value);
    }

    result
  }
}
