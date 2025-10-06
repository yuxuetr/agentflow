//! ImageEdit Node - Specialized node for AI-powered image editing (inpainting/outpainting)

use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use agentflow_llm::{AgentFlow, providers::stepfun::ImageEditRequest};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Defines the structure for the ImageEditNode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEditNode {
    pub name: String,
    pub model: String,
    pub prompt: String,
    pub image_source: String,
    pub size: Option<String>,
    pub response_format: Option<String>,
    pub seed: Option<i32>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f32>,
    pub output_key: String,
    pub input_keys: Vec<String>,
}

impl ImageEditNode {
    pub fn new(name: &str, model: &str, prompt: &str, image_source: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            prompt: prompt.to_string(),
            image_source: image_source.to_string(),
            size: Some("1024x1024".to_string()),
            response_format: Some("b64_json".to_string()),
            seed: None,
            steps: None,
            cfg_scale: None,
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

    async fn load_image_bytes(&self, source: &str, inputs: &AsyncNodeInputs) -> Result<Vec<u8>, AgentFlowError> {
        if let Some(value) = inputs.get(source) {
            return match value {
                FlowValue::Json(Value::String(s)) => {
                    if let Some(data) = s.strip_prefix("data:image/") {
                        let parts: Vec<&str> = data.split(";base64,").collect();
                        if parts.len() == 2 {
                            return STANDARD.decode(parts[1]).map_err(|e| AgentFlowError::NodeInputError { 
                                message: format!("Invalid base64 data in input '{}': {}", source, e) 
                            });
                        }
                    }
                    Err(AgentFlowError::NodeInputError { message: format!("Unsupported string format for image source '{}'. Expected base64 data URI.", source) })
                },
                FlowValue::File { path, .. } => {
                    tokio::fs::read(path).await.map_err(|e| AgentFlowError::NodeInputError {
                        message: format!("Failed to read image file at {:?}: {}", path, e),
                    })
                },
                _ => Err(AgentFlowError::NodeInputError { 
                    message: format!("Unsupported FlowValue type for image source '{}'", source) 
                }),
            }
        }

        if source.starts_with("http") {
            let response = reqwest::get(source).await.map_err(|e| AgentFlowError::NodeInputError { 
                message: format!("Failed to download image from URL {}: {}", source, e)
            })?;
            return response.bytes().await.map(|b| b.to_vec()).map_err(|e| AgentFlowError::NodeInputError { 
                message: format!("Failed to read image bytes from URL {}: {}", source, e)
            });
        }

        tokio::fs::read(source).await.map_err(|e| AgentFlowError::NodeInputError {
            message: format!("Failed to read image file at {}: {}", source, e),
        })
    }
}

#[async_trait]
impl AsyncNode for ImageEditNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        println!("🎨 Executing ImageEditNode: {}", self.name);

        let mut resolved_prompt = self.prompt.clone();
        for key in &self.input_keys {
            if let Some(value) = inputs.get(key) {
                let placeholder = format!("{{{{{}}}}}", key);
                resolved_prompt = resolved_prompt.replace(&placeholder, &Self::flow_value_to_string(value));
            }
        }

        let image_data = self.load_image_bytes(&self.image_source, inputs).await?;

        let api_key = std::env::var("STEPFUN_API_KEY")
            .or_else(|_| std::env::var("AGENTFLOW_STEPFUN_API_KEY"))
            .map_err(|_| AgentFlowError::ConfigurationError {
                message: "StepFun API key not found".to_string(),
            })?;

        let stepfun_client = AgentFlow::stepfun_client(&api_key).await.map_err(|e| AgentFlowError::ConfigurationError { 
            message: format!("Failed to create stepfun client: {}", e)
        })?;

        let request = ImageEditRequest {
            model: self.model.clone(),
            prompt: resolved_prompt,
            image_data,
            image_filename: self.image_source.clone(),
            seed: self.seed,
            steps: self.steps,
            cfg_scale: self.cfg_scale,
            size: self.size.clone(),
            response_format: self.response_format.clone(),
        };

        println!("   Calling StepFun edit_image API...");
        let response = stepfun_client.edit_image(request).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError { message: format!("StepFun edit_image failed: {}", e) }
        })?;

        let output_data = if let Some(first_image) = response.data.first() {
            if let Some(b64) = &first_image.b64_json {
                format!("data:image/png;base64,{}", b64)
            } else if let Some(url) = &first_image.url {
                url.clone()
            } else {
                return Err(AgentFlowError::AsyncExecutionError { message: "No image data in response".to_string() });
            }
        } else {
            return Err(AgentFlowError::AsyncExecutionError { message: "No images returned in response".to_string() });
        };

        println!("✅ ImageEditNode execution successful.");
        let mut outputs = HashMap::new();
        outputs.insert(self.output_key.clone(), FlowValue::Json(Value::String(output_data)));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    const TEST_IMAGE_BASE64: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

    #[test]
    fn test_migrate_image_edit_node() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let node = ImageEditNode::new(
                "test_edit",
                "step-1x-edit",
                "add a blue sky",
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
            let output_value = outputs.get("test_edit_output").unwrap();
            if let FlowValue::Json(Value::String(data)) = output_value {
                assert!(data.starts_with("data:image/png;base64,") || data.starts_with("http"));
            } else {
                panic!("Output was not a FlowValue::Json(Value::String(...))");
            }
        });
    }
}
