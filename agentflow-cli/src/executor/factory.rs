use crate::config::v2::NodeDefinitionV2;
use agentflow_core::{
    flow::{GraphNode, NodeType},
    value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

pub fn create_graph_node(node_def: &NodeDefinitionV2) -> Result<GraphNode> {
    let node_type = match node_def.node_type.as_str() {
        "llm" => Ok(NodeType::Standard(Arc::new(LlmNode::default()))),
        "template" => {
            let template = node_def.parameters.get("template").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let node = TemplateNode::new(&node_def.id, &template);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "while" => {
            let condition = node_def.parameters.get("condition").and_then(|v| v.as_str()).context("While node requires a 'condition' parameter")?.to_string();
            let max_iterations = node_def.parameters.get("max_iterations").and_then(|v| v.as_u64()).context("While node requires a 'max_iterations' parameter")? as u32;
            let do_nodes_yaml = node_def.parameters.get("do").context("While node requires a 'do' block")?;
            let do_nodes_def: Vec<NodeDefinitionV2> = serde_yaml::from_value(do_nodes_yaml.clone())?;

            let template: Vec<GraphNode> = do_nodes_def.iter().map(create_graph_node).collect::<Result<_>>()?;

            Ok(NodeType::While {
                condition,
                max_iterations,
                template,
            })
        },
        "map" => {
            let template_nodes_yaml = node_def.parameters.get("template").context("Map node requires a 'template' block")?;
            let template_nodes_def: Vec<NodeDefinitionV2> = serde_yaml::from_value(template_nodes_yaml.clone())?;

            let template: Vec<GraphNode> = template_nodes_def.iter().map(create_graph_node).collect::<Result<_>>()?;

            Ok(NodeType::Map { template })
        }
        _ => Err(anyhow!("Unknown node type: {}", node_def.node_type)),
    }?; 

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
        if k == "do" || k == "template" { continue; }
        let json_val: serde_json::Value = serde_yaml::from_value(v.clone())?;
        let flow_value = agentflow_core::value::FlowValue::Json(json_val);
        initial_inputs.insert(k.clone(), flow_value);
    }

    Ok(GraphNode {
        id: node_def.id.clone(),
        node_type,
        dependencies: node_def.dependencies.clone(),
        input_mapping: Some(input_mapping),
        run_if: node_def.run_if.clone(),
        initial_inputs,
    })
}
