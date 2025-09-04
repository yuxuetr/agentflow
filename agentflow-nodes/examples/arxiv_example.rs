//! Arxiv Node Example
//!
//! This example demonstrates how to use the ArxivNode to retrieve LaTeX source
//! content from arXiv papers using HTTP requests.

use agentflow_core::SharedState;
use agentflow_nodes::{ArxivNode, ArxivConfig, AsyncNode};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Arxiv Node Example ===");

    // Initialize shared state
    let shared_state = SharedState::new();
    
    // Add some template variables for paper selection
    shared_state.insert("paper_id".to_string(), json!("2312.07104"));
    shared_state.insert("paper_version".to_string(), json!("v2"));

    // Example 1: Basic usage with a well-known paper
    println!("\n1. Basic ArXiv paper retrieval:");
    let basic_node = ArxivNode::new(
        "basic_paper",
        "https://arxiv.org/abs/2312.07104" // A real arXiv paper
    )
    .with_output_key("paper_source")
    .with_output_directory("./arxiv_downloads");

    match basic_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("✅ Basic paper retrieval successful!");
            if let Some(result) = shared_state.get("paper_source") {
                println!("   Paper ID: {}", result["paper_id"].as_str().unwrap_or("unknown"));
                println!("   Content size: {} bytes", result["content_size"].as_u64().unwrap_or(0));
                if let Some(saved_path) = result["saved_path"].as_str() {
                    println!("   Saved to: {}", saved_path);
                }
                if let Some(latex_content) = result["latex_content"].as_str() {
                    println!("   LaTeX content preview: {}...", 
                        &latex_content[..std::cmp::min(100, latex_content.len())]);
                } else {
                    println!("   Content is binary (tar.gz archive)");
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
            println!("   Note: This might fail due to network issues or arXiv rate limiting");
        }
    }

    // Example 2: Template-based URL with advanced LaTeX processing
    println!("\n2. Template-based retrieval with LaTeX expansion:");
    let advanced_config = ArxivConfig {
        timeout_seconds: Some(120),
        save_latex: Some(true),
        extract_files: Some(true), // Extract tar.gz contents
        expand_content: Some(true), // Expand all included files
        max_include_depth: Some(5), // Limit recursion depth
        user_agent: Some("AgentFlow-Example/1.0".to_string()),
    };

    let template_node = ArxivNode::new(
        "template_paper",
        "https://arxiv.org/abs/{{paper_id}}{{paper_version}}" // Uses template variables
    )
    .with_config(advanced_config)
    .with_output_key("extracted_paper")
    .with_output_directory("./arxiv_extracted");

    match template_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("✅ Advanced LaTeX processing successful!");
            if let Some(result) = shared_state.get("extracted_paper") {
                println!("   Paper ID: {}", result["paper_id"].as_str().unwrap_or("unknown"));
                println!("   Version: {}", result["version"].as_str().unwrap_or("latest"));
                println!("   Original URL: {}", result["original_url"].as_str().unwrap_or("unknown"));
                println!("   Source URL: {}", result["source_url"].as_str().unwrap_or("unknown"));
                println!("   Content size: {} bytes", result["content_size"].as_u64().unwrap_or(0));
                
                // Show LaTeX processing results
                if let Some(latex_info) = result["latex_info"].as_object() {
                    if let Some(main_file) = latex_info.get("main_file") {
                        println!("   Main LaTeX file: {}", main_file.as_str().unwrap_or("unknown"));
                    }
                    if let Some(has_expanded) = latex_info.get("has_expanded_content") {
                        println!("   Content expanded: {}", has_expanded.as_bool().unwrap_or(false));
                    }
                    if let Some(files_count) = latex_info.get("extracted_files_count") {
                        println!("   Extracted files: {}", files_count.as_u64().unwrap_or(0));
                    }
                    if let Some(expanded_content) = latex_info.get("expanded_content") {
                        if let Some(content_str) = expanded_content.as_str() {
                            println!("   Expanded content length: {} characters", content_str.len());
                            println!("   Preview: {}...", 
                                &content_str[..std::cmp::min(100, content_str.len())]);
                        }
                    }
                }
                
                if let Some(saved_path) = result["saved_path"].as_str() {
                    println!("   Archive saved to: {}", saved_path);
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
            println!("   Note: This might fail due to network issues or arXiv rate limiting");
        }
    }

    // Example 3: Different URL formats validation  
    println!("\n3. Validating different URL formats:");
    let url_formats = vec![
        ("PDF URL", "https://arxiv.org/pdf/2312.07104.pdf"),
        ("Bare ID", "2312.07104"),
        ("Versioned ID", "2312.07104v1"),
        ("ABS URL", "https://arxiv.org/abs/2312.07104"),
    ];

    for (description, url) in url_formats {
        println!("\n   Testing {}: {}", description, url);
        
        let test_node = ArxivNode::new(
            &format!("test_{}", description.replace(" ", "_").to_lowercase()),
            url
        )
        .with_output_key(&format!("test_{}_result", description.replace(" ", "_").to_lowercase()));

        // Just show that the node was created successfully
        println!("     ✅ Node created successfully");
        println!("     ✅ Node ID: {}", test_node.get_node_id().unwrap_or("none".to_string()));
        
        // Note: We can't test the private methods from the example,
        // but they are tested in the unit tests within the module
    }

    // Example 4: Error handling - invalid URL
    println!("\n4. Error handling with invalid URL:");
    let invalid_node = ArxivNode::new(
        "invalid_test",
        "not-a-valid-arxiv-url"
    )
    .with_output_key("invalid_result");

    match invalid_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("   ⚠️  Unexpected success with invalid URL");
        }
        Err(e) => {
            println!("   ✅ Expected error caught: {}", e);
        }
    }

    // Display final shared state (abbreviated)
    println!("\n=== Final Shared State Summary ===");
    for (key, value) in shared_state.iter() {
        if key.contains("paper") || key.contains("result") {
            println!("Key: {}", key);
            if let Some(obj) = value.as_object() {
                for (sub_key, sub_value) in obj {
                    match sub_key.as_str() {
                        "content_bytes" => println!("  {}: <binary data>", sub_key),
                        "latex_content" => {
                            if let Some(content) = sub_value.as_str() {
                                println!("  {}: {}...", sub_key, 
                                    &content[..std::cmp::min(50, content.len())]);
                            }
                        }
                        _ => println!("  {}: {}", sub_key, sub_value),
                    }
                }
            }
        }
    }

    println!("\n=== Arxiv Example Complete ===");
    println!("Note: Actual downloads may be limited by arXiv's rate limiting.");
    println!("Generated directories:");
    println!("  - ./arxiv_downloads/ (if downloads successful)");
    println!("  - ./arxiv_extracted/ (if extraction enabled and successful)");

    Ok(())
}