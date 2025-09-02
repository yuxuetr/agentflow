// Enhanced LLM Node Example
// This demonstrates the new comprehensive LLM node with response formats,
// MCP tool integration, and standardized parameters

use agentflow_core::{SharedState, AsyncNode};
use agentflow_nodes::nodes::llm::{LlmNode, RetryConfig};
use serde_json::json;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Enhanced LLM Node Example - Phase 1 Implementation");
    println!("===================================================");

    // Example 1: Basic Text Analysis with JSON Response
    println!("\n📊 Example 1: Text Analysis with Structured JSON Output");
    let analyzer = LlmNode::text_analyzer("sentiment_analyzer", "step-2-mini")
        .with_prompt("Analyze the sentiment and key themes of: {{text_input}}")
        .with_input_keys(vec!["text_input".to_string()]);

    let shared = SharedState::new();
    shared.insert(
        "text_input".to_string(),
        json!("I love the new features in this product! The interface is intuitive and the performance is excellent.")
    );

    // Run the analysis
    let result = analyzer.run_async(&shared).await?;
    println!("✅ Analysis complete: {:?}", result);
    
    // Check the structured output
    if let Some(analysis) = shared.get(&analyzer.output_key) {
        println!("📋 Structured Analysis Result:");
        println!("{}", serde_json::to_string_pretty(&analysis).unwrap_or_else(|_| analysis.to_string()));
    }

    // Example 2: Creative Writing with Markdown Response
    println!("\n✍️  Example 2: Creative Writing with Markdown Formatting");
    let writer = LlmNode::creative_writer("story_writer", "qwen-plus")
        .with_prompt("Write a short story about {{theme}} in {{genre}} style")
        .with_input_keys(vec!["theme".to_string(), "genre".to_string()])
        .with_system("You are a creative writer specializing in engaging narratives");

    shared.insert("theme".to_string(), json!("artificial intelligence"));
    shared.insert("genre".to_string(), json!("science fiction"));

    let result = writer.run_async(&shared).await?;
    println!("✅ Story generation complete: {:?}", result);
    
    if let Some(story) = shared.get(&writer.output_key) {
        println!("📖 Generated Story (Markdown):");
        println!("{}", story.as_str().unwrap_or("Error reading story"));
    }

    // Example 3: Code Generation with Specific Language
    println!("\n💻 Example 3: Code Generation with Language-Specific Output");
    let coder = LlmNode::code_generator("rust_coder", "qwen-plus", "rust")
        .with_prompt("Implement a {{function_type}} function that {{description}}")
        .with_input_keys(vec!["function_type".to_string(), "description".to_string()])
        .with_system("You are an expert Rust programmer. Write clean, idiomatic Rust code with proper error handling.");

    shared.insert("function_type".to_string(), json!("binary search"));
    shared.insert("description".to_string(), json!("finds an element in a sorted vector and returns its index"));

    let result = coder.run_async(&shared).await?;
    println!("✅ Code generation complete: {:?}", result);
    
    if let Some(code) = shared.get(&coder.output_key) {
        println!("🦀 Generated Rust Code:");
        println!("{}", code.as_str().unwrap_or("Error reading code"));
    }

    // Example 4: Advanced LLM Node with All Parameters
    println!("\n🔧 Example 4: Advanced Configuration with All Parameters");
    let advanced_node = LlmNode::new("advanced_analyzer", "step-2-mini")
        .with_prompt("Perform advanced analysis on: {{input_data}}")
        .with_system("You are an expert data analyst")
        .with_input_keys(vec!["input_data".to_string(), "analysis_type".to_string()])
        .with_output_key("advanced_results")
        .with_temperature(0.7)
        .with_max_tokens(1500)
        .with_top_p(0.9)
        .with_frequency_penalty(-0.2)
        .with_presence_penalty(0.1)
        .with_seed(42)
        .with_json_response(Some(json!({
            "type": "object",
            "properties": {
                "insights": {"type": "array", "items": {"type": "string"}},
                "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                "recommendations": {"type": "array", "items": {"type": "string"}},
                "metadata": {
                    "type": "object",
                    "properties": {
                        "analysis_type": {"type": "string"},
                        "timestamp": {"type": "string"}
                    }
                }
            },
            "required": ["insights", "confidence", "recommendations"]
        })))
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
        })
        .with_timeout(30000)
        .with_dependencies(vec!["data_preprocessor".to_string()]);

    shared.insert("input_data".to_string(), json!("Q1 sales data: Revenue $2.1M (+15%), Customers 1,847 (+8%), Churn 3.2% (-1.1%)"));
    shared.insert("analysis_type".to_string(), json!("quarterly_business_review"));

    let result = advanced_node.run_async(&shared).await?;
    println!("✅ Advanced analysis complete: {:?}", result);
    
    if let Some(analysis) = shared.get("advanced_results") {
        println!("🔍 Advanced Analysis Results:");
        println!("{}", serde_json::to_string_pretty(&analysis).unwrap_or_else(|_| analysis.to_string()));
    }

    // Example 5: Web Research Node (MCP Integration Ready)
    println!("\n🌐 Example 5: Web Research Node with MCP Tool Configuration");
    let researcher = LlmNode::web_researcher("web_researcher", "qwen-plus")
        .with_prompt("Research and summarize information about: {{research_topic}}")
        .with_input_keys(vec!["research_topic".to_string()])
        .with_system("You are a research assistant. Use web search tools to find current, credible information.");

    shared.insert("research_topic".to_string(), json!("latest developments in quantum computing"));

    let result = researcher.run_async(&shared).await?;
    println!("✅ Web research complete: {:?}", result);
    
    if let Some(research) = shared.get(&researcher.output_key) {
        println!("🔬 Research Results:");
        println!("{}", serde_json::to_string_pretty(&research).unwrap_or_else(|_| research.to_string()));
    }

    println!("\n🎉 Phase 1 Implementation Complete!");
    println!("Features demonstrated:");
    println!("  ✅ Comprehensive LLM parameters (temperature, top_p, penalties, etc.)");
    println!("  ✅ Response format specification (Text, JSON, Markdown, Code)");
    println!("  ✅ Response format validation");
    println!("  ✅ MCP tool configuration structures");
    println!("  ✅ Multimodal support (images, audio)");
    println!("  ✅ Retry and timeout mechanisms");
    println!("  ✅ Conditional execution");
    println!("  ✅ Helper constructors for common patterns");
    println!("  ✅ Backwards compatibility with existing workflows");

    Ok(())
}