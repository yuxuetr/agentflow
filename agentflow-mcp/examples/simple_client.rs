//! Simple MCP client example
//!
//! This example demonstrates basic MCP client usage:
//! - Connecting to an MCP server
//! - Listing and calling tools
//! - Listing and reading resources
//! - Listing and getting prompts
//!
//! # Usage
//!
//! ```bash
//! # Run with a real MCP server
//! cargo run --example simple_client -- npx -y @modelcontextprotocol/server-everything
//!
//! # Or run with mock transport (for testing)
//! cargo run --example simple_client -- --mock
//! ```

use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::transport_new::MockTransport;
use serde_json::json;
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("=== AgentFlow MCP Client Example ===\n");

  let args: Vec<String> = env::args().collect();

  // Build client
  let mut client = if args.contains(&"--mock".to_string()) {
    println!("Using mock transport\n");
    build_mock_client().await?
  } else {
    let command: Vec<String> = args.iter().skip(1).map(|s| s.clone()).collect();

    if command.is_empty() {
      eprintln!("Usage: {} <command> [args...] or {} --mock", args[0], args[0]);
      eprintln!("\nExample:");
      eprintln!("  {} npx -y @modelcontextprotocol/server-everything", args[0]);
      std::process::exit(1);
    }

    println!("Connecting to MCP server: {:?}\n", command);

    ClientBuilder::new()
      .with_stdio(command)
      .with_timeout(Duration::from_secs(30))
      .with_max_retries(3)
      .build()
      .await?
  };

  // Connect and initialize
  println!("Connecting to server...");
  client.connect().await?;
  println!("✓ Connected and initialized\n");

  // Show server info
  if let Some(server_info) = client.server_info().await {
    println!("Server: {} v{}", server_info.name, server_info.version);
  }

  if let Some(capabilities) = client.server_capabilities().await {
    println!("Capabilities: {}\n", capabilities);
  }

  // List tools
  println!("--- Tools ---");
  match client.list_tools().await {
    Ok(tools) => {
      if tools.is_empty() {
        println!("No tools available");
      } else {
        for tool in &tools {
          println!("  • {} - {:?}", tool.name, tool.description);
        }

        // Try calling the first tool if available
        if let Some(first_tool) = tools.first() {
          println!("\nCalling tool '{}'...", first_tool.name);

          // Example: add_numbers might be available
          let result = client
            .call_tool(&first_tool.name, json!({"a": 5, "b": 3}))
            .await;

          match result {
            Ok(result) => {
              if let Some(text) = result.first_text() {
                println!("Result: {}", text);
              } else {
                println!("Result: {:?}", result.content);
              }
            }
            Err(e) => println!("Error calling tool: {}", e),
          }
        }
      }
    }
    Err(e) => println!("Error listing tools: {}", e),
  }

  // List resources
  println!("\n--- Resources ---");
  match client.list_resources().await {
    Ok(resources) => {
      if resources.is_empty() {
        println!("No resources available");
      } else {
        for resource in &resources {
          println!("  • {} - {:?}", resource.name, resource.description);
        }

        // Try reading the first resource
        if let Some(first_resource) = resources.first() {
          println!("\nReading resource '{}'...", first_resource.uri);

          match client.read_resource(&first_resource.uri).await {
            Ok(result) => {
              if let Some(content) = result.first_content() {
                if let Some(text) = content.as_text() {
                  println!("Content (first 200 chars): {}...", &text.chars().take(200).collect::<String>());
                } else {
                  println!("Binary content: {} bytes", content.as_blob().map(|b| b.len()).unwrap_or(0));
                }
              }
            }
            Err(e) => println!("Error reading resource: {}", e),
          }
        }
      }
    }
    Err(e) => println!("Error listing resources: {}", e),
  }

  // List prompts
  println!("\n--- Prompts ---");
  match client.list_prompts().await {
    Ok(prompts) => {
      if prompts.is_empty() {
        println!("No prompts available");
      } else {
        for prompt in &prompts {
          println!("  • {} - {:?}", prompt.name, prompt.description);
          if !prompt.arguments.is_empty() {
            println!("    Arguments:");
            for arg in &prompt.arguments {
              let required = if arg.is_required() { " (required)" } else { "" };
              println!("      - {}{} - {:?}", arg.name, required, arg.description);
            }
          }
        }

        // Try getting the first prompt
        if let Some(first_prompt) = prompts.first() {
          println!("\nGetting prompt '{}'...", first_prompt.name);

          // Build arguments
          let mut args = std::collections::HashMap::new();
          for arg in &first_prompt.arguments {
            if arg.is_required() {
              args.insert(arg.name.clone(), format!("example_{}", arg.name));
            }
          }

          match client.get_prompt(&first_prompt.name, args).await {
            Ok(result) => {
              println!("Messages: {} total", result.messages.len());
              for (i, message) in result.messages.iter().enumerate() {
                if let Some(text) = message.as_text() {
                  println!("  Message {}: {:?} - {}", i + 1, message.role, &text.chars().take(100).collect::<String>());
                }
              }
            }
            Err(e) => println!("Error getting prompt: {}", e),
          }
        }
      }
    }
    Err(e) => println!("Error listing prompts: {}", e),
  }

  // Disconnect
  println!("\n--- Disconnecting ---");
  client.disconnect().await?;
  println!("✓ Disconnected");

  Ok(())
}

/// Build a mock client for testing
async fn build_mock_client() -> Result<agentflow_mcp::client::MCPClient, Box<dyn std::error::Error>> {
  let mut transport = MockTransport::new();

  // Add responses
  transport.add_response(MockTransport::standard_initialize_response());

  // Tools
  transport.add_response(MockTransport::tools_list_response(vec![json!({
    "name": "add_numbers",
    "description": "Add two numbers together",
    "inputSchema": {
      "type": "object",
      "properties": {
        "a": {"type": "number"},
        "b": {"type": "number"}
      },
      "required": ["a", "b"]
    }
  })]));

  transport.add_response(MockTransport::tool_call_response(vec![json!({
    "type": "text",
    "text": "The sum is 8"
  })]));

  // Resources
  transport.add_response(MockTransport::resources_list_response(vec![json!({
    "uri": "file:///example.txt",
    "name": "example.txt",
    "description": "An example file",
    "mimeType": "text/plain"
  })]));

  transport.add_response(MockTransport::resource_read_response(vec![json!({
    "uri": "file:///example.txt",
    "mimeType": "text/plain",
    "text": "This is an example file content for demonstration purposes."
  })]));

  // Prompts
  transport.add_response(MockTransport::prompts_list_response(vec![json!({
    "name": "code_review",
    "description": "Review code for best practices",
    "arguments": [
      {
        "name": "code",
        "description": "The code to review",
        "required": true
      }
    ]
  })]));

  transport.add_response(MockTransport::prompt_get_response(vec![
    json!({
      "role": "user",
      "content": {
        "type": "text",
        "text": "Please review this code: example_code"
      }
    }),
    json!({
      "role": "assistant",
      "content": {
        "type": "text",
        "text": "I'll review the code for best practices and potential improvements."
      }
    }),
  ]));

  let client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await?;

  Ok(client)
}
