use crate::config::v2::FlowDefinitionV2;
use crate::executor::factory;
use anyhow::{Context, Result};
use agentflow_core::flow::Flow;
use std::fs;

pub async fn execute(
  workflow_file: String,
  _watch: bool, // not implemented
  _output: Option<String>, // not implemented
  _input: Vec<(String, String)>, // not implemented
  _dry_run: bool, // not implemented
  _timeout: String, // not implemented
  _max_retries: u32, // not implemented
) -> Result<()> {
    println!("🚀 Starting AgentFlow V2 workflow execution: {}", workflow_file);

    // 1. Read and parse the V2 workflow file
    let yaml_content = fs::read_to_string(&workflow_file)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;
    let flow_def: FlowDefinitionV2 = serde_yaml::from_str(&yaml_content)
        .with_context(|| "Failed to parse V2 workflow YAML.")?;

    println!("📄 Workflow '\'{}\'\' loaded.", flow_def.name);

    // 2. Build the core Flow object from the definition
    let mut flow = Flow::new();
    println!("🔨 Building workflow graph with {} nodes...", flow_def.nodes.len());
    for node_def in &flow_def.nodes {
        let graph_node = factory::create_graph_node(node_def)
            .with_context(|| format!("Failed to create graph node for id: {}", node_def.id))?;
        flow.add_node(graph_node);
        println!("  - Added node '{}' (type: '{}')", node_def.id, node_def.node_type);
    }

    // 3. Execute the flow
    println!("\n▶️  Running flow...");
    let start_time = std::time::Instant::now();
    let final_state = flow.run().await.unwrap();
    let duration = start_time.elapsed();
    println!("\n✅ Workflow completed in {:.2?}.", duration);

    // 4. Print the results
    println!("DEBUG: Final state before returning: {:?}", final_state);
    println!("\n📊 Final State Pool:");
    let final_state_json = serde_json::to_string_pretty(&final_state)
        .context("Failed to serialize final state to JSON.")?;
    println!("{}", final_state_json);

    Ok(())
}

