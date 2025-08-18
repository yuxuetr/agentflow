use serde_yaml;
use std::fs;

#[derive(Debug, serde::Deserialize)]
struct TestConfig {
    name: String,
    workflow: WorkflowDef,
}

#[derive(Debug, serde::Deserialize)]
struct WorkflowDef {
    #[serde(rename = "type")]
    workflow_type: String,
    nodes: Vec<serde_yaml::Value>,
}

fn main() {
    let content = fs::read_to_string("examples/workflows/hello_world.yml").expect("Failed to read file");
    println!("File content length: {}", content.len());
    
    // Try parsing as raw YAML first
    let raw: serde_yaml::Value = serde_yaml::from_str(&content).expect("Failed to parse as raw YAML");
    println!("Raw YAML parsed successfully!");
    println!("Keys: {:?}", raw.as_mapping().unwrap().keys().collect::<Vec<_>>());
    
    // Try parsing as our structure
    match serde_yaml::from_str::<TestConfig>(&content) {
        Ok(config) => {
            println!("Parsed successfully: {}", config.name);
            println!("Workflow type: {}", config.workflow.workflow_type);
            println!("Number of nodes: {}", config.workflow.nodes.len());
        }
        Err(e) => {
            println!("Parse error: {}", e);
        }
    }
}