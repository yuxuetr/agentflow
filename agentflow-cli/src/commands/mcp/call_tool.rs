use agentflow_mcp::client::ClientBuilder;
use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use std::time::Duration;

/// Execute the call-tool command to invoke a tool on an MCP server
pub async fn execute(
  server_command: Vec<String>,
  tool_name: String,
  tool_params: Option<String>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
  output_file: Option<String>,
) -> Result<()> {
  if server_command.is_empty() {
    anyhow::bail!("Server command cannot be empty. Example: npx -y @modelcontextprotocol/server-filesystem /tmp");
  }

  // Parse tool parameters from JSON string
  let params: Value = if let Some(params_str) = tool_params {
    serde_json::from_str(&params_str)
      .context("Failed to parse tool parameters as JSON")?
  } else {
    serde_json::json!({})
  };

  println!(
    "{}",
    format!("🔌 Connecting to MCP server: {:?}", server_command)
      .bold()
      .blue()
  );

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

  println!("{}", "✅ Connected to MCP server".green());

  // Call the tool
  println!();
  println!(
    "{}",
    format!("🔧 Calling tool: {} with params: {}", tool_name, params)
      .bold()
      .cyan()
  );

  let result = client
    .call_tool(&tool_name, params)
    .await
    .context(format!("Failed to call tool '{}'", tool_name))?;

  // Disconnect gracefully
  client.disconnect().await.ok();

  println!("{}", "✅ Tool call completed".green());
  println!();

  // Display results
  println!("{}", "Result:".bold().yellow());
  println!();

  let result_json = serde_json::to_value(&result)
    .context("Failed to serialize tool result")?;

  // Pretty print the result
  let pretty_result = serde_json::to_string_pretty(&result_json)
    .context("Failed to format result as JSON")?;

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
