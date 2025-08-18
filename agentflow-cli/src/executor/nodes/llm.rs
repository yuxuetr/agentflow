// LLM node implementation integrated with agentflow-core
use anyhow::Context;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use agentflow_core::{AsyncNode, MetricsCollector, Result, SharedState};
use agentflow_llm::{client::llm_client::LLMClientBuilder, registry::ModelRegistry};

use crate::config::workflow::{LlmNodeConfig, NodeConfig, NodeDefinition};

pub struct LlmNode {
  name: String,
  config: LlmNodeConfig,
  next_actions: Option<Vec<String>>,
}

impl LlmNode {
  pub async fn new(node_def: &NodeDefinition) -> Result<Self> {
    let config = match &node_def.config {
      NodeConfig::Llm(llm_config) => llm_config.clone(),
      _ => {
        return Err(agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
          "Invalid configuration for LLM node"
        )))
      }
    };

    // Note: Template expansion will happen during execution with access to SharedState
    // The config values may contain templates like "{{ inputs.model }}" which will be expanded later

    let next_actions = node_def
      .outputs
      .as_ref()
      .map(|outputs| outputs.keys().cloned().collect());

    Ok(Self {
      name: node_def.name.clone(),
      config,
      next_actions,
    })
  }

  fn expand_template(&self, template: &str, shared_state: &SharedState) -> String {
    let mut result = template.to_string();

    // Replace template variables with shared state values
    for (key, value) in shared_state.iter() {
      let placeholder = format!("{{{{{}}}}}", key);
      let replacement = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => serde_json::to_string(&value).unwrap_or_default(),
      };
      result = result.replace(&placeholder, &replacement);
    }

    // Handle input references
    let input_pattern = "{{ inputs.";
    while let Some(start) = result.find(input_pattern) {
      if let Some(end) = result[start..].find("}}") {
        let full_match = &result[start..start + end + 2];
        let key_part = &full_match[10..full_match.len() - 2]; // Remove {{ inputs. and }}
        let input_key = format!("input_{}", key_part);

        if let Some(value) = shared_state.get(&input_key) {
          let replacement = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => serde_json::to_string(&value).unwrap_or_default(),
          };
          result = result.replace(full_match, &replacement);
        } else {
          result = result.replace(full_match, "");
        }
      } else {
        break;
      }
    }

    result
  }

  async fn call_llm(&self, prompt: &str, system_prompt: Option<&str>) -> Result<String> {
    self
      .call_llm_with_expanded(
        prompt,
        system_prompt,
        &self.config.model,
        self.config.temperature.clone(),
        self.config.max_tokens.clone(),
      )
      .await
  }

  async fn call_llm_with_expanded(
    &self,
    prompt: &str,
    system_prompt: Option<&str>,
    model: &str,
    temperature: Option<Value>,
    max_tokens: Option<Value>,
  ) -> Result<String> {
    // Initialize ModelRegistry if not already done
    let registry = ModelRegistry::global();
    let _ = registry.load_builtin_config().await.map_err(|e| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
        "Failed to load model registry config: {:?}",
        e
      ))
    })?;

    // Create LLM client using builder pattern
    let mut client_builder = LLMClientBuilder::new(model).prompt(prompt);

    // Add system prompt if provided
    if let Some(system) = system_prompt {
      // For now, we'll prepend system message to the prompt
      // TODO: Use proper system message support when available in LLMClientBuilder
      let full_prompt = format!("System: {}\n\nUser: {}", system, prompt);
      client_builder = client_builder.prompt(&full_prompt);
    }

    // Add optional parameters using expanded values
    if let Some(temp) = temperature {
      match temp {
        serde_json::Value::Number(n) => {
          if let Some(f) = n.as_f64() {
            client_builder = client_builder.temperature(f as f32);
          }
        }
        serde_json::Value::String(s) => {
          // This should not happen now since we expand in prep_async
          println!(
            "Warning: Unexpanded template string found for temperature: {}",
            s
          );
        }
        _ => {}
      }
    }
    if let Some(max_tokens_val) = max_tokens {
      match max_tokens_val {
        serde_json::Value::Number(n) => {
          if let Some(i) = n.as_u64() {
            client_builder = client_builder.max_tokens(i as u32);
          }
        }
        serde_json::Value::String(s) => {
          // This should not happen now since we expand in prep_async
          println!(
            "Warning: Unexpanded template string found for max_tokens: {}",
            s
          );
        }
        _ => {}
      }
    }
    if let Some(top_p) = self.config.top_p {
      client_builder = client_builder.top_p(top_p);
    }
    if let Some(freq_penalty) = self.config.frequency_penalty {
      client_builder = client_builder.frequency_penalty(freq_penalty);
    }
    if let Some(ref stop_sequences) = self.config.stop {
      client_builder = client_builder.stop(stop_sequences.clone());
    }

    // Execute the request
    let response = client_builder
      .execute()
      .await
      .with_context(|| format!("Failed to call LLM model: {}", model))
      .map_err(agentflow_core::AgentFlowError::Generic)?;

    Ok(response)
  }
}

#[async_trait]
impl AsyncNode for LlmNode {
  async fn prep_async(&self, shared_state: &SharedState) -> Result<Value> {
    // Expand template variables in prompt and system message
    let expanded_prompt = self.expand_template(&self.config.prompt, shared_state);
    let expanded_system = self
      .config
      .system
      .as_ref()
      .map(|s| self.expand_template(s, shared_state));

    // Expand template variables in model name
    let expanded_model = self.expand_template(&self.config.model, shared_state);

    // Expand template variables in temperature if it's a string
    let expanded_temperature = self.config.temperature.as_ref().map(|temp| {
      match temp {
        Value::String(s) => {
          let expanded = self.expand_template(s, shared_state);
          // Try to parse as number
          if let Ok(f) = expanded.parse::<f64>() {
            Value::Number(
              serde_json::Number::from_f64(f)
                .unwrap_or_else(|| serde_json::Number::from_f64(0.7).unwrap()),
            )
          } else {
            temp.clone()
          }
        }
        _ => temp.clone(),
      }
    });

    // Expand template variables in max_tokens if it's a string
    let expanded_max_tokens = self.config.max_tokens.as_ref().map(|tokens| {
      match tokens {
        Value::String(s) => {
          let expanded = self.expand_template(s, shared_state);
          // Try to parse as number
          if let Ok(i) = expanded.parse::<u64>() {
            Value::Number(serde_json::Number::from(i))
          } else {
            tokens.clone()
          }
        }
        _ => tokens.clone(),
      }
    });

    // Store expanded values for exec phase
    let prep_data = serde_json::json!({
      "expanded_prompt": expanded_prompt,
      "expanded_system": expanded_system,
      "expanded_model": expanded_model,
      "expanded_temperature": expanded_temperature,
      "expanded_max_tokens": expanded_max_tokens,
      "node_name": self.name
    });

    Ok(prep_data)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let expanded_prompt = prep_result["expanded_prompt"].as_str().ok_or_else(|| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
        "Missing expanded_prompt in prep result"
      ))
    })?;

    let expanded_system = prep_result["expanded_system"].as_str();
    let expanded_model = prep_result["expanded_model"].as_str().ok_or_else(|| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
        "Missing expanded_model in prep result"
      ))
    })?;

    // Handle potentially null values
    let expanded_temperature = if prep_result["expanded_temperature"].is_null() {
      None
    } else {
      Some(prep_result["expanded_temperature"].clone())
    };

    let expanded_max_tokens = if prep_result["expanded_max_tokens"].is_null() {
      None
    } else {
      Some(prep_result["expanded_max_tokens"].clone())
    };

    // Call the LLM with expanded values
    let response_text = self
      .call_llm_with_expanded(
        expanded_prompt,
        expanded_system,
        expanded_model,
        expanded_temperature,
        expanded_max_tokens,
      )
      .await?;

    let exec_result = serde_json::json!({
      "response": response_text,
      "model": expanded_model,
      "prompt": expanded_prompt,
      "system": expanded_system,
    });

    Ok(exec_result)
  }

  async fn post_async(
    &self,
    shared_state: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Store LLM response in shared state
    let response_text = exec_result["response"].as_str().ok_or_else(|| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!("Missing response in exec result"))
    })?;

    // Store the response with the node name
    shared_state.insert(
      format!("{}_response", self.name),
      Value::String(response_text.to_string()),
    );
    shared_state.insert(format!("{}_executed", self.name), Value::Bool(true));

    // Store full execution result
    shared_state.insert(format!("{}_result", self.name), exec_result);

    // Return next action based on node configuration
    if let Some(actions) = &self.next_actions {
      if let Some(first_action) = actions.first() {
        Ok(Some(first_action.clone()))
      } else {
        Ok(None)
      }
    } else {
      Ok(None)
    }
  }

  async fn run_async_with_observability(
    &self,
    shared_state: &SharedState,
    metrics_collector: Option<Arc<MetricsCollector>>,
  ) -> Result<Option<String>> {
    let start_time = std::time::Instant::now();

    // Record execution start
    if let Some(ref collector) = metrics_collector {
      let event = agentflow_core::observability::ExecutionEvent {
        node_id: self.name.clone(),
        event_type: "llm_node_start".to_string(),
        timestamp: start_time,
        duration_ms: None,
        metadata: HashMap::from([("model".to_string(), self.config.model.clone())]),
      };
      collector.record_event(event);
    }

    // Execute the standard AsyncNode flow
    let prep_result = self.prep_async(shared_state).await?;
    let exec_result = self.exec_async(prep_result.clone()).await?;
    let next_action = self
      .post_async(shared_state, prep_result, exec_result)
      .await?;

    let duration = start_time.elapsed();

    // Record execution completion
    if let Some(ref collector) = metrics_collector {
      let event = agentflow_core::observability::ExecutionEvent {
        node_id: self.name.clone(),
        event_type: "llm_node_complete".to_string(),
        timestamp: start_time,
        duration_ms: Some(duration.as_millis() as u64),
        metadata: HashMap::from([
          ("model".to_string(), self.config.model.clone()),
          (
            "duration_ms".to_string(),
            (duration.as_millis() as u64).to_string(),
          ),
        ]),
      };
      collector.record_event(event);

      collector.increment_counter(&format!("{}.execution_count", self.name), 1.0);
      collector.increment_counter(
        &format!("{}.duration_ms", self.name),
        duration.as_millis() as f64,
      );
    }

    Ok(next_action)
  }
}
