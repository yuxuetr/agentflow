//! Resource access interface
//!
//! This module provides the interface for listing and reading resources from MCP servers.

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult, ResultExt};
use crate::protocol::types::{JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};

use super::MCPClient;

/// Resource definition from server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
  /// Resource URI
  pub uri: String,
  /// Human-readable name
  pub name: String,
  /// Optional description
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  /// Optional MIME type
  #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
  pub mime_type: Option<String>,
}

/// Resource content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
  /// Resource URI
  pub uri: String,
  /// MIME type
  #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
  pub mime_type: Option<String>,
  /// Text content
  #[serde(skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
  /// Blob content (base64)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blob: Option<String>,
}

impl ResourceContent {
  /// Get content as text
  pub fn as_text(&self) -> Option<&str> {
    self.text.as_deref()
  }

  /// Get content as blob
  pub fn as_blob(&self) -> Option<&str> {
    self.blob.as_deref()
  }

  /// Check if resource has text content
  pub fn is_text(&self) -> bool {
    self.text.is_some()
  }

  /// Check if resource has blob content
  pub fn is_blob(&self) -> bool {
    self.blob.is_some()
  }
}

/// Read resource result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceResult {
  /// Resource contents
  pub contents: Vec<ResourceContent>,
}

impl ReadResourceResult {
  /// Get first content item
  pub fn first_content(&self) -> Option<&ResourceContent> {
    self.contents.first()
  }

  /// Get all text contents
  pub fn text_contents(&self) -> Vec<&str> {
    self
      .contents
      .iter()
      .filter_map(|c| c.as_text())
      .collect()
  }
}

/// Resource access methods for MCPClient
impl MCPClient {
  /// List available resources from the server
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
  /// let resources = client.list_resources().await?;
  ///
  /// for resource in resources {
  ///   println!("Resource: {} ({})", resource.name, resource.uri);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn list_resources(&mut self) -> MCPResult<Vec<Resource>> {
    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "resources/list", None);

    // Send request
    let response = self
      .send_request(request)
      .await
      .context("Failed to send resources/list request")?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response)
      .map_err(|e| MCPError::from(e).context("Failed to parse resources/list response"))?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!("resources/list failed: {} - {}", error.code, error.message),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response
      .result
      .ok_or_else(|| MCPError::protocol("Missing result in resources/list response", JsonRpcErrorCode::InvalidRequest))?;

    // Extract resources array
    let resources_array = result
      .get("resources")
      .and_then(|v| v.as_array())
      .ok_or_else(|| {
        MCPError::protocol("Missing or invalid 'resources' field in response", JsonRpcErrorCode::InvalidRequest)
      })?;

    // Parse resources
    let resources: Vec<Resource> = resources_array
      .iter()
      .map(|v| {
        serde_json::from_value(v.clone())
          .map_err(|e| MCPError::from(e).context("Failed to parse resource definition"))
      })
      .collect::<MCPResult<Vec<Resource>>>()?;

    Ok(resources)
  }

  /// Read a resource from the server
  ///
  /// # Arguments
  ///
  /// * `uri` - Resource URI
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Resource does not exist
  /// - Resource cannot be read
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// let result = client.read_resource("file:///path/to/file.txt").await?;
  ///
  /// if let Some(content) = result.first_content() {
  ///   if let Some(text) = content.as_text() {
  ///     println!("Content: {}", text);
  ///   }
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn read_resource(&mut self, uri: impl Into<String>) -> MCPResult<ReadResourceResult> {
    let uri = uri.into();

    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build params
    let params = serde_json::json!({
      "uri": uri
    });

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "resources/read", Some(params));

    // Send request
    let response = self
      .send_request(request)
      .await
      .context(format!("Failed to send resources/read request for '{}'", uri))?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse resources/read response for '{}'", uri))
    })?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!(
          "Resource '{}' read failed: {} - {}",
          uri, error.code, error.message
        ),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response.result.ok_or_else(|| {
      MCPError::protocol(
        format!("Missing result in resources/read response for '{}'", uri),
        JsonRpcErrorCode::InvalidRequest,
      )
    })?;

    // Parse read result
    let read_result: ReadResourceResult = serde_json::from_value(result).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse resource result for '{}'", uri))
    })?;

    Ok(read_result)
  }

  /// Subscribe to resource updates
  ///
  /// # Arguments
  ///
  /// * `uri` - Resource URI
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Server doesn't support subscriptions
  /// - Resource does not exist
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// client.subscribe_resource("file:///path/to/file.txt").await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn subscribe_resource(&mut self, uri: impl Into<String>) -> MCPResult<()> {
    let uri = uri.into();

    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build params
    let params = serde_json::json!({
      "uri": uri
    });

    // Build request
    let request = JsonRpcRequest::new(
      self.next_request_id(),
      "resources/subscribe",
      Some(params),
    );

    // Send request
    let response = self
      .send_request(request)
      .await
      .context(format!(
        "Failed to send resources/subscribe request for '{}'",
        uri
      ))?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response).map_err(|e| {
      MCPError::from(e).context(format!(
        "Failed to parse resources/subscribe response for '{}'",
        uri
      ))
    })?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!(
          "Resource '{}' subscription failed: {} - {}",
          uri, error.code, error.message
        ),
        JsonRpcErrorCode::InternalError,
      ));
    }

    Ok(())
  }

  /// Unsubscribe from resource updates
  ///
  /// # Arguments
  ///
  /// * `uri` - Resource URI
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Resource was not subscribed
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// client.unsubscribe_resource("file:///path/to/file.txt").await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn unsubscribe_resource(&mut self, uri: impl Into<String>) -> MCPResult<()> {
    let uri = uri.into();

    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build params
    let params = serde_json::json!({
      "uri": uri
    });

    // Build request
    let request = JsonRpcRequest::new(
      self.next_request_id(),
      "resources/unsubscribe",
      Some(params),
    );

    // Send request
    let response = self
      .send_request(request)
      .await
      .context(format!(
        "Failed to send resources/unsubscribe request for '{}'",
        uri
      ))?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response).map_err(|e| {
      MCPError::from(e).context(format!(
        "Failed to parse resources/unsubscribe response for '{}'",
        uri
      ))
    })?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!(
          "Resource '{}' unsubscription failed: {} - {}",
          uri, error.code, error.message
        ),
        JsonRpcErrorCode::InternalError,
      ));
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_resource_deserialization() {
    let json = serde_json::json!({
      "uri": "file:///test.txt",
      "name": "test.txt",
      "description": "A test file",
      "mimeType": "text/plain"
    });

    let resource: Resource = serde_json::from_value(json).unwrap();
    assert_eq!(resource.uri, "file:///test.txt");
    assert_eq!(resource.name, "test.txt");
    assert_eq!(resource.description, Some("A test file".to_string()));
    assert_eq!(resource.mime_type, Some("text/plain".to_string()));
  }

  #[test]
  fn test_resource_content_text() {
    let content = ResourceContent {
      uri: "file:///test.txt".to_string(),
      mime_type: Some("text/plain".to_string()),
      text: Some("Hello, world!".to_string()),
      blob: None,
    };

    assert!(content.is_text());
    assert!(!content.is_blob());
    assert_eq!(content.as_text(), Some("Hello, world!"));
  }

  #[test]
  fn test_resource_content_blob() {
    let content = ResourceContent {
      uri: "file:///image.png".to_string(),
      mime_type: Some("image/png".to_string()),
      text: None,
      blob: Some("base64data".to_string()),
    };

    assert!(!content.is_text());
    assert!(content.is_blob());
    assert_eq!(content.as_blob(), Some("base64data"));
  }

  #[test]
  fn test_read_resource_result() {
    let result = ReadResourceResult {
      contents: vec![
        ResourceContent {
          uri: "file:///test1.txt".to_string(),
          mime_type: None,
          text: Some("First".to_string()),
          blob: None,
        },
        ResourceContent {
          uri: "file:///test2.txt".to_string(),
          mime_type: None,
          text: Some("Second".to_string()),
          blob: None,
        },
      ],
    };

    assert_eq!(result.first_content().unwrap().as_text(), Some("First"));
    assert_eq!(result.text_contents().len(), 2);
    assert_eq!(result.text_contents()[0], "First");
    assert_eq!(result.text_contents()[1], "Second");
  }
}
