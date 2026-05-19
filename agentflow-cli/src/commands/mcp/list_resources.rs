use agentflow_mcp::client::ClientBuilder;
use anyhow::{Context, Result};
use colored::*;
use std::time::Duration;

/// Execute the list-resources command to discover available resources from an MCP server
pub async fn execute(
  server_command: Vec<String>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
  format: String,
) -> Result<()> {
  if server_command.is_empty() {
    anyhow::bail!(
      "Server command cannot be empty. Example: npx -y @modelcontextprotocol/server-filesystem /tmp"
    );
  }

  let is_json_envelope = format == "json-envelope";

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

  // List available resources
  let resources = client
    .list_resources()
    .await
    .context("Failed to list resources from MCP server")?;

  // Disconnect gracefully
  client.disconnect().await.ok();

  if is_json_envelope {
    // P3.3 migration: envelope-wrap the full `Resource` list
    // (preserves uri + name + description + mime_type).
    let payload = serde_json::json!({
      "server_command": server_command,
      "resources": &resources,
      "total": resources.len(),
    });
    let envelope = crate::json_envelope::CliJsonEnvelope::ok("mcp list-resources", &payload);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    return Ok(());
  }

  // Display results (text mode)
  if resources.is_empty() {
    println!("{}", "⚠️  No resources found".yellow());
    return Ok(());
  }

  println!();
  println!(
    "{}",
    format!("Available Resources ({}):", resources.len())
      .bold()
      .green()
  );
  println!();

  for resource in &resources {
    println!("  {}", format!("• {}", resource.name).bold());

    if let Some(description) = &resource.description {
      println!("    {}", description.dimmed());
    }

    // Display URI
    println!("    {}: {}", "URI".italic(), resource.uri.cyan());

    // Display MIME type if available
    if let Some(mime_type) = &resource.mime_type {
      println!("    {}: {}", "MIME Type".italic(), mime_type.yellow());
    }

    println!();
  }

  println!(
    "{}",
    format!("Total: {} resources available", resources.len())
      .bold()
      .green()
  );

  Ok(())
}
