//! TTS Node — converts text to speech via the modality dispatcher.
//!
//! Post-P-LLM.3 this node no longer talks to StepFun directly. It picks
//! the vendor via the registry by model name: `AgentFlow::tts(&self.model)`
//! returns a boxed [`agentflow_llm::TtsProvider`] which routes to the
//! right vendor implementation (today StepFun; P-LLM.5 will add others).

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_llm::{AgentFlow, TtsRequest};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum AudioResponseFormat {
  Wav,
  #[default]
  Mp3,
  Flac,
  Opus,
}

impl AudioResponseFormat {
  fn as_wire_str(&self) -> &'static str {
    match self {
      AudioResponseFormat::Wav => "wav",
      AudioResponseFormat::Mp3 => "mp3",
      AudioResponseFormat::Flac => "flac",
      AudioResponseFormat::Opus => "opus",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSNode {
  pub name: String,
  pub model: String,
  pub voice: String,
  pub input_template: String,
  pub response_format: AudioResponseFormat,
  pub speed: Option<f32>,
  pub output_key: String,
  pub input_keys: Vec<String>,
}

impl TTSNode {
  pub fn new(name: &str, model: &str, voice: &str, input_template: &str) -> Self {
    Self {
      name: name.to_string(),
      model: model.to_string(),
      voice: voice.to_string(),
      input_template: input_template.to_string(),
      response_format: Default::default(),
      speed: None,
      output_key: format!("{}_output", name),
      input_keys: vec![],
    }
  }

  fn flow_value_to_string(value: &FlowValue) -> String {
    match value {
      FlowValue::Json(Value::String(s)) => s.clone(),
      FlowValue::Json(v) => v.to_string().trim_matches('"').to_string(),
      FlowValue::File { path, .. } => path.to_string_lossy().to_string(),
      FlowValue::Url { url, .. } => url.clone(),
    }
  }
}

#[async_trait]
impl AsyncNode for TTSNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    println!("🗣️ Executing TTSNode: {}", self.name);

    AgentFlow::init()
      .await
      .map_err(|e| AgentFlowError::ConfigurationError {
        message: format!("Failed to initialize AgentFlow LLM service: {}", e),
      })?;

    let mut resolved_input = self.input_template.clone();
    for key in &self.input_keys {
      if let Some(value) = inputs.get(key) {
        let placeholder = format!("{{{{{}}}}}", key);
        resolved_input = resolved_input.replace(&placeholder, &Self::flow_value_to_string(value));
      }
    }

    // P-LLM.3: route through the modality dispatcher. The registry
    // entry for `self.model` decides which vendor handles the call
    // (today only StepFun; future vendors plug in transparently).
    let provider =
      AgentFlow::tts(&self.model)
        .await
        .map_err(|e| AgentFlowError::ConfigurationError {
          message: format!(
            "Failed to resolve TTS provider for model '{}': {}",
            self.model, e
          ),
        })?;

    let response_format = self.response_format.as_wire_str().to_string();
    let request = TtsRequest {
      model: self.model.clone(),
      input: resolved_input,
      voice: self.voice.clone(),
      response_format: Some(response_format),
      speed: self.speed,
      volume: None,
      sample_rate: None,
    };

    println!(
      "   Synthesizing speech via provider '{}'...",
      provider.name()
    );
    let tts_response =
      provider
        .synthesize(request)
        .await
        .map_err(|e| AgentFlowError::AsyncExecutionError {
          message: format!("TTS synthesis failed: {}", e),
        })?;

    let base64_data = STANDARD.encode(&tts_response.audio);
    let data_uri = format!("data:{};base64,{}", tts_response.mime_type, base64_data);

    println!("✅ TTSNode execution successful.");
    let mut outputs = HashMap::new();
    outputs.insert(
      self.output_key.clone(),
      FlowValue::Json(Value::String(data_uri)),
    );

    Ok(outputs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_tts_node_integration() {
    let node = TTSNode {
      name: "test_tts".to_string(),
      model: "step-tts-mini".to_string(),
      voice: "cixingnansheng".to_string(),
      input_template: "Hello world!".to_string(),
      response_format: AudioResponseFormat::Mp3,
      speed: Some(1.0),
      output_key: "audio_output".to_string(),
      input_keys: vec![],
    };

    let inputs = AsyncNodeInputs::new();

    let result = node.execute(&inputs).await;
    assert!(result.is_ok(), "Node execution failed: {:?}", result.err());

    let outputs = result.unwrap();
    let output_value = outputs.get("audio_output").unwrap();
    if let FlowValue::Json(Value::String(data)) = output_value {
      assert!(data.starts_with("data:audio/mpeg;base64,"));
      let base64_part = data.split(",").nth(1).unwrap();
      assert!(!base64_part.is_empty(), "Base64 data should not be empty");
    } else {
      panic!("Output was not a FlowValue::Json(Value::String(...))");
    }
  }
}
