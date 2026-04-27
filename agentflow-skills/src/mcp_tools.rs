use agentflow_mcp::client::{ClientBuilder, Content, MCPClient, Tool as McpTool};
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolOutputPart};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::{error::SkillError, manifest::McpServerConfig};

/// Shared MCP client handle for all tools exposed by one configured server.
///
/// The handle lazily reconnects when needed and serializes access through a
/// mutex because the MCP client API requires mutable access for requests.
#[derive(Debug)]
pub struct McpClientPool {
  config: McpServerConfig,
  client: Mutex<Option<MCPClient>>,
}

impl McpClientPool {
  pub fn new(config: McpServerConfig) -> Self {
    Self {
      config,
      client: Mutex::new(None),
    }
  }

  pub fn server_name(&self) -> &str {
    &self.config.name
  }

  pub async fn list_tools(&self) -> Result<Vec<McpTool>, SkillError> {
    info!(
      event = "mcp_tools_list_started",
      server = %self.config.name,
      "Listing MCP tools"
    );
    let mut guard = self.client.lock().await;
    let client = ensure_client(&self.config, &mut guard).await?;
    let tools = client.list_tools().await.map_err(|e| {
      warn!(
        event = "mcp_tools_list_failed",
        server = %self.config.name,
        error = %e,
        "Failed to list MCP tools"
      );
      SkillError::McpError(format!("{}: {}", self.config.name, e))
    })?;
    info!(
      event = "mcp_tools_list_succeeded",
      server = %self.config.name,
      tool_count = tools.len(),
      "Listed MCP tools"
    );
    Ok(tools)
  }

  pub async fn disconnect(&self) -> Result<(), SkillError> {
    let mut guard = self.client.lock().await;
    if let Some(client) = guard.as_mut() {
      client
        .disconnect()
        .await
        .map_err(|e| SkillError::McpError(format!("{}: {}", self.config.name, e)))?;
    }
    *guard = None;
    Ok(())
  }

  async fn call_tool(&self, tool_name: &str, params: Value) -> Result<ToolOutput, ToolError> {
    let mut guard = self.client.lock().await;
    let client = ensure_client_for_tool(&self.config, &mut guard).await?;
    let timeout = self.config.resolved_timeout();
    info!(
      event = "mcp_tool_call_started",
      server = %self.config.name,
      tool = %tool_name,
      timeout_ms = timeout.as_millis() as u64,
      "Calling MCP tool"
    );
    let result = match tokio::time::timeout(timeout, client.call_tool(tool_name, params)).await {
      Ok(result) => result.map_err(|e| {
        let message = format!(
          "MCP server '{}' tool '{}' failed: {}",
          self.config.name, tool_name, e
        );
        warn!(
          event = "mcp_tool_call_failed",
          server = %self.config.name,
          tool = %tool_name,
          error = %e,
          "MCP tool call failed"
        );
        ToolError::ExecutionFailed { message }
      })?,
      Err(_) => {
        if let Some(client) = guard.as_mut() {
          let _ = client.disconnect().await;
        }
        *guard = None;
        warn!(
          event = "mcp_tool_call_timeout",
          server = %self.config.name,
          tool = %tool_name,
          timeout_ms = timeout.as_millis() as u64,
          "MCP tool call timed out"
        );
        return Err(ToolError::ExecutionFailed {
          message: format!(
            "MCP server '{}' tool '{}' timed out after {:?}",
            self.config.name, tool_name, timeout
          ),
        });
      }
    };

    let (content, parts) = convert_mcp_result_content(&result.content);
    if result.is_error() {
      let content = format!(
        "MCP server '{}' tool '{}' returned error: {}",
        self.config.name, tool_name, content
      );
      warn!(
        event = "mcp_tool_call_result_error",
        server = %self.config.name,
        tool = %tool_name,
        "MCP tool returned an error result"
      );
      Ok(ToolOutput::error_parts(content, parts))
    } else {
      info!(
        event = "mcp_tool_call_succeeded",
        server = %self.config.name,
        tool = %tool_name,
        "MCP tool call succeeded"
      );
      Ok(ToolOutput::success_parts(content, parts))
    }
  }
}

/// Tool adapter registered in AgentFlow's local ToolRegistry.
pub struct McpToolAdapter {
  public_name: String,
  remote_name: String,
  description: String,
  input_schema: Value,
  pool: Arc<McpClientPool>,
}

impl McpToolAdapter {
  pub fn new(pool: Arc<McpClientPool>, tool: McpTool) -> Self {
    let public_name = public_tool_name(pool.server_name(), &tool.name);
    let description = tool.description.unwrap_or_else(|| {
      format!(
        "MCP tool '{}' exposed by server '{}'",
        tool.name,
        pool.server_name()
      )
    });

    Self {
      public_name,
      remote_name: tool.name,
      description,
      input_schema: tool.input_schema,
      pool,
    }
  }
}

#[async_trait]
impl Tool for McpToolAdapter {
  fn name(&self) -> &str {
    &self.public_name
  }

  fn description(&self) -> &str {
    &self.description
  }

  fn parameters_schema(&self) -> Value {
    self.input_schema.clone()
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::mcp(self.pool.server_name(), &self.remote_name)
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    self.pool.call_tool(&self.remote_name, params).await
  }
}

pub fn public_tool_name(server_name: &str, tool_name: &str) -> String {
  format!(
    "mcp_{}_{}",
    sanitize_tool_name(server_name),
    sanitize_tool_name(tool_name)
  )
}

fn sanitize_tool_name(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for ch in value.chars() {
    if ch.is_ascii_alphanumeric() || ch == '_' {
      out.push(ch.to_ascii_lowercase());
    } else {
      out.push('_');
    }
  }
  let trimmed = out.trim_matches('_').to_string();
  if trimmed.is_empty() {
    "tool".to_string()
  } else {
    trimmed
  }
}

async fn ensure_client<'a>(
  config: &McpServerConfig,
  slot: &'a mut Option<MCPClient>,
) -> Result<&'a mut MCPClient, SkillError> {
  if slot.is_none() {
    info!(
      event = "mcp_server_connect_started",
      server = %config.name,
      command = %config.command,
      timeout_ms = config.resolved_timeout().as_millis() as u64,
      "Connecting MCP server"
    );
    let mut client = build_client(config).await.map_err(|e| {
      warn!(
        event = "mcp_server_client_build_failed",
        server = %config.name,
        error = %e,
        "Failed to build MCP client"
      );
      SkillError::McpError(format!(
        "Failed to build MCP client '{}': {}",
        config.name, e
      ))
    })?;
    client.connect().await.map_err(|e| {
      warn!(
        event = "mcp_server_connect_failed",
        server = %config.name,
        error = %e,
        "Failed to connect MCP server"
      );
      SkillError::McpError(format!(
        "Failed to connect MCP server '{}': {}",
        config.name, e
      ))
    })?;
    info!(
      event = "mcp_server_connected",
      server = %config.name,
      "Connected MCP server"
    );
    *slot = Some(client);
  }

  slot.as_mut().ok_or_else(|| {
    SkillError::McpError(format!(
      "MCP client '{}' was not available after initialization",
      config.name
    ))
  })
}

async fn ensure_client_for_tool<'a>(
  config: &McpServerConfig,
  slot: &'a mut Option<MCPClient>,
) -> Result<&'a mut MCPClient, ToolError> {
  ensure_client(config, slot)
    .await
    .map_err(|e| ToolError::ExecutionFailed {
      message: e.to_string(),
    })
}

async fn build_client(config: &McpServerConfig) -> agentflow_mcp::MCPResult<MCPClient> {
  let mut command = Vec::with_capacity(1 + config.args.len());
  command.push(config.command.clone());
  command.extend(config.args.clone());

  let builder = if config.env.is_empty() {
    ClientBuilder::new().with_stdio(command)
  } else {
    ClientBuilder::new().with_stdio_env(command, config.env.clone())
  };

  builder
    .with_timeout(config.resolved_timeout())
    .build()
    .await
}

fn convert_mcp_result_content(content: &[Content]) -> (String, Vec<ToolOutputPart>) {
  if content.is_empty() {
    return (String::new(), Vec::new());
  }

  let mut text_parts = Vec::with_capacity(content.len());
  let mut output_parts = Vec::with_capacity(content.len());
  for item in content {
    match item {
      Content::Text { text } => {
        text_parts.push(text.clone());
        output_parts.push(ToolOutputPart::Text { text: text.clone() });
      }
      Content::Image { data, mime_type } => {
        text_parts.push(format!("[image:{};{} bytes]", mime_type, data.len()));
        output_parts.push(ToolOutputPart::Image {
          data: data.clone(),
          mime_type: mime_type.clone(),
        });
      }
      Content::Resource {
        uri,
        mime_type,
        text,
      } => {
        if let Some(text) = text {
          text_parts.push(text.clone());
        } else if let Some(mime_type) = mime_type {
          text_parts.push(format!("[resource:{};{}]", uri, mime_type));
        } else {
          text_parts.push(format!("[resource:{}]", uri));
        }
        output_parts.push(ToolOutputPart::Resource {
          uri: uri.clone(),
          mime_type: mime_type.clone(),
          text: text.clone(),
        });
      }
    }
  }
  (text_parts.join("\n"), output_parts)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn public_tool_names_are_stable_and_prefixed() {
    assert_eq!(
      public_tool_name("github-server", "search/repositories"),
      "mcp_github_server_search_repositories"
    );
  }

  #[test]
  fn empty_tool_name_segments_fall_back() {
    assert_eq!(public_tool_name("!!!", "???"), "mcp_tool_tool");
  }

  #[test]
  fn mcp_adapter_metadata_preserves_original_server_and_tool_names() {
    let pool = Arc::new(McpClientPool::new(McpServerConfig {
      name: "local-demo".to_string(),
      command: "python3".to_string(),
      args: vec![],
      env: Default::default(),
      timeout_secs: None,
    }));
    let adapter = McpToolAdapter::new(
      pool,
      McpTool {
        name: "echo/raw".to_string(),
        description: Some("Echo".to_string()),
        input_schema: serde_json::json!({"type": "object"}),
      },
    );

    let definition = adapter.definition();
    assert_eq!(definition.name, "mcp_local_demo_echo_raw");
    assert_eq!(definition.metadata.source, agentflow_tools::ToolSource::Mcp);
    assert_eq!(
      definition.metadata.mcp_server_name.as_deref(),
      Some("local-demo")
    );
    assert_eq!(
      definition.metadata.mcp_tool_name.as_deref(),
      Some("echo/raw")
    );
  }
}
