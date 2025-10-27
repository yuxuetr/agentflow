//! Prompt template interface
//!
//! This module provides the interface for listing and retrieving prompt templates from MCP servers.

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult, ResultExt};
use crate::protocol::types::{JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::MCPClient;

/// Prompt argument definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
  /// Argument name
  pub name: String,
  /// Human-readable description
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  /// Whether argument is required
  #[serde(skip_serializing_if = "Option::is_none")]
  pub required: Option<bool>,
}

impl PromptArgument {
  /// Check if argument is required
  pub fn is_required(&self) -> bool {
    self.required.unwrap_or(false)
  }
}

/// Prompt definition from server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
  /// Prompt name
  pub name: String,
  /// Human-readable description
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  /// Prompt arguments
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub arguments: Vec<PromptArgument>,
}

/// Prompt message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptMessageRole {
  /// User message
  User,
  /// Assistant message
  Assistant,
}

/// Prompt message content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PromptMessageContent {
  /// Text content
  Text {
    /// The text content
    text: String,
  },
  /// Image content
  Image {
    /// Image data (base64 or URL)
    data: String,
    /// MIME type
    #[serde(rename = "mimeType")]
    mime_type: String,
  },
  /// Resource reference
  Resource {
    /// Resource URI
    uri: String,
    /// Optional MIME type
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    mime_type: Option<String>,
    /// Optional text content
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
  },
}

/// Prompt message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
  /// Message role
  pub role: PromptMessageRole,
  /// Message content
  pub content: PromptMessageContent,
}

impl PromptMessage {
  /// Create a user message with text
  pub fn user_text(text: impl Into<String>) -> Self {
    Self {
      role: PromptMessageRole::User,
      content: PromptMessageContent::Text {
        text: text.into(),
      },
    }
  }

  /// Create an assistant message with text
  pub fn assistant_text(text: impl Into<String>) -> Self {
    Self {
      role: PromptMessageRole::Assistant,
      content: PromptMessageContent::Text {
        text: text.into(),
      },
    }
  }

  /// Get text content if available
  pub fn as_text(&self) -> Option<&str> {
    match &self.content {
      PromptMessageContent::Text { text } => Some(text),
      _ => None,
    }
  }
}

/// Get prompt result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResult {
  /// Optional description
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  /// Prompt messages
  pub messages: Vec<PromptMessage>,
}

impl GetPromptResult {
  /// Get all text messages
  pub fn text_messages(&self) -> Vec<&str> {
    self
      .messages
      .iter()
      .filter_map(|m| m.as_text())
      .collect()
  }

  /// Get first text message
  pub fn first_text(&self) -> Option<&str> {
    self.text_messages().first().copied()
  }
}

/// Prompt access methods for MCPClient
impl MCPClient {
  /// List available prompts from the server
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Request fails
  /// - Server returns an error
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// let mut client = ClientBuilder::new()
  ///   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  ///   .build()
  ///   .await?;
  ///
  /// client.connect().await?;
  /// let prompts = client.list_prompts().await?;
  ///
  /// for prompt in prompts {
  ///   println!("Prompt: {} - {:?}", prompt.name, prompt.description);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn list_prompts(&mut self) -> MCPResult<Vec<Prompt>> {
    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "prompts/list", None);

    // Send request
    let response = self
      .send_request(request)
      .await
      .context("Failed to send prompts/list request")?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response)
      .map_err(|e| MCPError::from(e).context("Failed to parse prompts/list response"))?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!("prompts/list failed: {} - {}", error.code, error.message),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response.result.ok_or_else(|| {
      MCPError::protocol(
        "Missing result in prompts/list response",
        JsonRpcErrorCode::InvalidRequest,
      )
    })?;

    // Extract prompts array
    let prompts_array = result
      .get("prompts")
      .and_then(|v| v.as_array())
      .ok_or_else(|| {
        MCPError::protocol(
          "Missing or invalid 'prompts' field in response",
          JsonRpcErrorCode::InvalidRequest,
        )
      })?;

    // Parse prompts
    let prompts: Vec<Prompt> = prompts_array
      .iter()
      .map(|v| {
        serde_json::from_value(v.clone())
          .map_err(|e| MCPError::from(e).context("Failed to parse prompt definition"))
      })
      .collect::<MCPResult<Vec<Prompt>>>()?;

    Ok(prompts)
  }

  /// Get a prompt with arguments
  ///
  /// # Arguments
  ///
  /// * `name` - Prompt name
  /// * `arguments` - Prompt arguments (if any)
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Prompt does not exist
  /// - Arguments are invalid
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # use std::collections::HashMap;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// let mut args = HashMap::new();
  /// args.insert("topic".to_string(), "Rust programming".to_string());
  ///
  /// let result = client.get_prompt("code_review", args).await?;
  ///
  /// for message in result.messages {
  ///   if let Some(text) = message.as_text() {
  ///     println!("{:?}: {}", message.role, text);
  ///   }
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn get_prompt(
    &mut self,
    name: impl Into<String>,
    arguments: HashMap<String, String>,
  ) -> MCPResult<GetPromptResult> {
    let name = name.into();

    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build params
    let params = serde_json::json!({
      "name": name,
      "arguments": arguments
    });

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "prompts/get", Some(params));

    // Send request
    let response = self
      .send_request(request)
      .await
      .context(format!("Failed to send prompts/get request for '{}'", name))?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse prompts/get response for '{}'", name))
    })?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!(
          "Prompt '{}' retrieval failed: {} - {}",
          name, error.code, error.message
        ),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response.result.ok_or_else(|| {
      MCPError::protocol(
        format!("Missing result in prompts/get response for '{}'", name),
        JsonRpcErrorCode::InvalidRequest,
      )
    })?;

    // Parse prompt result
    let prompt_result: GetPromptResult = serde_json::from_value(result).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse prompt result for '{}'", name))
    })?;

    Ok(prompt_result)
  }

  /// Get a prompt with validated arguments
  ///
  /// This validates required arguments before calling.
  ///
  /// # Arguments
  ///
  /// * `prompt` - Prompt definition (from list_prompts)
  /// * `arguments` - Prompt arguments
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Required arguments are missing
  /// - Prompt retrieval fails
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # use std::collections::HashMap;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// let prompts = client.list_prompts().await?;
  /// let prompt = prompts.iter().find(|p| p.name == "code_review").unwrap();
  ///
  /// let mut args = HashMap::new();
  /// args.insert("topic".to_string(), "Rust".to_string());
  ///
  /// let result = client.get_prompt_validated(prompt, args).await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn get_prompt_validated(
    &mut self,
    prompt: &Prompt,
    arguments: HashMap<String, String>,
  ) -> MCPResult<GetPromptResult> {
    // Validate required arguments
    self
      .validate_prompt_arguments(prompt, &arguments)
      .context(format!("Validation failed for prompt '{}'", prompt.name))?;

    // Get prompt
    self.get_prompt(&prompt.name, arguments).await
  }

  /// Validate prompt arguments
  fn validate_prompt_arguments(
    &self,
    prompt: &Prompt,
    arguments: &HashMap<String, String>,
  ) -> MCPResult<()> {
    // Check required arguments
    for arg in &prompt.arguments {
      if arg.is_required() && !arguments.contains_key(&arg.name) {
        return Err(MCPError::validation(
          format!(
            "Prompt '{}' requires argument '{}' but it was not provided",
            prompt.name, arg.name
          ),
          Some(arg.name.clone()),
        ));
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_prompt_deserialization() {
    let json = serde_json::json!({
      "name": "test_prompt",
      "description": "A test prompt",
      "arguments": [
        {
          "name": "topic",
          "description": "The topic",
          "required": true
        }
      ]
    });

    let prompt: Prompt = serde_json::from_value(json).unwrap();
    assert_eq!(prompt.name, "test_prompt");
    assert_eq!(prompt.description, Some("A test prompt".to_string()));
    assert_eq!(prompt.arguments.len(), 1);
    assert_eq!(prompt.arguments[0].name, "topic");
    assert!(prompt.arguments[0].is_required());
  }

  #[test]
  fn test_prompt_message_user_text() {
    let msg = PromptMessage::user_text("Hello");
    assert_eq!(msg.role, PromptMessageRole::User);
    assert_eq!(msg.as_text(), Some("Hello"));
  }

  #[test]
  fn test_prompt_message_assistant_text() {
    let msg = PromptMessage::assistant_text("Hi there");
    assert_eq!(msg.role, PromptMessageRole::Assistant);
    assert_eq!(msg.as_text(), Some("Hi there"));
  }

  #[test]
  fn test_get_prompt_result_text_messages() {
    let result = GetPromptResult {
      description: None,
      messages: vec![
        PromptMessage::user_text("Question"),
        PromptMessage::assistant_text("Answer"),
      ],
    };

    let texts = result.text_messages();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0], "Question");
    assert_eq!(texts[1], "Answer");
  }

  #[test]
  fn test_get_prompt_result_first_text() {
    let result = GetPromptResult {
      description: None,
      messages: vec![PromptMessage::user_text("Hello")],
    };

    assert_eq!(result.first_text(), Some("Hello"));
  }
}
