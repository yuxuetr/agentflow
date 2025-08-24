//! Test MCP integration with MarkMap server

use agentflow_mcp::{MCPClient, ToolCall};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing MCP MarkMap integration...");
    
    // Create MCP client
    let server_command = vec![
        "npx".to_string(),
        "-y".to_string(),
        "@jinzcdev/markmap-mcp-server".to_string(),
    ];
    
    let mut client = MCPClient::stdio(server_command);
    
    println!("Connecting to MarkMap MCP server...");
    client.connect().await?;
    
    println!("Listing available tools...");
    let tools = client.list_tools().await?;
    println!("Available tools: {:#?}", tools);
    
    // Test mind map generation
    let test_markdown = r#"
# Test Mind Map

## Main Topic 1
- Point A
- Point B

## Main Topic 2  
- Point C
- Point D
"#;

    println!("Creating mind map from markdown...");
    let tool_call = ToolCall::new("markdown-to-mindmap", json!({
        "markdown": test_markdown,
        "open": false
    }));
    
    let result = client.call_tool(tool_call).await?;
    println!("Mind map result: {:#?}", result);
    
    client.disconnect().await?;
    println!("Test completed successfully!");
    
    Ok(())
}