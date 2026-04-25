//! Test MarkMap MCP integration

use agentflow_mcp::client::ClientBuilder;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🧪 Testing MarkMap MCP integration...");

  // Create MCP client
  let server_command = vec![
    "npx".to_string(),
    "-y".to_string(),
    "@jinzcdev/markmap-mcp-server".to_string(),
  ];

  let mut client = ClientBuilder::new()
    .with_stdio(server_command)
    .build()
    .await?;

  println!("🔗 Connecting to MarkMap MCP server...");
  match client.connect().await {
    Ok(_) => println!("✅ Connected successfully"),
    Err(e) => {
      println!("❌ Connection failed: {}", e);
      return Err(e.into());
    }
  }

  println!("📋 Listing available tools...");
  match client.list_tools().await {
    Ok(tools) => {
      println!("✅ Available tools:");
      for tool in &tools {
        println!("  - {}: {:?}", tool.name, tool.description);
      }
    }
    Err(e) => {
      println!("❌ Failed to list tools: {}", e);
    }
  }

  // Test mind map generation
  let test_markdown = r#"# Test Mind Map

## Research Areas
- Natural Language Processing
- Machine Learning
- Computer Vision

## Applications  
- Text Analysis
- Image Recognition
- Speech Processing

## Future Directions
- Multi-modal AI
- Autonomous Systems
- Ethical AI"#;

  println!("🎨 Creating mind map from markdown...");
  let tool_args = json!({
      "markdown": test_markdown,
      "open": false
  });

  match client.call_tool("markdown-to-mindmap", tool_args).await {
    Ok(result) => {
      println!("✅ Mind map generated successfully!");
      if let Some(text) = result.first_text() {
        println!("📄 Output path: {}", text);
      }
    }
    Err(e) => {
      println!("❌ Mind map generation failed: {}", e);
      return Err(e.into());
    }
  }

  client.disconnect().await?;
  println!("🎉 Test completed successfully!");

  Ok(())
}
