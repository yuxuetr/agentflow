use agentflow_mcp::client::ClientBuilder;
use anyhow::{Context, Result};
use colored::*;
use std::time::Duration;

/// Execute the list-tools command to discover available tools from an MCP server
pub async fn execute(
  server_command: Vec<String>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
) -> Result<()> {
  if server_command.is_empty() {
    anyhow::bail!("Server command cannot be empty. Example: npx -y @modelcontextprotocol/server-filesystem /tmp");
  }

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

  // List available tools
  let tools = client
    .list_tools()
    .await
    .context("Failed to list tools from MCP server")?;

  // Disconnect gracefully
  client.disconnect().await.ok();

  // Display results
  if tools.is_empty() {
    println!("{}", "⚠️  No tools found".yellow());
    return Ok(());
  }

  println!();
  println!(
    "{}",
    format!("Available Tools ({}):", tools.len()).bold().green()
  );
  println!();

  for tool in &tools {
    println!("  {}", format!("• {}", tool.name).bold());

    if let Some(description) = &tool.description {
      println!("    {}", description.dimmed());
    }

    // Display input schema if available
    if let Some(properties) = tool
      .input_schema
      .get("properties")
      .and_then(|p| p.as_object())
    {
      if !properties.is_empty() {
        println!("    {}:", "Parameters:".italic());
        for (param_name, param_schema) in properties {
          let param_type = param_schema
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

          let param_desc = param_schema
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");

          println!(
            "      - {} ({}): {}",
            param_name.cyan(),
            param_type.yellow(),
            param_desc.dimmed()
          );
        }
      }
    }

    println!();
  }

  println!(
    "{}",
    format!("Total: {} tools available", tools.len())
      .bold()
      .green()
  );

  Ok(())
}
