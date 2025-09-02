//! Example demonstrating real LLM integration instead of mocks
//!
//! This shows how agentflow-nodes should integrate with agentflow-llm
//! for actual model execution rather than using mock responses.

use agentflow_core::{SharedState, AsyncNode};
use agentflow_nodes::LlmNode;
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 LLM Integration Example");
    println!("=========================");
    println!();
    println!("This example shows how nodes should integrate with agentflow-llm");
    println!("for real model execution instead of mock responses.");
    println!();

    // Initialize agentflow-llm with configuration
    // In a real implementation, this would be called in the node's exec_async
    println!("📋 Current State: Using mock implementations");
    println!("🎯 Goal: Replace mocks with real agentflow-llm calls");
    println!();

    let shared = SharedState::new();
    shared.insert("product_name".to_string(), json!("AI-powered smart glasses"));

    // Example of what the integration should look like
    println!("🔧 Example Integration Pattern:");
    println!("-------------------------------");
    println!();
    println!("// Instead of mock response in exec_async:");
    println!("let mock_result = json!({{\"response\": \"mock text\"}});");
    println!();
    println!("// Should call agentflow-llm:");
    println!("let response = AgentFlow::model(&self.model)");
    println!("    .prompt(&resolved_prompt)");
    println!("    .temperature(self.temperature.unwrap_or(0.7))");
    println!("    .max_tokens(self.max_tokens.unwrap_or(1000))");
    println!("    .execute().await?;");
    println!();

    let llm_node = LlmNode::new("test_integration", "step-2-mini")
        .with_prompt("Write a creative description for: {{product_name}}")
        .with_temperature(0.8)
        .with_max_tokens(200)
        .with_input_keys(vec!["product_name".to_string()]);

    let result = llm_node.run_async(&shared).await?;
    println!("📤 Current Result : {:?}", result);
    
    if let Some(response) = shared.get(&llm_node.output_key) {
        println!("📋 Mock Response: {}", response.as_str().unwrap_or(""));
    }
    println!();

    println!("🚀 Next Steps for Integration:");
    println!("==============================");
    println!("1. Add agentflow-llm dependency to agentflow-nodes Cargo.toml");
    println!("2. Replace mock implementations in exec_async methods");
    println!("3. Handle authentication and configuration properly");
    println!("4. Map node parameters to agentflow-llm client parameters");
    println!("5. Ensure error handling and response format consistency");
    println!();
    println!("🎯 Benefits after integration:");
    println!("- Real AI model responses instead of mocks");
    println!("- Unified authentication and configuration");
    println!("- Consistent error handling across all nodes");
    println!("- Support for all LLM providers (OpenAI, Anthropic, StepFun, etc.)");

    Ok(())
}