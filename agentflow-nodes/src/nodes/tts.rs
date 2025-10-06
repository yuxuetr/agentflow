//! TTS Node - Converts text to speech using a specified model and voice.

use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use agentflow_llm::{AgentFlow, providers::stepfun::TTSBuilder};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioResponseFormat {
    Wav, Mp3, Flac, Opus
}

impl Default for AudioResponseFormat {
    fn default() -> Self { AudioResponseFormat::Mp3 }
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

        AgentFlow::init().await.map_err(|e| AgentFlowError::ConfigurationError {
            message: format!("Failed to initialize AgentFlow LLM service: {}", e),
        })?;

        let mut resolved_input = self.input_template.clone();
        for key in &self.input_keys {
            if let Some(value) = inputs.get(key) {
                let placeholder = format!("{{{{{}}}}}", key);
                resolved_input = resolved_input.replace(&placeholder, &Self::flow_value_to_string(value));
            }
        }

        let api_key = std::env::var("STEPFUN_API_KEY")
            .or_else(|_| std::env::var("AGENTFLOW_STEPFUN_API_KEY"))
            .map_err(|_| AgentFlowError::ConfigurationError {
                message: "StepFun API key not found".to_string(),
            })?;

        let stepfun_client = AgentFlow::stepfun_client(&api_key).await.map_err(|e| AgentFlowError::ConfigurationError { 
            message: format!("Failed to create stepfun client: {}", e)
        })?;

        let format_str = match self.response_format {
            AudioResponseFormat::Wav => "wav",
            AudioResponseFormat::Mp3 => "mp3",
            AudioResponseFormat::Flac => "flac",
            AudioResponseFormat::Opus => "opus",
        };

        let mut builder = TTSBuilder::new(&self.model, &resolved_input, &self.voice)
            .response_format(format_str);

        if let Some(speed) = self.speed {
            builder = builder.speed(speed);
        }

        let request = builder.build();

        println!("   Calling StepFun text_to_speech API...");
        let audio_data = stepfun_client.text_to_speech(request).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError { message: format!("StepFun text_to_speech failed: {}", e) }
        })?;

        let mime_type = match self.response_format {
            AudioResponseFormat::Wav => "audio/wav",
            AudioResponseFormat::Mp3 => "audio/mpeg",
            AudioResponseFormat::Flac => "audio/flac",
            AudioResponseFormat::Opus => "audio/opus",
        };

        let base64_data = STANDARD.encode(&audio_data);
        let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

        println!("✅ TTSNode execution successful.");
        let mut outputs = HashMap::new();
        outputs.insert(self.output_key.clone(), FlowValue::Json(Value::String(data_uri)));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    #[test]
    fn test_tts_node() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let node = TTSNode {
                name: "test_tts".to_string(),
                model: "step-tts-mini".to_string(),
                voice: "default_voice".to_string(),
                input_template: "Hello {{name}}!".to_string(),
                response_format: AudioResponseFormat::Mp3,
                speed: Some(1.2),
                output_key: "audio_output".to_string(),
                input_keys: vec!["name".to_string()],
            };

            let mut inputs = AsyncNodeInputs::new();
            inputs.insert("name".to_string(), FlowValue::Json(Value::String("world".to_string())));

            if std::env::var("STEPFUN_API_KEY").is_err() {
                println!("Skipping API call in test mode as STEPFUN_API_KEY is not set.");
                return;
            }
            
            let result = node.execute(&inputs).await;
            assert!(result.is_ok());

            let outputs = result.unwrap();
            let output_value = outputs.get("audio_output").unwrap();
            if let FlowValue::Json(Value::String(data)) = output_value {
                assert!(data.starts_with("data:audio/mpeg;base64,"));
            } else {
                panic!("Output was not a FlowValue::Json(Value::String(...))");
            }
        });
    }
}
