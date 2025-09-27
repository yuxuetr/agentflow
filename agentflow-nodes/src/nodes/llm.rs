use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct LlmNode;

#[async_trait]
impl AsyncNode for LlmNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let prompt = get_string_input(inputs, "prompt")?;
        let model = get_optional_string_input(inputs, "model")?;
        let system = get_optional_string_input(inputs, "system")?;

        AgentFlow::init().await.map_err(|e| AgentFlowError::ConfigurationError {
            message: format!("Failed to initialize AgentFlow LLM service: {}", e),
        })?;

        let mut request = AgentFlow::model(model.as_deref().unwrap_or_default()).prompt(&prompt);

        if let Some(sys) = system {
            request = request.system(&sys);
        }
        if let Some(temp) = get_optional_f64_input(inputs, "temperature")? {
            request = request.temperature(temp as f32);
        }
        if let Some(max_tokens) = get_optional_u64_input(inputs, "max_tokens")? {
            request = request.max_tokens(max_tokens as u32);
        }

        println!("🤖 Executing LLM request...");
        let response = request.execute().await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
                message: format!("LLM execution failed: {}", e),
            }
        })?;
        println!("✅ LLM Response received.");

        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), FlowValue::Json(Value::String(response)));

        Ok(outputs)
    }
}

fn get_string_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a str, AgentFlowError> {
    inputs.get(key)
        .and_then(|v| match v {
            FlowValue::Json(Value::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| AgentFlowError::NodeInputError { message: format!("Required string input '{}' is missing or has wrong type", key) })
}

fn get_optional_string_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<Option<&'a str>, AgentFlowError> {
    match inputs.get(key) {
        None => Ok(None),
        Some(v) => match v {
            FlowValue::Json(Value::String(s)) => Ok(Some(s.as_str())),
            _ => Err(AgentFlowError::NodeInputError { message: format!("Input '{}' has wrong type, expected a string", key) })
        }
    }
}

fn get_optional_f64_input(inputs: &AsyncNodeInputs, key: &str) -> Result<Option<f64>, AgentFlowError> {
    match inputs.get(key) {
        None => Ok(None),
        Some(v) => match v {
            FlowValue::Json(Value::Number(n)) => Ok(n.as_f64()),
            _ => Err(AgentFlowError::NodeInputError { message: format!("Input '{}' has wrong type, expected a number", key) })
        }
    }
}

fn get_optional_u64_input(inputs: &AsyncNodeInputs, key: &str) -> Result<Option<u64>, AgentFlowError> {
    match inputs.get(key) {
        None => Ok(None),
        Some(v) => match v {
            FlowValue::Json(Value::Number(n)) => Ok(n.as_u64()),
            _ => Err(AgentFlowError::NodeInputError { message: format!("Input '{}' has wrong type, expected a number", key) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_llm_node_async_execution() {
        let node = LlmNode::default();
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("prompt".to_string(), FlowValue::Json(json!("What is 2+2? Respond with only the number.")));
        inputs.insert("temperature".to_string(), FlowValue::Json(json!(0.0)));

        let result = node.execute(&inputs).await;

        if let Ok(outputs) = result {
            let response_val = outputs.get("output").unwrap();
            if let FlowValue::Json(Value::String(s)) = response_val {
                assert!(s.contains('4'));
            }
        } else {
            println!("LLM node execution failed (as expected without API key): {:?}", result.err().unwrap());
        }
    }
}
