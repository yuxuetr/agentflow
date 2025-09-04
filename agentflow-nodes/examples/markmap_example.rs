//! MarkMap Node Example
//!
//! This example demonstrates how to use the MarkMapNode to convert Markdown content
//! into interactive mind map HTML files using the markmap-api service.

use agentflow_core::SharedState;
use agentflow_nodes::{MarkMapNode, MarkMapConfig, AsyncNode};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MarkMap Node Example ===");

    // Initialize shared state
    let shared_state = SharedState::new();
    
    // Add some template variables
    shared_state.insert("project_name".to_string(), json!("AgentFlow"));
    shared_state.insert("version".to_string(), json!("1.0.0"));
    shared_state.insert("author".to_string(), json!("AgentFlow Team"));

    // Example 1: Basic usage with template variables
    println!("\n1. Basic MarkMap with template variables:");
    let basic_node = MarkMapNode::new(
        "project_overview",
        r#"# {{project_name}} v{{version}}

## Core Features
### Workflow Engine
- Async execution
- Node composition
- Error handling
- Observability

### LLM Integration
- Multiple providers
- Streaming support
- Multimodal capabilities
- Template processing

## Development
### Architecture
- Core abstractions
- Provider plugins
- Configuration management

### Team
- Created by {{author}}
- Open source community
- Continuous improvement

## Roadmap
### Phase 1
- Basic functionality
- Core nodes
- CLI interface

### Phase 2
- Advanced features
- Performance optimization
- Extended integrations
"#
    )
    .with_output_key("project_mindmap")
    .with_file_output("project_overview.html");

    match basic_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("✅ Basic mind map generated successfully!");
            if let Some(result) = shared_state.get("project_mindmap") {
                if let Some(html) = result.get("html") {
                    println!("   HTML length: {} characters", html.as_str().unwrap_or("").len());
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    // Example 2: Custom configuration with dark theme
    println!("\n2. MarkMap with custom configuration (dark theme):");
    let custom_config = MarkMapConfig {
        title: Some("AgentFlow Architecture".to_string()),
        theme: Some("dark".to_string()),
        color_freeze_level: Some(8),
        initial_expand_level: Some(2),
        max_width: Some(250),
        timeout_seconds: Some(45),
        ..Default::default()
    };

    let custom_node = MarkMapNode::new(
        "architecture_overview",
        r#"# AgentFlow Architecture

## Core Layer
### agentflow-core
- Workflow execution engine
- Async node framework
- Shared state management
- Observability infrastructure

### agentflow-llm
- LLM provider abstraction
- Model registry
- Multimodal support
- Streaming capabilities

## Application Layer
### agentflow-cli
- Command-line interface
- Configuration management
- User interaction

### agentflow-nodes
- Built-in node implementations
- Specialized processors
- Integration components

## Extension Layer
### agentflow-mcp (Planned)
- Model Context Protocol
- Tool integration
- Dynamic context

### agentflow-rag (Planned)
- Retrieval augmented generation
- Vector store integration
- Knowledge management

## Infrastructure
### Configuration
- YAML-based workflows
- Environment variables
- Hierarchical config

### Deployment
- Cross-platform binaries
- Container support
- Cloud deployment
"#
    )
    .with_config(custom_config)
    .with_output_key("architecture_mindmap")
    .with_file_output("architecture_dark.html");

    match custom_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("✅ Custom mind map generated successfully!");
            if let Some(result) = shared_state.get("architecture_mindmap") {
                if let Some(html) = result.get("html") {
                    println!("   HTML length: {} characters", html.as_str().unwrap_or("").len());
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    // Example 3: Simple content without templates
    println!("\n3. Simple mind map without templates:");
    let simple_node = MarkMapNode::new(
        "simple_example",
        r#"# My Project

## Planning
- Requirements gathering
- Architecture design
- Technology selection

## Development
- Core implementation
- Testing
- Documentation

## Deployment
- Build pipeline
- Testing environment
- Production deployment

## Maintenance
- Bug fixes
- Feature updates
- Performance optimization
"#
    )
    .with_output_key("simple_mindmap");

    match simple_node.run_async(&shared_state).await {
        Ok(_) => {
            println!("✅ Simple mind map generated successfully!");
            if let Some(result) = shared_state.get("simple_mindmap") {
                if let Some(html) = result.get("html") {
                    println!("   HTML length: {} characters", html.as_str().unwrap_or("").len());
                    println!("   Contains interactive features: {}", 
                        html.as_str().unwrap_or("").contains("markmap"));
                }
            }
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    // Display final shared state
    println!("\n=== Final Shared State ===");
    for (key, value) in shared_state.iter() {
        if key.contains("mindmap") {
            println!("Key: {}", key);
            if let Some(obj) = value.as_object() {
                for (sub_key, sub_value) in obj {
                    if sub_key == "html" {
                        println!("  {}: <HTML content {} chars>", 
                            sub_key, sub_value.as_str().unwrap_or("").len());
                    } else {
                        println!("  {}: {}", sub_key, sub_value);
                    }
                }
            }
        }
    }

    println!("\n=== MarkMap Example Complete ===");
    println!("Generated HTML files:");
    println!("  - project_overview.html (if successful)");
    println!("  - architecture_dark.html (if successful)");

    Ok(())
}