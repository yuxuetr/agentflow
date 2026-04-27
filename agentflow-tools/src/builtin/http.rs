use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::{sandbox::SandboxPolicy, Tool, ToolError, ToolMetadata, ToolOutput};

/// Make HTTP GET / POST requests with domain sandbox enforcement.
pub struct HttpTool {
  client: Client,
  policy: Arc<SandboxPolicy>,
  /// Maximum response body size to return (truncate beyond this).
  max_response_chars: usize,
}

impl HttpTool {
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    let client = Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .user_agent("AgentFlow/0.1")
      .build()
      .expect("Failed to build HTTP client");

    Self {
      client,
      policy,
      max_response_chars: 8_000,
    }
  }

  pub fn default_policy() -> Self {
    Self::new(Arc::new(SandboxPolicy::default()))
  }

  fn extract_host(url: &str) -> Option<String> {
    url::Url::parse(url)
      .ok()
      .and_then(|u| u.host_str().map(String::from))
  }
}

#[async_trait]
impl Tool for HttpTool {
  fn name(&self) -> &str {
    "http"
  }

  fn description(&self) -> &str {
    "Make HTTP GET or POST requests to fetch web content or call REST APIs. \
        Returns the response body as text (truncated to 8 000 characters)."
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "url": {
                "type": "string",
                "description": "Full URL to request"
            },
            "method": {
                "type": "string",
                "enum": ["GET", "POST"],
                "description": "HTTP method (default: GET)"
            },
            "body": {
                "type": "string",
                "description": "Request body string (for POST)"
            },
            "headers": {
                "type": "object",
                "description": "Optional key-value map of additional request headers"
            }
        },
        "required": ["url"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named(self.name())
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let url = params["url"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'url'".to_string(),
      })?;

    // Domain allowlist check
    let host = Self::extract_host(url).ok_or_else(|| ToolError::InvalidParams {
      message: format!("Cannot parse host from URL: {}", url),
    })?;

    if !self.policy.is_domain_allowed(&host) {
      return Err(ToolError::SandboxViolation {
        message: format!("Domain '{}' is not in the allowed-domains list", host),
      });
    }

    let method = params["method"].as_str().unwrap_or("GET");

    let mut builder = match method {
      "GET" => self.client.get(url),
      "POST" => self.client.post(url),
      other => {
        return Err(ToolError::InvalidParams {
          message: format!("Unsupported HTTP method '{}'. Use GET or POST", other),
        })
      }
    };

    // Attach custom headers
    if let Some(headers) = params["headers"].as_object() {
      for (k, v) in headers {
        if let Some(v_str) = v.as_str() {
          builder = builder.header(k.as_str(), v_str);
        }
      }
    }

    // Attach body
    if method == "POST" {
      if let Some(body) = params["body"].as_str() {
        builder = builder.body(body.to_string());
      }
    }

    let response = builder.send().await.map_err(|e| ToolError::HttpError {
      message: e.to_string(),
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|e| ToolError::HttpError {
      message: e.to_string(),
    })?;

    // Truncate very long responses
    let content = if body.len() > self.max_response_chars {
      format!(
        "{}... [truncated — total {} chars]",
        &body[..self.max_response_chars],
        body.len()
      )
    } else {
      body
    };

    if status.is_success() {
      Ok(ToolOutput::success(content))
    } else {
      Ok(ToolOutput::error(format!("HTTP {}: {}", status, content)))
    }
  }
}
