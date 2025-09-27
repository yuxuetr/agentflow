use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct HttpNode;

#[async_trait]
impl AsyncNode for HttpNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let url = get_string_input(inputs, "url")?;
        let method = get_optional_string_input(inputs, "method")?.unwrap_or("GET");
        let headers = get_optional_map_input(inputs, "headers")?;
        let body = get_optional_string_input(inputs, "body")?;

        let client = reqwest::Client::new();
        let mut request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return Err(AgentFlowError::NodeInputError { message: format!("Unsupported HTTP method: {}", method) }),
        };

        if let Some(h) = headers {
            for (key, value) in h {
                request = request.header(key, value);
            }
        }

        if let Some(b) = body {
            request = request.body(b.to_string());
        }

        let response = request.send().await.map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;

        let status = response.status().as_u16();
        let response_body = response.text().await.map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;

        let mut outputs = HashMap::new();
        outputs.insert("status".to_string(), FlowValue::Json(json!(status)));
        outputs.insert("body".to_string(), FlowValue::Json(json!(response_body)));

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

fn get_optional_map_input(inputs: &AsyncNodeInputs, key: &str) -> Result<Option<HashMap<String, String>>, AgentFlowError> {
    match inputs.get(key) {
        None => Ok(None),
        Some(FlowValue::Json(Value::Object(map))) => {
            let mut result = HashMap::new();
            for (k, v) in map {
                if let Value::String(s) = v {
                    result.insert(k.clone(), s.clone());
                }
            }
            Ok(Some(result))
        }
        _ => Err(AgentFlowError::NodeInputError { message: format!("Input '{}' has wrong type, expected a map", key) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_get_node() {
        let node = HttpNode::default();
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("url".to_string(), FlowValue::Json(json!("https://httpbin.org/get")));

        let result = node.execute(&inputs).await;
        assert!(result.is_ok());

        let outputs = result.unwrap();
        let status = outputs.get("status").unwrap();
        if let FlowValue::Json(Value::Number(n)) = status {
            assert_eq!(n.as_u64().unwrap(), 200);
        }
    }
}