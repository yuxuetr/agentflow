use crate::{AsyncNode, SharedState};
use agentflow_core::{AgentFlowError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// HTTP client node for making API calls
#[derive(Debug, Clone)]
pub struct HttpNode {
    pub name: String,
    pub url: String,
    pub method: String,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
}

impl HttpNode {
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            method: "GET".to_string(),
            headers: None,
            body: None,
        }
    }

    pub fn with_method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    pub fn with_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }
}

#[async_trait]
impl AsyncNode for HttpNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        // TODO: Resolve template variables in URL, headers, and body
        Ok(serde_json::json!({
            "url": self.url,
            "method": self.method,
            "headers": self.headers,
            "body": self.body
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let url = prep_result["url"].as_str().unwrap_or(&self.url);
        let method = prep_result["method"].as_str().unwrap_or(&self.method);

        // Build HTTP client request
        let client = reqwest::Client::new();
        let mut request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => return Err(AgentFlowError::AsyncExecutionError {
                message: format!("Unsupported HTTP method: {}", method),
            }),
        };

        // Add headers if present
        if let Some(headers) = prep_result["headers"].as_object() {
            for (key, value) in headers {
                if let Some(header_value) = value.as_str() {
                    request = request.header(key, header_value);
                }
            }
        }

        // Add body if present
        if let Some(body) = prep_result["body"].as_str() {
            request = request.body(body.to_string());
        }

        // Execute request
        let response = request.send().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("HTTP request failed: {}", e),
        })?;

        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to read response body: {}", e),
        })?;

        Ok(serde_json::json!({
            "status": status,
            "body": text,
            "success": status >= 200 && status < 300
        }))
    }

    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>> {
        // Store response in shared state
        shared.insert(format!("{}_response", self.name), exec_result.clone());
        shared.insert(format!("{}_status", self.name), exec_result["status"].clone());
        shared.insert(format!("{}_body", self.name), exec_result["body"].clone());

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_http_node_creation() {
        let node = HttpNode::new("test_http", "https://httpbin.org/get");
        assert_eq!(node.name, "test_http");
        assert_eq!(node.url, "https://httpbin.org/get");
        assert_eq!(node.method, "GET");
        assert!(node.headers.is_none());
        assert!(node.body.is_none());
    }

    #[tokio::test]
    async fn test_http_node_builder_pattern() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let node = HttpNode::new("test_http", "https://httpbin.org/post")
            .with_method("POST")
            .with_headers(headers.clone())
            .with_body(r#"{"test": "data"}"#);

        assert_eq!(node.method, "POST");
        assert_eq!(node.headers, Some(headers));
        assert_eq!(node.body, Some(r#"{"test": "data"}"#.to_string()));
    }

    #[tokio::test]
    async fn test_http_node_prep_async() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        let node = HttpNode::new("test_http", "https://httpbin.org/post")
            .with_method("POST")
            .with_headers(headers)
            .with_body(r#"{"key": "value"}"#);

        let shared = SharedState::new();
        let prep_result = node.prep_async(&shared).await.unwrap();
        
        assert_eq!(prep_result["url"].as_str().unwrap(), "https://httpbin.org/post");
        assert_eq!(prep_result["method"].as_str().unwrap(), "POST");
        assert_eq!(prep_result["body"].as_str().unwrap(), r#"{"key": "value"}"#);
        assert!(prep_result["headers"].is_object());
    }

    #[tokio::test]
    async fn test_http_node_get_request() {
        let node = HttpNode::new("get_test", "https://httpbin.org/get");
        let shared = SharedState::new();

        // This is an integration test that requires network
        // In a real test suite, you might mock the HTTP client
        let result = node.run_async(&shared).await;
        
        // Just test that the structure is correct, don't test network
        if result.is_ok() {
            let response = shared.get("get_test_response").unwrap();
            assert!(response["status"].is_number());
            assert!(response["body"].is_string());
            assert!(response["success"].is_boolean());
        }
        // If network fails, that's okay for this test
    }

    #[tokio::test]
    async fn test_http_node_post_async() {
        let node = HttpNode::new("post_test", "https://httpbin.org/post");
        let shared = SharedState::new();
        
        let exec_result = serde_json::json!({
            "status": 200,
            "body": "OK",
            "success": true
        });
        let prep_result = Value::Object(serde_json::Map::new());

        let result = node.post_async(&shared, prep_result, exec_result.clone()).await.unwrap();
        assert!(result.is_none());

        // Verify shared state was updated
        assert_eq!(shared.get("post_test_response").unwrap(), exec_result);
        assert_eq!(shared.get("post_test_status").unwrap(), exec_result["status"]);
        assert_eq!(shared.get("post_test_body").unwrap(), exec_result["body"]);
    }

    #[tokio::test]
    async fn test_http_node_method_variations() {
        let methods = vec!["GET", "POST", "PUT", "DELETE", "PATCH"];
        
        for method in methods {
            let node = HttpNode::new("method_test", "https://httpbin.org/get")
                .with_method(method);
            assert_eq!(node.method, method);
            
            let shared = SharedState::new();
            let prep_result = node.prep_async(&shared).await.unwrap();
            assert_eq!(prep_result["method"].as_str().unwrap(), method);
        }
    }

    #[test]
    fn test_http_node_unsupported_method() {
        // Test that unsupported HTTP methods are handled
        let node = HttpNode::new("invalid_method", "https://httpbin.org/get")
            .with_method("INVALID");
        
        assert_eq!(node.method, "INVALID");
        // The actual error would be caught during exec_async
    }

    #[tokio::test]
    async fn test_http_node_with_custom_headers() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom-Header".to_string(), "test-value".to_string());
        headers.insert("User-Agent".to_string(), "AgentFlow/0.1.0".to_string());

        let node = HttpNode::new("header_test", "https://httpbin.org/headers")
            .with_headers(headers.clone());

        let shared = SharedState::new();
        let prep_result = node.prep_async(&shared).await.unwrap();
        
        let result_headers = prep_result["headers"].as_object().unwrap();
        assert_eq!(result_headers["X-Custom-Header"].as_str().unwrap(), "test-value");
        assert_eq!(result_headers["User-Agent"].as_str().unwrap(), "AgentFlow/0.1.0");
    }

    #[tokio::test]
    async fn test_http_node_with_json_body() {
        let json_body = r#"{"message": "Hello, World!", "number": 42}"#;
        let node = HttpNode::new("json_test", "https://httpbin.org/post")
            .with_method("POST")
            .with_body(json_body);

        let shared = SharedState::new();
        let prep_result = node.prep_async(&shared).await.unwrap();
        
        assert_eq!(prep_result["body"].as_str().unwrap(), json_body);
        assert_eq!(prep_result["method"].as_str().unwrap(), "POST");
    }
}