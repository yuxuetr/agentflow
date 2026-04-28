use crate::config::v2::NodeDefinitionV2;
use agentflow_agents::{AgentNodeResumeContract, AgentRunResult};
use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  flow::{GraphNode, NodeType},
  value::FlowValue,
};
use agentflow_llm::AgentFlow;
use agentflow_nodes::nodes::{
  arxiv::ArxivNode, asr::ASRNode, file::FileNode, http::HttpNode, image_edit::ImageEditNode,
  image_to_image::ImageToImageNode, image_understand::ImageUnderstandNode, llm::LlmNode,
  markmap::MarkMapNode, template::TemplateNode, text_to_image::TextToImageNode, tts::TTSNode,
};
use agentflow_skills::{SkillBuilder, SkillLoader};
use async_trait::async_trait;
use serde_json::{json, Value};

#[cfg(feature = "mcp")]
use agentflow_nodes::nodes::mcp::MCPNode;

#[cfg(feature = "rag")]
use agentflow_nodes::nodes::rag::RAGNode;

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

// Helper to get a string parameter from the node definition, returning a default if not found.
fn get_string_param_optional(params: &HashMap<String, serde_yaml::Value>, key: &str) -> String {
  params
    .get(key)
    .and_then(|v| v.as_str())
    .map(|s| s.to_string())
    .unwrap_or_default()
}

pub fn create_graph_node(node_def: &NodeDefinitionV2) -> Result<GraphNode> {
  let node_type = match node_def.node_type.as_str() {
    "llm" => Ok(NodeType::Standard(Arc::new(LlmNode))),
    "skill_agent" | "agent" => {
      let node = SkillAgentWorkflowNode::new(&node_def.id);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "http" => Ok(NodeType::Standard(Arc::new(HttpNode))),
    "file" => Ok(NodeType::Standard(Arc::new(FileNode))),
    "template" => {
      let template_str = get_string_param_optional(&node_def.parameters, "template");
      let mut node = TemplateNode::new(&node_def.id, &template_str);

      let output_key = get_string_param_optional(&node_def.parameters, "output_key");
      if !output_key.is_empty() {
        node = node.with_output_key(&output_key);
      }

      let output_format = get_string_param_optional(&node_def.parameters, "output_format");
      if !output_format.is_empty() {
        node = node.with_format(&output_format);
      }

      Ok(NodeType::Standard(Arc::new(node)))
    }
    "arxiv" => {
      let url = get_string_param_optional(&node_def.parameters, "url");
      let fetch_source = node_def
        .parameters
        .get("fetch_source")
        .and_then(|v| v.as_bool());
      let simplify_latex = node_def
        .parameters
        .get("simplify_latex")
        .and_then(|v| v.as_bool());
      let node = ArxivNode {
        name: node_def.id.clone(),
        url,
        fetch_source,
        simplify_latex,
      };
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "asr" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let audio_source = get_string_param_optional(&node_def.parameters, "audio_source");
      let node = ASRNode::new(&node_def.id, &model, &audio_source);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "image_edit" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let prompt = get_string_param_optional(&node_def.parameters, "prompt");
      let image_source = get_string_param_optional(&node_def.parameters, "image_source");
      let node = ImageEditNode::new(&node_def.id, &model, &prompt, &image_source);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "image_to_image" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let prompt = get_string_param_optional(&node_def.parameters, "prompt");
      let source_image = get_string_param_optional(&node_def.parameters, "source_image");
      let node = ImageToImageNode::new(&node_def.id, &model, &prompt, &source_image);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "image_understand" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let text_prompt = get_string_param_optional(&node_def.parameters, "text_prompt");
      let image_source = get_string_param_optional(&node_def.parameters, "image_source");
      let node = ImageUnderstandNode::new(&node_def.id, &model, &text_prompt, &image_source);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "markmap" => {
      // markdown content will be provided via input_mapping at runtime
      let markdown = get_string_param_optional(&node_def.parameters, "markdown");
      let mut node = MarkMapNode::new(node_def.id.clone(), markdown);

      // Check if save_to_file parameter is provided
      if let Some(save_path) = node_def
        .parameters
        .get("save_to_file")
        .and_then(|v| v.as_str())
      {
        node.save_to_file = Some(save_path.to_string());
      }

      Ok(NodeType::Standard(Arc::new(node)))
    }
    "text_to_image" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let node = TextToImageNode::new(&node_def.id, &model);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "tts" => {
      let model = get_string_param_optional(&node_def.parameters, "model");
      let voice = get_string_param_optional(&node_def.parameters, "voice");
      let input_template = get_string_param_optional(&node_def.parameters, "input_template");
      let node = TTSNode::new(&node_def.id, &model, &voice, &input_template);
      Ok(NodeType::Standard(Arc::new(node)))
    }
    "while" => {
      let condition = get_string_param_optional(&node_def.parameters, "condition");
      let max_iterations = node_def
        .parameters
        .get("max_iterations")
        .and_then(|v| v.as_u64())
        .context("While node requires a 'max_iterations' parameter")?
        as u32;
      let do_nodes_yaml = node_def
        .parameters
        .get("do")
        .context("While node requires a 'do' block")?;
      let do_nodes_def: Vec<NodeDefinitionV2> = serde_yaml::from_value(do_nodes_yaml.clone())?;
      let template: Vec<GraphNode> = do_nodes_def
        .iter()
        .map(create_graph_node)
        .collect::<Result<_>>()?;
      Ok(NodeType::While {
        condition,
        max_iterations,
        template,
      })
    }
    "map" => {
      let template_nodes_yaml = node_def
        .parameters
        .get("template")
        .context("Map node requires a 'template' block")?;
      let template_nodes_def: Vec<NodeDefinitionV2> =
        serde_yaml::from_value(template_nodes_yaml.clone())?;
      let parallel = node_def
        .parameters
        .get("parallel")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
      let template: Vec<GraphNode> = template_nodes_def
        .iter()
        .map(create_graph_node)
        .collect::<Result<_>>()?;
      Ok(NodeType::Map { template, parallel })
    }
    #[cfg(feature = "mcp")]
    "mcp" => {
      // Extract server_command (required)
      let server_command = node_def
        .parameters
        .get("server_command")
        .and_then(|v| match v {
          serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .map(|s| s.as_str().map(|s| s.to_string()))
            .collect(),
          _ => None,
        })
        .context("MCP node requires 'server_command' as an array of strings")?;

      // Extract tool_name (required)
      let tool_name = get_string_param_optional(&node_def.parameters, "tool_name");
      if tool_name.is_empty() {
        return Err(anyhow!("MCP node requires 'tool_name' parameter"));
      }

      // Extract tool_params (optional, default to empty object)
      let tool_params = node_def
        .parameters
        .get("tool_params")
        .map(|v| serde_yaml::from_value(v.clone()))
        .transpose()?
        .unwrap_or(serde_json::json!({}));

      // Create MCPNode
      let mut node = MCPNode::new(server_command, tool_name).with_params(tool_params);

      // Optional timeout_ms
      if let Some(timeout) = node_def
        .parameters
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
      {
        node = node.with_timeout_ms(timeout);
      }

      // Optional max_retries
      if let Some(retries) = node_def
        .parameters
        .get("max_retries")
        .and_then(|v| v.as_u64())
      {
        node = node.with_max_retries(retries as u32);
      }

      Ok(NodeType::Standard(Arc::new(node)))
    }
    #[cfg(feature = "rag")]
    "rag" => {
      // Extract operation (required)
      let operation = get_string_param_optional(&node_def.parameters, "operation");
      if operation.is_empty() {
        return Err(anyhow!("RAG node requires 'operation' parameter (search, index, create_collection, delete_collection, stats)"));
      }

      // Extract collection (required)
      let collection = get_string_param_optional(&node_def.parameters, "collection");
      if collection.is_empty() {
        return Err(anyhow!("RAG node requires 'collection' parameter"));
      }

      // Create RAGNode with builder pattern
      let mut node = RAGNode::new(operation, collection);

      // Optional qdrant_url
      let qdrant_url = get_string_param_optional(&node_def.parameters, "qdrant_url");
      if !qdrant_url.is_empty() {
        node = node.with_qdrant_url(qdrant_url);
      }

      // Optional embedding_model
      let embedding_model = get_string_param_optional(&node_def.parameters, "embedding_model");
      if !embedding_model.is_empty() {
        node = node.with_embedding_model(embedding_model);
      }

      Ok(NodeType::Standard(Arc::new(node)))
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
    if k == "do" || k == "template" {
      continue;
    }
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

#[derive(Debug, Clone)]
struct SkillAgentWorkflowNode {
  name: String,
}

impl SkillAgentWorkflowNode {
  fn new(name: &str) -> Self {
    Self {
      name: name.to_string(),
    }
  }
}

#[async_trait]
impl AsyncNode for SkillAgentWorkflowNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let skill_dir =
      get_required_string(inputs, "skill").map_err(|message| AgentFlowError::NodeInputError {
        message: format!("skill_agent '{}': {}", self.name, message),
      })?;
    let message =
      get_required_string(inputs, "message").map_err(|message| AgentFlowError::NodeInputError {
        message: format!("skill_agent '{}': {}", self.name, message),
      })?;
    let model_override =
      get_optional_string(inputs, "model").map_err(|message| AgentFlowError::NodeInputError {
        message: format!("skill_agent '{}': {}", self.name, message),
      })?;

    let dir = std::path::Path::new(skill_dir);
    let mut manifest = SkillLoader::load(dir).map_err(|err| AgentFlowError::NodeInputError {
      message: format!(
        "skill_agent '{}': failed to load skill '{}': {}",
        self.name, skill_dir, err
      ),
    })?;
    if let Some(model) = model_override {
      manifest.model.name = Some(model.to_string());
    }
    SkillLoader::validate(&manifest, dir).map_err(|err| AgentFlowError::NodeInputError {
      message: format!(
        "skill_agent '{}': skill validation failed for '{}': {}",
        self.name, skill_dir, err
      ),
    })?;

    AgentFlow::init()
      .await
      .map_err(|err| AgentFlowError::ConfigurationError {
        message: format!(
          "skill_agent '{}': failed to initialize LLM: {}",
          self.name, err
        ),
      })?;

    let mut agent = SkillBuilder::build(&manifest, dir).await.map_err(|err| {
      AgentFlowError::NodeExecutionFailed {
        message: format!(
          "skill_agent '{}': failed to build agent from skill '{}': {}",
          self.name, skill_dir, err
        ),
      }
    })?;

    let result =
      agent
        .run_with_trace(message)
        .await
        .map_err(|err| AgentFlowError::NodeExecutionFailed {
          message: format!("skill_agent '{}': agent run failed: {}", self.name, err),
        })?;

    if !result.stop_reason.is_success() {
      let partial_outputs = build_skill_agent_outputs(&self.name, &result)?;
      return Err(AgentFlowError::NodePartialExecutionFailed {
        message: format!(
          "skill_agent '{}': agent stopped before final answer: {:?}",
          self.name, result.stop_reason
        ),
        partial_outputs,
      });
    }

    build_skill_agent_outputs(&self.name, &result)
  }
}

fn get_required_string<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a str, String> {
  get_optional_string(inputs, key)?.ok_or_else(|| format!("required input '{}' is missing", key))
}

fn get_optional_string<'a>(
  inputs: &'a AsyncNodeInputs,
  key: &str,
) -> Result<Option<&'a str>, String> {
  match inputs.get(key) {
    None => Ok(None),
    Some(FlowValue::Json(Value::String(value))) => Ok(Some(value.as_str())),
    Some(_) => Err(format!("input '{}' must be a string", key)),
  }
}

fn build_skill_agent_outputs(node_name: &str, result: &AgentRunResult) -> AsyncNodeResult {
  let response = result.answer.clone().unwrap_or_default();
  let stop_reason = serde_json::to_value(&result.stop_reason).map_err(|err| {
    AgentFlowError::NodeExecutionFailed {
      message: format!(
        "skill_agent '{}': failed to serialize stop reason: {}",
        node_name, err
      ),
    }
  })?;
  let agent_result =
    serde_json::to_value(result).map_err(|err| AgentFlowError::NodeExecutionFailed {
      message: format!(
        "skill_agent '{}': failed to serialize runtime result: {}",
        node_name, err
      ),
    })?;
  let agent_resume = serde_json::to_value(AgentNodeResumeContract::from_result(
    node_name,
    "skill_agent",
    result,
  ))
  .map_err(|err| AgentFlowError::NodeExecutionFailed {
    message: format!(
      "skill_agent '{}': failed to serialize resume contract: {}",
      node_name, err
    ),
  })?;

  let mut outputs = std::collections::HashMap::new();
  outputs.insert("response".to_string(), FlowValue::Json(json!(response)));
  outputs.insert(
    "session_id".to_string(),
    FlowValue::Json(json!(result.session_id)),
  );
  outputs.insert("stop_reason".to_string(), FlowValue::Json(stop_reason));
  outputs.insert("agent_result".to_string(), FlowValue::Json(agent_result));
  outputs.insert("agent_resume".to_string(), FlowValue::Json(agent_resume));
  Ok(outputs)
}
