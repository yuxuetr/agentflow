//! Mock LLM Provider for Testing
//!
//! This provider simulates LLM responses without making actual API calls.
//! Useful for:
//! - Unit and integration testing without API keys
//! - Workflow validation and debugging
//! - Performance testing and benchmarking
//! - Development without network connectivity

use super::{ContentType, LLMProvider, ProviderRequest, ProviderResponse, TokenUsage};
use crate::client::streaming::{StreamChunk, StreamingResponse};
use crate::{LLMError, Result};
use async_trait::async_trait;

/// Mock LLM provider for testing
#[derive(Debug, Clone)]
pub struct MockProvider {
    /// Pre-configured response text
    response_text: Option<String>,
    /// Response delay in milliseconds (simulates network latency)
    delay_ms: u64,
    /// Whether to simulate an error
    simulate_error: bool,
}

impl MockProvider {
    /// Create a new mock provider with default settings
    pub fn new(_api_key: &str, _base_url: Option<String>) -> Result<Self> {
        Ok(Self {
            response_text: None,
            delay_ms: 0,
            simulate_error: false,
        })
    }

    /// Create a mock provider with custom response
    pub fn with_response(mut self, text: impl Into<String>) -> Self {
        self.response_text = Some(text.into());
        self
    }

    /// Set response delay in milliseconds
    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Configure to simulate an error
    pub fn with_error(mut self) -> Self {
        self.simulate_error = true;
        self
    }

    /// Generate a default response based on the request
    fn generate_default_response(&self, request: &ProviderRequest) -> String {
        let first_message = request
            .messages
            .first()
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");

        format!(
            "Mock response for: '{}'... (model: {})",
            &first_message.chars().take(50).collect::<String>(),
            request.model
        )
    }
}

/// Mock streaming response
pub struct MockStreamingResponse {
    content: String,
    sent: bool,
}

impl MockStreamingResponse {
    fn new(content: String) -> Self {
        Self {
            content,
            sent: false,
        }
    }
}

#[async_trait]
impl StreamingResponse for MockStreamingResponse {
    async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
        if self.sent {
            Ok(None)
        } else {
            self.sent = true;
            Ok(Some(StreamChunk {
                content: self.content.clone(),
                is_final: true,
                metadata: None,
                usage: None,
                content_type: Some("text".to_string()),
            }))
        }
    }
}

#[async_trait]
impl LLMProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    async fn execute(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate error if configured
        if self.simulate_error {
            return Err(LLMError::ModelExecutionError {
                message: "Mock provider simulated error".to_string(),
            });
        }

        // Generate response
        let content_text = self
            .response_text
            .clone()
            .unwrap_or_else(|| self.generate_default_response(request));

        let word_count = content_text.split_whitespace().count() as u32;

        Ok(ProviderResponse {
            content: ContentType::Text(content_text),
            usage: Some(TokenUsage {
                prompt_tokens: Some(50),
                completion_tokens: Some(word_count),
                total_tokens: Some(50 + word_count),
            }),
            metadata: Some(serde_json::json!({
                "model": request.model,
                "finish_reason": "stop"
            })),
        })
    }

    async fn execute_streaming(
        &self,
        request: &ProviderRequest,
    ) -> Result<Box<dyn StreamingResponse>> {
        // Simulate network delay
        if self.delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Simulate error if configured
        if self.simulate_error {
            return Err(LLMError::ModelExecutionError {
                message: "Mock provider simulated error".to_string(),
            });
        }

        let content = self
            .response_text
            .clone()
            .unwrap_or_else(|| self.generate_default_response(request));

        Ok(Box::new(MockStreamingResponse::new(content)))
    }

    async fn validate_config(&self) -> Result<()> {
        if self.simulate_error {
            Err(LLMError::ConfigurationError {
                message: "Mock provider configured to simulate error".to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn base_url(&self) -> &str {
        "mock://localhost"
    }

    fn supported_models(&self) -> Vec<String> {
        vec![
            "mock-model".to_string(),
            "mock-fast".to_string(),
            "mock-slow".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_mock_provider_default_response() {
        let provider = MockProvider::new("", None).unwrap();
        let request = ProviderRequest {
            model: "mock-model".to_string(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "Hello, world!"
            })],
            stream: false,
            parameters: HashMap::new(),
        };

        let response = provider.execute(&request).await.unwrap();
        assert!(response.content.to_string().contains("Mock response"));
    }

    #[tokio::test]
    async fn test_mock_provider_custom_response() {
        let provider = MockProvider::new("", None)
            .unwrap()
            .with_response("Custom test response");

        let request = ProviderRequest {
            model: "mock-model".to_string(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "Test prompt"
            })],
            stream: false,
            parameters: HashMap::new(),
        };

        let response = provider.execute(&request).await.unwrap();
        assert_eq!(response.content.to_string(), "Custom test response");
    }

    #[tokio::test]
    async fn test_mock_provider_error_simulation() {
        let provider = MockProvider::new("", None).unwrap().with_error();

        let request = ProviderRequest {
            model: "mock-model".to_string(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "Test prompt"
            })],
            stream: false,
            parameters: HashMap::new(),
        };

        let result = provider.execute(&request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_with_delay() {
        let provider = MockProvider::new("", None).unwrap().with_delay(50);

        let request = ProviderRequest {
            model: "mock-model".to_string(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "Test prompt"
            })],
            stream: false,
            parameters: HashMap::new(),
        };

        let start = std::time::Instant::now();
        let _response = provider.execute(&request).await.unwrap();
        let duration = start.elapsed();

        assert!(duration.as_millis() >= 50);
    }

    #[tokio::test]
    async fn test_mock_provider_streaming() {
        let provider = MockProvider::new("", None)
            .unwrap()
            .with_response("Streaming test");

        let request = ProviderRequest {
            model: "mock-model".to_string(),
            messages: vec![serde_json::json!({
                "role": "user",
                "content": "Test prompt"
            })],
            stream: true,
            parameters: HashMap::new(),
        };

        let _stream = provider.execute_streaming(&request).await.unwrap();
        // Note: Testing actual stream consumption would require more complex setup
    }
}
