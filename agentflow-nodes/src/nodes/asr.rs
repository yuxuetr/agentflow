//! ASR Node - Transcribes audio to text using a specified model.

use crate::common::utils::{flow_value_to_string, load_bytes_from_source};
use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use agentflow_llm::{AgentFlow, providers::stepfun::ASRRequest};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ASRResponseFormat { Json, Text, Srt, Vtt }

impl Default for ASRResponseFormat {
    fn default() -> Self { ASRResponseFormat::Text }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ASRNode {
    pub name: String,
    pub model: String,
    pub audio_source: String,
    pub response_format: ASRResponseFormat,
    pub output_key: String,
    pub input_keys: Vec<String>,
}

impl ASRNode {
    pub fn new(name: &str, model: &str, audio_source: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            audio_source: audio_source.to_string(),
            response_format: Default::default(),
            output_key: format!("{}_output", name),
            input_keys: vec![],
        }
    }
}

#[async_trait]
impl AsyncNode for ASRNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        println!("🎤 Executing ASRNode: {}", self.name);

        AgentFlow::init().await.map_err(|e| AgentFlowError::ConfigurationError {
            message: format!("Failed to initialize AgentFlow LLM service: {}", e),
        })?;

        let mut resolved_source = self.audio_source.clone();
        for key in &self.input_keys {
            if let Some(value) = inputs.get(key) {
                let placeholder = format!("{{{{{}}}}}", key);
                resolved_source = resolved_source.replace(&placeholder, &flow_value_to_string(value));
            }
        }

        let audio_data = load_bytes_from_source(&resolved_source, inputs).await?;

        let api_key = std::env::var("STEPFUN_API_KEY")
            .or_else(|_| std::env::var("AGENTFLOW_STEPFUN_API_KEY"))
            .map_err(|_| AgentFlowError::ConfigurationError {
                message: "StepFun API key not found".to_string(),
            })?;

        let stepfun_client = AgentFlow::stepfun_client(&api_key).await.map_err(|e| AgentFlowError::ConfigurationError { 
            message: format!("Failed to create stepfun client: {}", e)
        })?;

        let format_str = match self.response_format {
            ASRResponseFormat::Json => "json",
            ASRResponseFormat::Text => "text",
            ASRResponseFormat::Srt => "srt",
            ASRResponseFormat::Vtt => "vtt",
        };

        let request = ASRRequest {
            model: self.model.clone(),
            response_format: format_str.to_string(),
            audio_data,
            filename: resolved_source,
        };

        println!("   Calling StepFun speech_to_text API...");
        let transcript = stepfun_client.speech_to_text(request).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError { message: format!("StepFun speech_to_text failed: {}", e) }
        })?;

        println!("✅ ASRNode execution successful.");
        let mut outputs = HashMap::new();
        outputs.insert(self.output_key.clone(), FlowValue::Json(Value::String(transcript)));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[tokio::test]
    async fn test_asr_node_integration() {
        // This test requires a valid STEP_API_KEY to be set in the environment.
        if std::env::var("STEP_API_KEY").is_err() {
            println!("Skipping ASR integration test: STEP_API_KEY not set.");
            return;
        }

        let node = ASRNode {
            name: "test_asr".to_string(),
            model: "step-asr".to_string(),
                            audio_source: "test.wav".to_string(),
                            response_format: ASRResponseFormat::Text,
                            output_key: "transcript_output".to_string(),
                            input_keys: vec!["test.wav".to_string()],
                        };
            
                        let mut inputs = AsyncNodeInputs::new();
                        // A minimal, silent WAV file encoded in base64
                        let audio_data = "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEARKwAAIhYAQACABgAZGF0YQAAAAA=";
                        inputs.insert("test.wav".to_string(), FlowValue::Json(Value::String(audio_data.to_string())));
        let result = node.execute(&inputs).await;
        assert!(result.is_ok(), "Node execution failed: {:?}", result.err());

        let outputs = result.unwrap();
        let output_value = outputs.get("transcript_output").unwrap();
        if let FlowValue::Json(Value::String(s)) = output_value {
            // The silent audio should produce an empty string or a predictable result.
            // For now, we just check that it is a string.
            assert!(s.is_ascii(), "Transcript should be a string.");
        } else {
            panic!("Output was not a FlowValue::Json(Value::String(...))");
        }
    }
}
