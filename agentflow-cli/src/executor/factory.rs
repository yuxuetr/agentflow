use std::sync::Arc;
use crate::config::v2::NodeDefinitionV2;
use agentflow_core::{
    async_node::AsyncNode,
    flow::{GraphNode, NodeType},
};
use agentflow_nodes::LlmNode;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

pub fn create_graph_node(node_def: &NodeDefinitionV2) -> Result<GraphNode> {
    let node_arc: Arc<dyn AsyncNode> = match node_def.node_type.as_str() {
        "llm" => Arc::new(LlmNode::default()),
        _ => return Err(anyhow!("Unknown node type: {}", node_def.node_type)),
    };

    let mut input_mapping = HashMap::new();
    for (k, v) in &node_def.input_mapping {
        let path = v.trim_start_matches("{{ ").trim_end_matches(" }}");
        let parts: Vec<&str> = path.split('.').collect();

        if parts.len() == 4 && parts[0] == "nodes" && parts[2] == "outputs" {
            input_mapping.insert(k.clone(), (parts[1].to_string(), parts[3].to_string()));
        }
    }

    let mut initial_inputs = HashMap::new();
    for (k, v) in &node_def.parameters {
        let json_val: serde_json::Value = serde_yaml::from_value(v.clone())?;
        let flow_value = agentflow_core::value::FlowValue::Json(json_val);
        initial_inputs.insert(k.clone(), flow_value);
    }

    Ok(GraphNode {
        id: node_def.id.clone(),
        node_type: NodeType::Standard(node_arc),
        dependencies: node_def.dependencies.clone(),
        input_mapping: Some(input_mapping),
        run_if: node_def.run_if.clone(),
        initial_inputs,
    })
}
