use agentflow_core::{
    error::AgentFlowError,
    value::FlowValue,
};
use serde_json::Value;
use base64::{engine::general_purpose::STANDARD, Engine as _};

pub fn flow_value_to_string(value: &FlowValue) -> String {
    match value {
        FlowValue::Json(Value::String(s)) => s.clone(),
        FlowValue::Json(v) => v.to_string().trim_matches('"').to_string(),
        FlowValue::File { path, .. } => path.to_string_lossy().to_string(),
        FlowValue::Url { url, .. } => url.clone(),
    }
}

pub async fn load_data_uri_from_source(source: &str, inputs: &agentflow_core::async_node::AsyncNodeInputs) -> Result<String, AgentFlowError> {
    if let Some(value) = inputs.get(source) {
        return match value {
            FlowValue::Json(Value::String(s)) => Ok(s.clone()), // Assume it's already a data URI or URL
            FlowValue::File { path, .. } => {
                let data = tokio::fs::read(path).await.map_err(|e| AgentFlowError::NodeInputError {
                    message: format!("Failed to read file at {:?}: {}", path, e),
                })?;
                let mime_type = mime_guess::from_path(path).first_or_octet_stream();
                Ok(format!("data:{};base64,{}", mime_type, STANDARD.encode(data)))
            },
            FlowValue::Url { url, .. } => Ok(url.clone()),
            _ => Err(AgentFlowError::NodeInputError { 
                message: format!("Unsupported FlowValue type for source '{}'", source) 
            }),
        }
    }

    if source.starts_with("http") || source.starts_with("data:") {
        return Ok(source.to_string());
    }

    let data = tokio::fs::read(source).await.map_err(|e| AgentFlowError::NodeInputError {
        message: format!("Failed to read file at {}: {}", source, e),
    })?;
    let mime_type = mime_guess::from_path(source).first_or_octet_stream();
    Ok(format!("data:{};base64,{}", mime_type, STANDARD.encode(data)))
}

pub async fn load_bytes_from_source(source: &str, inputs: &agentflow_core::async_node::AsyncNodeInputs) -> Result<Vec<u8>, AgentFlowError> {
    if let Some(value) = inputs.get(source) {
        return match value {
            FlowValue::Json(Value::String(s)) => {
                if let Some(data) = s.strip_prefix("data:") {
                    let parts: Vec<&str> = data.split(";base64,").collect();
                    if parts.len() == 2 {
                        return STANDARD.decode(parts[1]).map_err(|e| AgentFlowError::NodeInputError { 
                            message: format!("Invalid base64 data in input '{}': {}", source, e) 
                        });
                    }
                }
                Err(AgentFlowError::NodeInputError { message: format!("Unsupported string format for source '{}'. Expected base64 data URI.", source) })
            },
            FlowValue::File { path, .. } => {
                tokio::fs::read(path).await.map_err(|e| AgentFlowError::NodeInputError {
                    message: format!("Failed to read file at {:?}: {}", path, e),
                })
            },
            _ => Err(AgentFlowError::NodeInputError { 
                message: format!("Unsupported FlowValue type for source '{}'", source) 
            }),
        }
    }

    if source.starts_with("http") {
        let response = reqwest::get(source).await.map_err(|e| AgentFlowError::NodeInputError { 
            message: format!("Failed to download data from URL {}: {}", source, e)
        })?;
        return response.bytes().await.map(|b| b.to_vec()).map_err(|e| AgentFlowError::NodeInputError { 
            message: format!("Failed to read bytes from URL {}: {}", source, e)
        });
    }

    tokio::fs::read(source).await.map_err(|e| AgentFlowError::NodeInputError {
        message: format!("Failed to read file at {}: {}", source, e),
    })
}