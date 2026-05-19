use agentflow_mcp::client::ClientBuilder;
use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use std::time::Duration;

/// Execute the call-tool command to invoke a tool on an MCP server
#[allow(clippy::too_many_arguments)]
pub async fn execute(
  server_command: Vec<String>,
  tool_name: String,
  tool_params: Option<String>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
  output_file: Option<String>,
  format: String,
) -> Result<()> {
  if server_command.is_empty() {
    anyhow::bail!(
      "Server command cannot be empty. Example: npx -y @modelcontextprotocol/server-filesystem /tmp"
    );
  }

  let is_json_envelope = format == "json-envelope";

  // Parse tool parameters from JSON string
  let params: Value = if let Some(params_str) = tool_params {
    serde_json::from_str(&params_str).context("Failed to parse tool parameters as JSON")?
  } else {
    serde_json::json!({})
  };

  if !is_json_envelope {
    println!(
      "{}",
      format!("🔌 Connecting to MCP server: {:?}", server_command)
        .bold()
        .blue()
    );
  }

  // Build MCP client with configuration
  let mut client_builder = ClientBuilder::new().with_stdio(server_command.clone());

  if let Some(timeout) = timeout_ms {
    client_builder = client_builder.with_timeout(Duration::from_millis(timeout));
  }

  if let Some(retries) = max_retries {
    client_builder = client_builder.with_max_retries(retries);
  }

  let mut client = client_builder
    .build()
    .await
    .context("Failed to build MCP client")?;

  // Connect and initialize
  client
    .connect()
    .await
    .context("Failed to connect to MCP server")?;

  if !is_json_envelope {
    println!("{}", "✅ Connected to MCP server".green());
  }

  let tools = client
    .list_tools()
    .await
    .context("Failed to list MCP tools before call")?;
  let tool = tools
    .iter()
    .find(|tool| tool.name == tool_name)
    .with_context(|| format!("MCP tool '{}' was not found on this server", tool_name))?;

  if !is_json_envelope {
    println!();
    println!(
      "{}",
      format!("🔧 Calling tool: {} with params: {}", tool_name, params)
        .bold()
        .cyan()
    );
  }

  let result = client
    .call_tool_validated(tool, params.clone())
    .await
    .context(format!("Failed to call tool '{}'", tool_name))?;

  // Disconnect gracefully
  client.disconnect().await.ok();

  let result_json = serde_json::to_value(&result).context("Failed to serialize tool result")?;
  let pretty_result =
    serde_json::to_string_pretty(&result_json).context("Failed to format result as JSON")?;

  if is_json_envelope {
    // P3.3 migration: wrap the tool-call result in the canonical
    // envelope. The `result` payload carries the input params + the
    // tool's response so consumers can correlate the call with its
    // output without a second round trip.
    let payload = serde_json::json!({
      "server_command": server_command,
      "tool": tool_name,
      "params": params,
      "result": result_json,
    });
    let envelope =
      crate::json_envelope::CliJsonEnvelope::ok("mcp call-tool", &payload);
    let envelope_str = serde_json::to_string_pretty(&envelope)?;
    println!("{}", envelope_str);
    if let Some(output_path) = output_file {
      // Envelope mode writes the envelope (not the bare result) to
      // disk so the file is self-describing.
      std::fs::write(&output_path, envelope_str)
        .context(format!("Failed to write result to {}", output_path))?;
    }
    return Ok(());
  }

  println!("{}", "✅ Tool call completed".green());
  println!();
  println!("{}", "Result:".bold().yellow());
  println!();
  println!("{}", pretty_result);

  // Save to file if requested
  if let Some(output_path) = output_file {
    std::fs::write(&output_path, pretty_result)
      .context(format!("Failed to write result to {}", output_path))?;

    println!();
    println!(
      "{}",
      format!("💾 Result saved to: {}", output_path)
        .bold()
        .green()
    );
  }

  Ok(())
}
