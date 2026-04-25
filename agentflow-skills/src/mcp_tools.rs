use std::sync::Arc;
use std::time::Duration;

use agentflow_mcp::client::{ClientBuilder, Content, MCPClient, Tool as McpTool};
use agentflow_tools::{Tool, ToolError, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

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
    let mut guard = self.client.lock().await;
    let client = ensure_client(&self.config, &mut guard).await?;
    client
      .list_tools()
      .await
      .map_err(|e| SkillError::McpError(format!("{}: {}", self.config.name, e)))
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
    let result =
      client
        .call_tool(tool_name, params)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
          message: format!(
            "MCP server '{}' tool '{}' failed: {}",
            self.config.name, tool_name, e
          ),
        })?;

    let content = format_mcp_result_content(&result.content);
    if result.is_error() {
      Ok(ToolOutput::error(content))
    } else {
      Ok(ToolOutput::success(content))
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
    let mut client = build_client(config).await.map_err(|e| {
      SkillError::McpError(format!(
        "Failed to build MCP client '{}': {}",
        config.name, e
      ))
    })?;
    client.connect().await.map_err(|e| {
      SkillError::McpError(format!(
        "Failed to connect MCP server '{}': {}",
        config.name, e
      ))
    })?;
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

  builder.with_timeout(Duration::from_secs(30)).build().await
}

fn format_mcp_result_content(content: &[Content]) -> String {
  if content.is_empty() {
    return String::new();
  }

  let mut parts = Vec::with_capacity(content.len());
  for item in content {
    match item {
      Content::Text { text } => parts.push(text.clone()),
      Content::Image { data, mime_type } => {
        parts.push(format!("[image:{};{} bytes]", mime_type, data.len()));
      }
      Content::Resource {
        uri,
        mime_type,
        text,
      } => {
        if let Some(text) = text {
          parts.push(text.clone());
        } else if let Some(mime_type) = mime_type {
          parts.push(format!("[resource:{};{}]", uri, mime_type));
        } else {
          parts.push(format!("[resource:{}]", uri));
        }
      }
    }
  }
  parts.join("\n")
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
}
