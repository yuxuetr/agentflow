use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Simplified workflow configuration that maps to code-first nodes
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
  pub name: String,
  pub description: Option<String>,
  pub inputs: Option<HashMap<String, InputConfig>>,
  pub workflow: Vec<NodeConfig>,
  pub outputs: Option<HashMap<String, OutputConfig>>,
}

/// Input parameter configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputConfig {
  #[serde(rename = "type")]
  pub input_type: String,
  pub required: Option<bool>,
  pub default: Option<Value>,
  pub description: Option<String>,
}

/// Node configuration that maps directly to our code-first nodes
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum NodeConfig {
  #[serde(rename = "llm")]
  Llm {
    name: String,
    model: String,
    prompt: String,
    system: Option<String>,
    temperature: Option<Value>, // Can be number or template string
    max_tokens: Option<Value>,  // Can be number or template string
    outputs: Option<HashMap<String, String>>, // output_name -> field_mapping
  },

  #[serde(rename = "http")]
  Http {
    name: String,
    url: String,
    method: Option<String>, // GET, POST, etc.
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
    outputs: Option<HashMap<String, String>>,
  },

  #[serde(rename = "file")]
  File {
    name: String,
    operation: String, // read, write, append
    path: String,
    content: Option<String>, // For write operations
    outputs: Option<HashMap<String, String>>,
  },
}

/// Output configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputConfig {
  pub from: String, // node_name.field_name
  pub description: Option<String>,
}

impl WorkflowConfig {
  /// Load workflow configuration from YAML file
  pub async fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
    let content = tokio::fs::read_to_string(path).await?;
    let config: WorkflowConfig = serde_yaml::from_str(&content)?;
    Ok(config)
  }

  /// Validate the workflow configuration
  pub fn validate(&self) -> Result<(), String> {
    // Check that all nodes have unique names
    let mut node_names = std::collections::HashSet::new();
    for node in &self.workflow {
      let name = match node {
        NodeConfig::Llm { name, .. } => name,
        NodeConfig::Http { name, .. } => name,
        NodeConfig::File { name, .. } => name,
      };

      if !node_names.insert(name.clone()) {
        return Err(format!("Duplicate node name: {}", name));
      }
    }

    // Check that outputs reference valid nodes
    if let Some(outputs) = &self.outputs {
      for (output_name, output_config) in outputs {
        if output_config.from.contains('.') {
          let parts: Vec<&str> = output_config.from.split('.').collect();
          if parts.len() != 2 {
            return Err(format!(
              "Invalid output reference '{}' in output '{}'",
              output_config.from, output_name
            ));
          }

          let node_name = parts[0];
          if !node_names.contains(node_name) {
            return Err(format!(
              "Output '{}' references unknown node '{}'",
              output_name, node_name
            ));
          }
        } else {
          return Err(format!(
            "Output reference '{}' must be in format 'node_name.field_name'",
            output_config.from
          ));
        }
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_workflow_config_parsing() {
    let yaml = r#"
name: "Test Workflow"
description: "A simple test"
inputs:
  question:
    type: string
    required: true
workflow:
  - type: llm
    name: answer
    model: "step-2-mini"
    prompt: "{{ question }}"
    system: "You are helpful"
outputs:
  result:
    from: answer.response
"#;

    let config: WorkflowConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.name, "Test Workflow");
    assert_eq!(config.workflow.len(), 1);

    match &config.workflow[0] {
      NodeConfig::Llm {
        name,
        model,
        prompt,
        ..
      } => {
        assert_eq!(name, "answer");
        assert_eq!(model, "step-2-mini");
        assert_eq!(prompt, "{{ question }}");
      }
      _ => panic!("Expected LLM node"),
    }
  }

  #[test]
  fn test_workflow_validation() {
    let config = WorkflowConfig {
      name: "Test".to_string(),
      description: None,
      inputs: None,
      workflow: vec![NodeConfig::Llm {
        name: "node1".to_string(),
        model: "test".to_string(),
        prompt: "test".to_string(),
        system: None,
        temperature: None,
        max_tokens: None,
        outputs: None,
      }],
      outputs: Some(HashMap::from([(
        "result".to_string(),
        OutputConfig {
          from: "node1.response".to_string(),
          description: None,
        },
      )])),
    };

    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_workflow_validation_duplicate_names() {
    let config = WorkflowConfig {
      name: "Test".to_string(),
      description: None,
      inputs: None,
      workflow: vec![
        NodeConfig::Llm {
          name: "duplicate".to_string(),
          model: "test".to_string(),
          prompt: "test".to_string(),
          system: None,
          temperature: None,
          max_tokens: None,
          outputs: None,
        },
        NodeConfig::Llm {
          name: "duplicate".to_string(),
          model: "test".to_string(),
          prompt: "test".to_string(),
          system: None,
          temperature: None,
          max_tokens: None,
          outputs: None,
        },
      ],
      outputs: None,
    };

    assert!(config.validate().is_err());
    assert!(config
      .validate()
      .unwrap_err()
      .contains("Duplicate node name"));
  }
}
