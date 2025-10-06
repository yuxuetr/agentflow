use crate::config::v2::NodeDefinitionV2;
use agentflow_core::{
    flow::{GraphNode, NodeType},
    value::FlowValue,
};
use agentflow_nodes::nodes::{
    arxiv::ArxivNode,
    asr::ASRNode,
    file::FileNode,
    http::HttpNode,
    image_edit::ImageEditNode,
    image_to_image::ImageToImageNode,
    image_understand::ImageUnderstandNode,
    llm::LlmNode,
    markmap::MarkMapNode,
    template::TemplateNode,
    text_to_image::TextToImageNode,
    tts::TTSNode,
};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

// Helper to get a string parameter from the node definition
fn get_string_param(params: &HashMap<String, serde_yaml::Value>, key: &str) -> Result<String> {
    params.get(key).and_then(|v| v.as_str()).map(|s| s.to_string()).context(format!("Missing or invalid string parameter '{}'", key))
}

// Helper to get an optional string parameter
fn get_optional_string_param(params: &HashMap<String, serde_yaml::Value>, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub fn create_graph_node(node_def: &NodeDefinitionV2) -> Result<GraphNode> {
    let node_type = match node_def.node_type.as_str() {
        "llm" => Ok(NodeType::Standard(Arc::new(LlmNode::default()))),
        "http" => Ok(NodeType::Standard(Arc::new(HttpNode::default()))),
        "file" => Ok(NodeType::Standard(Arc::new(FileNode::default()))),
        "template" => {
            let template = get_string_param(&node_def.parameters, "template")?;
            let node = TemplateNode::new(&node_def.id, &template);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "arxiv" => {
            let url = get_string_param(&node_def.parameters, "url")?;
            let mut node = ArxivNode { name: node_def.id.clone(), url, fetch_source: None, simplify_latex: None };
            // Note: optional params for ArxivNode not yet handled from YAML
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "asr" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let audio_source = get_string_param(&node_def.parameters, "audio_source")?;
            let node = ASRNode::new(&node_def.id, &model, &audio_source);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "image_edit" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let prompt = get_string_param(&node_def.parameters, "prompt")?;
            let image_source = get_string_param(&node_def.parameters, "image_source")?;
            let node = ImageEditNode::new(&node_def.id, &model, &prompt, &image_source);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "image_to_image" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let prompt = get_string_param(&node_def.parameters, "prompt")?;
            let source_image = get_string_param(&node_def.parameters, "source_image")?;
            let node = ImageToImageNode::new(&node_def.id, &model, &prompt, &source_image);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "image_understand" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let text_prompt = get_string_param(&node_def.parameters, "text_prompt")?;
            let image_source = get_string_param(&node_def.parameters, "image_source")?;
            let node = ImageUnderstandNode::new(&node_def.id, &model, &text_prompt, &image_source);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "markmap" => {
            let markdown = get_string_param(&node_def.parameters, "markdown")?;
            let node = MarkMapNode::new(node_def.id.clone(), markdown);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "text_to_image" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let node = TextToImageNode::new(&node_def.id, &model);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "tts" => {
            let model = get_string_param(&node_def.parameters, "model")?;
            let voice = get_string_param(&node_def.parameters, "voice")?;
            let input_template = get_string_param(&node_def.parameters, "input_template")?;
            let node = TTSNode::new(&node_def.id, &model, &voice, &input_template);
            Ok(NodeType::Standard(Arc::new(node)))
        },
        "while" => {
            let condition = get_string_param(&node_def.parameters, "condition")?;
            let max_iterations = node_def.parameters.get("max_iterations").and_then(|v| v.as_u64()).context("While node requires a 'max_iterations' parameter")? as u32;
            let do_nodes_yaml = node_def.parameters.get("do").context("While node requires a 'do' block")?;
            let do_nodes_def: Vec<NodeDefinitionV2> = serde_yaml::from_value(do_nodes_yaml.clone())?;
            let template: Vec<GraphNode> = do_nodes_def.iter().map(create_graph_node).collect::<Result<_>>()?;
            Ok(NodeType::While { condition, max_iterations, template })
        },
        "map" => {
            let template_nodes_yaml = node_def.parameters.get("template").context("Map node requires a 'template' block")?;
            let template_nodes_def: Vec<NodeDefinitionV2> = serde_yaml::from_value(template_nodes_yaml.clone())?;
            let parallel = node_def.parameters.get("parallel").and_then(|v| v.as_bool()).unwrap_or(false);
            let template: Vec<GraphNode> = template_nodes_def.iter().map(create_graph_node).collect::<Result<_>>()?;
            Ok(NodeType::Map { template, parallel })
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