//! ImageUnderstand Node - Specialized node for multimodal image understanding using vision models

use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use agentflow_llm::{AgentFlow, multimodal::{MultimodalMessage, MessageContent}};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUnderstandNode {
    pub name: String,
    pub model: String,
    pub text_prompt: String,
    pub image_source: String,
    pub system_message: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub output_key: String,
    pub input_keys: Vec<String>,
}

impl ImageUnderstandNode {
    pub fn new(name: &str, model: &str, text_prompt: &str, image_source: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            text_prompt: text_prompt.to_string(),
            image_source: image_source.to_string(),
            system_message: None,
            temperature: None,
            max_tokens: None,
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

    async fn load_image_as_base64(&self, source: &str, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
        if let Some(value) = inputs.get(source) {
            return match value {
                FlowValue::Json(Value::String(s)) => Ok(s.clone()),
                FlowValue::File { path, .. } => {
                    let data = tokio::fs::read(path).await.map_err(|e| AgentFlowError::NodeInputError {
                        message: format!("Failed to read image file at {:?}: {}", path, e),
                    })?;
                    let mime_type = mime_guess::from_path(path).first_or_octet_stream();
                    Ok(format!("data:{};base64,{}", mime_type, STANDARD.encode(data)))
                },
                FlowValue::Url { url, .. } => Ok(url.clone()),
                _ => Err(AgentFlowError::NodeInputError { 
                    message: format!("Unsupported FlowValue type for image source '{}'", source) 
                }),
            }
        }

        if source.starts_with("http") || source.starts_with("data:") {
            return Ok(source.to_string());
        }

        let data = tokio::fs::read(source).await.map_err(|e| AgentFlowError::NodeInputError {
            message: format!("Failed to read image file at {}: {}", source, e),
        })?;
        let mime_type = mime_guess::from_path(source).first_or_octet_stream();
        Ok(format!("data:{};base64,{}", mime_type, STANDARD.encode(data)))
    }
}

#[async_trait]
impl AsyncNode for ImageUnderstandNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        println!("🔍 Executing ImageUnderstandNode: {}", self.name);

        AgentFlow::init().await.map_err(|e| AgentFlowError::ConfigurationError {
            message: format!("Failed to initialize AgentFlow LLM service: {}", e),
        })?;

        let mut resolved_prompt = self.text_prompt.clone();
        for key in &self.input_keys {
            if let Some(value) = inputs.get(key) {
                let placeholder = format!("{{{{{}}}}}", key);
                resolved_prompt = resolved_prompt.replace(&placeholder, &Self::flow_value_to_string(value));
            }
        }

        let image_data_uri = self.load_image_as_base64(&self.image_source, inputs).await?;

        let message = MultimodalMessage::user()
            .add_text(resolved_prompt)
            .add_image_url(image_data_uri)
            .build();

        let mut request = AgentFlow::model(&self.model).multimodal_prompt(message);

        if let Some(system_message) = &self.system_message {
            request = request.system(system_message);
        }
        if let Some(temp) = self.temperature {
            request = request.temperature(temp);
        }
        if let Some(max_tokens) = self.max_tokens {
            request = request.max_tokens(max_tokens);
        }

        let response = request.execute().await.map_err(|e| {
            AgentFlowError::AsyncExecutionError { message: format!("LLM execution failed: {}", e) }
        })?;

        println!("✅ ImageUnderstandNode execution successful.");
        let mut outputs = HashMap::new();
        outputs.insert(self.output_key.clone(), FlowValue::Json(Value::String(response)));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    const TEST_IMAGE_BASE64: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

    #[test]
    fn test_image_understand_node() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let node = ImageUnderstandNode::new(
                "test_vision",
                "step-1o-turbo-vision",
                "what is in this image?",
                "image_input"
            );

            let mut inputs = AsyncNodeInputs::new();
            inputs.insert("image_input".to_string(), FlowValue::Json(Value::String(TEST_IMAGE_BASE64.to_string())));

            if std::env::var("STEPFUN_API_KEY").is_err() {
                println!("Skipping API call in test mode as STEPFUN_API_KEY is not set.");
                return;
            }
            
            let result = node.execute(&inputs).await;
            assert!(result.is_ok());

            let outputs = result.unwrap();
            let output_value = outputs.get("test_vision_output").unwrap();
            if let FlowValue::Json(Value::String(s)) = output_value {
                assert!(!s.is_empty());
            } else {
                panic!("Output was not a FlowValue::Json(Value::String(...))");
            }
        });
    }
}
