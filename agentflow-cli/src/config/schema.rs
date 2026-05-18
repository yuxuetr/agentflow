use std::collections::{BTreeSet, HashSet};

use crate::config::v2::{FlowDefinitionV2, NodeDefinitionV2};
use agentflow_core::expr;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorkflowValidationReport {
  pub issues: Vec<String>,
  pub warnings: Vec<String>,
}

impl WorkflowValidationReport {
  pub fn is_valid(&self) -> bool {
    self.issues.is_empty()
  }
}

#[derive(Debug, Clone, Copy)]
enum ParamType {
  Any,
  String,
  Number,
  Integer,
  Bool,
  Object,
  Sequence,
  SequenceOfStrings,
}

#[derive(Debug, Clone, Copy)]
struct ParamSpec {
  name: &'static str,
  kind: ParamType,
  required: bool,
  input_allowed: bool,
}

impl ParamSpec {
  const fn required(name: &'static str, kind: ParamType) -> Self {
    Self {
      name,
      kind,
      required: true,
      input_allowed: false,
    }
  }

  const fn required_input(name: &'static str, kind: ParamType) -> Self {
    Self {
      name,
      kind,
      required: true,
      input_allowed: true,
    }
  }

  const fn optional(name: &'static str, kind: ParamType) -> Self {
    Self {
      name,
      kind,
      required: false,
      input_allowed: false,
    }
  }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WorkflowValidationOptions {
  pub unknown_parameters: UnknownParameterMode,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum UnknownParameterMode {
  #[default]
  Warning,
  Error,
}

pub fn validate_flow_definition(flow_def: &FlowDefinitionV2) -> WorkflowValidationReport {
  validate_flow_definition_with_options(flow_def, WorkflowValidationOptions::default())
}

pub fn validate_flow_definition_with_options(
  flow_def: &FlowDefinitionV2,
  options: WorkflowValidationOptions,
) -> WorkflowValidationReport {
  let mut report = WorkflowValidationReport::default();
  let mut seen_ids = HashSet::new();

  if flow_def.nodes.is_empty() {
    report
      .issues
      .push("workflow must define at least one node".to_string());
  }

  for (idx, node) in flow_def.nodes.iter().enumerate() {
    let path = format!("nodes[{}]", idx);
    if node.id.trim().is_empty() {
      report.issues.push(format!("{}.id must not be empty", path));
    } else if !seen_ids.insert(node.id.clone()) {
      report
        .issues
        .push(format!("{}.id '{}' is duplicated", path, node.id));
    }

    validate_node_schema(node, &path, options, &mut report);
  }

  let valid_ids: HashSet<_> = flow_def.nodes.iter().map(|node| node.id.as_str()).collect();
  for (idx, node) in flow_def.nodes.iter().enumerate() {
    let path = format!("nodes[{}]", idx);
    for dep in &node.dependencies {
      if !valid_ids.contains(dep.as_str()) {
        report.issues.push(format!(
          "{}.dependencies references unknown node '{}'",
          path, dep
        ));
      }
    }
    for (input_name, mapping) in &node.input_mapping {
      if let Some(source_node) = parse_mapping_source_node(mapping) {
        if !valid_ids.contains(source_node) {
          report.issues.push(format!(
            "{}.input_mapping.{} references unknown node '{}'",
            path, input_name, source_node
          ));
        }
      } else {
        report.warnings.push(format!(
          "{}.input_mapping.{} uses unsupported mapping expression '{}'",
          path, input_name, mapping
        ));
      }
    }
  }

  report
}

fn validate_node_schema(
  node: &NodeDefinitionV2,
  path: &str,
  options: WorkflowValidationOptions,
  report: &mut WorkflowValidationReport,
) {
  let specs = match specs_for_node_type(node.node_type.as_str()) {
    Some(specs) => specs,
    None => {
      report.issues.push(format!(
        "{}.type '{}' is not supported by the CLI workflow factory{}",
        path,
        node.node_type,
        feature_hint(node.node_type.as_str())
      ));
      return;
    }
  };

  let known: BTreeSet<_> = specs.iter().map(|spec| spec.name).collect();
  for spec in specs {
    let has_param = node.parameters.contains_key(spec.name);
    let has_input_mapping = spec.input_allowed && node.input_mapping.contains_key(spec.name);
    if spec.required && !has_param && !has_input_mapping {
      report.issues.push(format!(
        "{}.{} requires '{}' as a parameter{}",
        path,
        node.id,
        spec.name,
        if spec.input_allowed {
          " or input_mapping"
        } else {
          ""
        }
      ));
      continue;
    }

    if let Some(value) = node.parameters.get(spec.name) {
      validate_param_type(path, spec.name, value, spec.kind, report);
    }
  }

  for key in node.parameters.keys() {
    if !known.contains(key.as_str()) {
      let message = format!(
        "{}.{}.parameters.{} is not defined in the CLI schema for node type '{}'",
        path, node.id, key, node.node_type
      );
      match options.unknown_parameters {
        UnknownParameterMode::Warning => report.warnings.push(message),
        UnknownParameterMode::Error => report.issues.push(message),
      }
    }
  }

  if let Some(run_if) = &node.run_if
    && let Err(err) = expr::compile(run_if)
  {
    report
      .issues
      .push(format!("{}.{}.run_if is invalid: {}", path, node.id, err));
  }

  if node.node_type == "while"
    && let Some(condition) = node
      .parameters
      .get("condition")
      .and_then(serde_yaml::Value::as_str)
    && let Err(err) = expr::compile(condition)
  {
    report.issues.push(format!(
      "{}.{}.parameters.condition is invalid: {}",
      path, node.id, err
    ));
  }

  match node.node_type.as_str() {
    "map" => validate_nested_nodes(node, path, "template", options, report),
    "while" => validate_nested_nodes(node, path, "do", options, report),
    _ => {}
  }

  #[cfg(feature = "plugin")]
  if node.node_type == "plugin" {
    validate_plugin_node_type(node, path, report);
  }
}

/// Resolve the plugin manifest referenced by a `type: plugin` node and
/// verify that its `node_type` parameter matches one of the
/// `[[plugin.nodes]]` entries the manifest declares. Lets the operator
/// catch a typo or stale node name at validate time instead of at the
/// first workflow run.
///
/// This runs regardless of strict mode because a wrong `node_type` is
/// never benign — the runtime would always fail. Manifests that can't
/// be read are silently skipped (the missing-manifest case is already
/// caught by the structural require-`manifest` check), so this gate
/// is purely informational on top of what's already validated.
#[cfg(feature = "plugin")]
fn validate_plugin_node_type(
  node: &NodeDefinitionV2,
  path: &str,
  report: &mut WorkflowValidationReport,
) {
  let Some(manifest_str) = node.parameters.get("manifest").and_then(|v| v.as_str()) else {
    return;
  };
  let Some(node_type) = node.parameters.get("node_type").and_then(|v| v.as_str()) else {
    return;
  };
  let manifest_path = std::path::PathBuf::from(manifest_str);
  let resolved = if manifest_path.is_dir() {
    manifest_path.join("plugin.toml")
  } else {
    manifest_path
  };
  if !resolved.is_file() {
    // The structural validator already complained if the path was
    // missing; nothing to add here.
    return;
  }
  let (manifest, _dir) = match agentflow_core::plugin::PluginManifest::load_from_path(&resolved) {
    Ok(pair) => pair,
    Err(err) => {
      report.warnings.push(format!(
        "{}.{}.parameters.manifest at '{}' could not be parsed: {err}",
        path,
        node.id,
        resolved.display()
      ));
      return;
    }
  };
  let known: Vec<&str> = manifest
    .plugin
    .nodes
    .iter()
    .map(|spec| spec.node_type.as_str())
    .collect();
  if !known.contains(&node_type) {
    report.issues.push(format!(
      "{}.{}.parameters.node_type '{node_type}' is not declared by plugin '{}'. Known node types: [{}]",
      path,
      node.id,
      manifest.plugin.name,
      known.join(", "),
    ));
  }
}

fn specs_for_node_type(node_type: &str) -> Option<Vec<ParamSpec>> {
  match node_type {
    "llm" => Some(vec![
      ParamSpec::required_input("prompt", ParamType::String),
      ParamSpec::optional("model", ParamType::String),
      ParamSpec::optional("system", ParamType::String),
      ParamSpec::optional("temperature", ParamType::Number),
      ParamSpec::optional("max_tokens", ParamType::Integer),
    ]),
    "skill_agent" | "agent" => Some(vec![
      ParamSpec::required_input("skill", ParamType::String),
      ParamSpec::required_input("message", ParamType::String),
      ParamSpec::optional("model", ParamType::String),
    ]),
    "multi_agent" => Some(vec![
      ParamSpec::required("mode", ParamType::String),
      ParamSpec::required_input("message", ParamType::String),
      ParamSpec::optional("model", ParamType::String),
      // Mode-specific shapes are validated when MultiAgentConfig is parsed
      // by the factory; we accept the structured fields as Any here so the
      // schema gate doesn't reject valid YAML it doesn't fully understand.
      ParamSpec::optional("agents", ParamType::Any),
      ParamSpec::optional("participants", ParamType::Any),
      ParamSpec::optional("judge", ParamType::Any),
      ParamSpec::optional("initial_agent", ParamType::String),
      ParamSpec::optional("max_handoffs", ParamType::Integer),
      ParamSpec::optional("schedule", ParamType::Any),
      ParamSpec::optional("stop_when", ParamType::Any),
      ParamSpec::optional("answer_from", ParamType::String),
      ParamSpec::optional("rounds", ParamType::Integer),
      ParamSpec::optional("judge_prompt", ParamType::String),
    ]),
    "http" => Some(vec![
      ParamSpec::required_input("url", ParamType::String),
      ParamSpec::optional("method", ParamType::String),
      ParamSpec::optional("headers", ParamType::Object),
      ParamSpec::optional("body", ParamType::String),
    ]),
    "file" => Some(vec![
      ParamSpec::required_input("operation", ParamType::String),
      ParamSpec::required_input("path", ParamType::String),
      ParamSpec::optional("content", ParamType::String),
    ]),
    "template" => Some(vec![
      ParamSpec::required("template", ParamType::String),
      ParamSpec::optional("output_key", ParamType::String),
      ParamSpec::optional("output_format", ParamType::String),
    ]),
    "arxiv" => Some(vec![
      ParamSpec::required("url", ParamType::String),
      ParamSpec::optional("fetch_source", ParamType::Bool),
      ParamSpec::optional("simplify_latex", ParamType::Bool),
    ]),
    "asr" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required_input("audio_source", ParamType::String),
    ]),
    "image_edit" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required_input("prompt", ParamType::String),
      ParamSpec::required_input("image_source", ParamType::String),
    ]),
    "image_to_image" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required_input("prompt", ParamType::String),
      ParamSpec::required_input("source_image", ParamType::String),
    ]),
    "image_understand" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required_input("text_prompt", ParamType::String),
      ParamSpec::required_input("image_source", ParamType::String),
    ]),
    "markmap" => Some(vec![
      ParamSpec::optional("markdown", ParamType::String),
      ParamSpec::optional("save_to_file", ParamType::String),
    ]),
    "text_to_image" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required_input("prompt", ParamType::String),
    ]),
    "tts" => Some(vec![
      ParamSpec::required("model", ParamType::String),
      ParamSpec::required("voice", ParamType::String),
      ParamSpec::required_input("input_template", ParamType::String),
    ]),
    "while" => Some(vec![
      ParamSpec::required("condition", ParamType::String),
      ParamSpec::required("max_iterations", ParamType::Integer),
      ParamSpec::required("do", ParamType::Sequence),
    ]),
    "map" => Some(vec![
      ParamSpec::required("template", ParamType::Sequence),
      ParamSpec::optional("parallel", ParamType::Bool),
      // F-A6-1: optional `max_concurrent: N` (only meaningful when
      // `parallel: true`). Bounds the number of simultaneously-running
      // sub-flows so provider rate limits aren't trivially blown.
      ParamSpec::optional("max_concurrent", ParamType::Integer),
      // F-A6-2: `input_list` is the canonical map input but the
      // factory was reading it as generic `initial_inputs`, leaving
      // schema validate emitting a false-positive warning. Declare
      // it here so `agentflow workflow validate` stays quiet.
      ParamSpec::optional("input_list", ParamType::Sequence),
    ]),
    "mcp" if cfg!(feature = "mcp") => Some(vec![
      ParamSpec::required("server_command", ParamType::SequenceOfStrings),
      ParamSpec::required("tool_name", ParamType::String),
      ParamSpec::optional("tool_params", ParamType::Object),
      ParamSpec::optional("timeout_ms", ParamType::Integer),
      ParamSpec::optional("max_retries", ParamType::Integer),
    ]),
    "plugin" if cfg!(feature = "plugin") => Some(vec![
      ParamSpec::required("manifest", ParamType::String),
      ParamSpec::required("node_type", ParamType::String),
    ]),
    "rag" if cfg!(feature = "rag") => Some(vec![
      ParamSpec::required("operation", ParamType::String),
      ParamSpec::required("collection", ParamType::String),
      ParamSpec::optional("qdrant_url", ParamType::String),
      ParamSpec::optional("embedding_model", ParamType::String),
      ParamSpec::optional("query", ParamType::String),
      ParamSpec::optional("documents", ParamType::Any),
      ParamSpec::optional("top_k", ParamType::Integer),
      ParamSpec::optional("search_type", ParamType::String),
      ParamSpec::optional("alpha", ParamType::Number),
      ParamSpec::optional("rerank", ParamType::Bool),
      ParamSpec::optional("lambda", ParamType::Number),
      ParamSpec::optional("vector_size", ParamType::Integer),
      ParamSpec::optional("distance", ParamType::String),
    ]),
    _ => None,
  }
}

fn validate_param_type(
  path: &str,
  name: &str,
  value: &serde_yaml::Value,
  kind: ParamType,
  report: &mut WorkflowValidationReport,
) {
  let valid = match kind {
    ParamType::Any => true,
    ParamType::String => value.as_str().is_some(),
    ParamType::Number => {
      value.as_f64().is_some() || value.as_i64().is_some() || value.as_u64().is_some()
    }
    ParamType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
    ParamType::Bool => value.as_bool().is_some(),
    ParamType::Object => matches!(value, serde_yaml::Value::Mapping(_)),
    ParamType::Sequence => matches!(value, serde_yaml::Value::Sequence(_)),
    ParamType::SequenceOfStrings => match value {
      serde_yaml::Value::Sequence(items) => items.iter().all(|item| item.as_str().is_some()),
      _ => false,
    },
  };

  if !valid {
    report.issues.push(format!(
      "{}.parameters.{} must be {}",
      path,
      name,
      describe_param_type(kind)
    ));
  }
}

fn validate_nested_nodes(
  node: &NodeDefinitionV2,
  path: &str,
  key: &str,
  options: WorkflowValidationOptions,
  report: &mut WorkflowValidationReport,
) {
  let Some(value) = node.parameters.get(key) else {
    return;
  };
  let Ok(nodes) = serde_yaml::from_value::<Vec<NodeDefinitionV2>>(value.clone()) else {
    report.issues.push(format!(
      "{}.parameters.{} must be a list of workflow node definitions",
      path, key
    ));
    return;
  };

  for (idx, nested) in nodes.iter().enumerate() {
    validate_node_schema(
      nested,
      &format!("{}.parameters.{}[{}]", path, key, idx),
      options,
      report,
    );
  }
}

fn parse_mapping_source_node(mapping: &str) -> Option<&str> {
  let path = mapping
    .trim()
    .trim_start_matches("{{")
    .trim_end_matches("}}")
    .trim();
  let parts: Vec<_> = path.split('.').collect();
  if parts.len() == 4 && parts[0] == "nodes" && parts[2] == "outputs" {
    Some(parts[1])
  } else {
    None
  }
}

fn feature_hint(node_type: &str) -> &'static str {
  match node_type {
    "mcp" => " (enable the `mcp` feature for MCP workflow nodes)",
    "rag" => " (enable the `rag` feature for RAG workflow nodes)",
    "plugin" => " (enable the `plugin` feature for plugin workflow nodes)",
    _ => "",
  }
}

fn describe_param_type(kind: ParamType) -> &'static str {
  match kind {
    ParamType::Any => "any value",
    ParamType::String => "a string",
    ParamType::Number => "a number",
    ParamType::Integer => "an integer",
    ParamType::Bool => "a boolean",
    ParamType::Object => "an object/map",
    ParamType::Sequence => "a sequence/list",
    ParamType::SequenceOfStrings => "a sequence/list of strings",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse_workflow(yaml: &str) -> FlowDefinitionV2 {
    serde_yaml::from_str(yaml).unwrap()
  }

  #[test]
  fn validates_representative_config_first_node_schemas() {
    let flow = parse_workflow(
      r#"
name: Representative Nodes
nodes:
  - id: answer
    type: llm
    parameters:
      prompt: "Say hello"
      temperature: 0.2
      max_tokens: 64
  - id: render
    type: template
    parameters:
      template: "Hello {{ topic }}"
  - id: read_file
    type: file
    parameters:
      operation: read
      path: /tmp/input.txt
  - id: request
    type: http
    parameters:
      url: "https://example.test"
      method: POST
      headers:
        accept: application/json
  - id: review
    type: skill_agent
    parameters:
      skill: ./skills/review
      message: "Review this"
  - id: paper
    type: arxiv
    parameters:
      url: "https://arxiv.org/abs/2401.00001"
  - id: image
    type: text_to_image
    parameters:
      model: mock-image
      prompt: "Diagram"
  - id: speak
    type: tts
    parameters:
      model: mock-tts
      voice: alloy
      input_template: "Hello"
  - id: each_item
    type: map
    parameters:
      parallel: false
      template:
        - id: map_render
          type: template
          parameters:
            template: "{{ item }}"
  - id: retry_loop
    type: while
    parameters:
      condition: "{{ iteration < 2 }}"
      max_iterations: 2
      do:
        - id: loop_render
          type: template
          parameters:
            template: "{{ iteration }}"
"#,
    );

    let report = validate_flow_definition(&flow);

    assert_eq!(report.issues, Vec::<String>::new());
    assert_eq!(report.warnings, Vec::<String>::new());
  }

  #[test]
  fn input_mapping_can_satisfy_required_input_parameters() {
    let flow = parse_workflow(
      r#"
name: Required Inputs
nodes:
  - id: render
    type: template
    parameters:
      template: "Hello"
  - id: answer
    type: llm
    dependencies: [render]
    input_mapping:
      prompt: "{{ nodes.render.outputs.output }}"
    parameters:
      model: mock
"#,
    );

    let report = validate_flow_definition(&flow);

    assert_eq!(report.issues, Vec::<String>::new());
  }

  #[test]
  fn reports_parameter_type_mismatches_with_paths() {
    let flow = parse_workflow(
      r#"
name: Type Errors
nodes:
  - id: request
    type: http
    parameters:
      url: "https://example.test"
      headers: "not a map"
  - id: speak
    type: tts
    parameters:
      model: mock
      voice: alloy
      input_template: ["not", "a", "string"]
"#,
    );

    let report = validate_flow_definition(&flow);

    assert_eq!(report.issues.len(), 2);
    assert!(
      report
        .issues
        .iter()
        .any(|issue| issue.contains("nodes[0].parameters.headers must be an object/map"))
    );
    assert!(
      report
        .issues
        .iter()
        .any(|issue| issue.contains("nodes[1].parameters.input_template must be a string"))
    );
  }

  #[test]
  fn strict_validation_compiles_condition_expressions() {
    let flow = parse_workflow(
      r#"
name: Bad Condition
nodes:
  - id: answer
    type: llm
    run_if: "lenn(nodes.search.outputs.items) > 0"
    parameters:
      prompt: "Say hello"
"#,
    );

    let report = validate_flow_definition_with_options(
      &flow,
      WorkflowValidationOptions {
        unknown_parameters: UnknownParameterMode::Error,
      },
    );

    assert!(
      report
        .issues
        .iter()
        .any(|issue| issue.contains("unknown function 'lenn'"))
    );
  }
}
