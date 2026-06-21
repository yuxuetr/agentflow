//! Dynamic workflow — compile a declarative plan into an executable `Flow`.
//!
//! This is the productized form of `examples/dynamic_workflow_spike.rs` (P-A4.4).
//! Instead of an agent executing a plan step-by-step in its loop, it emits a
//! [`WorkflowPlan`] — the kind of JSON an LLM produces — which
//! [`compile_plan_to_flow`] turns into an `agentflow-graph` [`Flow`] of tool
//! calls. The core executor then runs it deterministically, inheriting
//! retry / checkpoint / timeout / tracing / replay, and — under
//! `FlowExecutionMode::Concurrent` — the parallelism the plan's `depends_on`
//! edges express.
//!
//! This is the "collapse many runtime LLM decisions into one up-front,
//! deterministically-executed artifact" pattern from the four-paradigm model
//! (`docs/ARCHITECTURE.md`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::{AgentFlowError, FlowExecutionConfig, FlowExt, FlowValue};
use agentflow_llm::{AgentFlow, MultimodalMessage};
use agentflow_tools::ToolRegistry;
use serde::Deserialize;
use serde_json::{Map, Value};

/// One step of a declarative workflow plan — the shape an LLM emits as JSON:
///
/// ```json
/// { "id": "summarize", "tool": "llm", "params": {"prompt": "..."},
///   "depends_on": ["fetch"] }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowPlanStep {
  /// Unique step id (also the graph node id).
  pub id: String,
  /// The registered tool to invoke.
  pub tool: String,
  /// Static parameters for the tool call. Merged with dependency outputs (each
  /// keyed by the dependency's step id) before the call.
  #[serde(default)]
  pub params: Value,
  /// Ids of steps this one depends on; their `result` outputs feed this step,
  /// and the dependency edges drive ordering + parallelism.
  #[serde(default)]
  pub depends_on: Vec<String>,
}

/// A declarative workflow plan: a DAG of tool calls. Steps with no dependency
/// path between them run concurrently under `FlowExecutionMode::Concurrent`.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowPlan {
  /// The steps, in any order (dependencies are resolved by id, not position).
  pub steps: Vec<WorkflowPlanStep>,
}

/// A node that invokes a registered tool. Its static `params` are merged with
/// the outputs of its dependencies (each keyed by the dependency's step id)
/// before the call; the tool's textual `content` is emitted as `result`.
struct ToolCallNode {
  /// Step id, for diagnostics.
  id: String,
  registry: Arc<ToolRegistry>,
  tool: String,
  params: Value,
}

/// Convert a [`FlowValue`] to the raw JSON a tool expects. `FlowValue`'s own
/// `Serialize` is *tagged* (`{"type":"json",...}`), which is not what a tool
/// call wants, so unwrap to the inner value explicitly.
fn flow_value_to_raw_json(value: &FlowValue) -> Value {
  match value {
    FlowValue::Json(json) => json.clone(),
    FlowValue::File { path, .. } => Value::String(path.display().to_string()),
    FlowValue::Url { url, .. } => Value::String(url.clone()),
  }
}

#[async_trait::async_trait]
impl AsyncNode for ToolCallNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    // Start from the step's static params (must be a JSON object, or empty).
    let mut merged: Map<String, Value> = match &self.params {
      Value::Object(map) => map.clone(),
      Value::Null => Map::new(),
      other => {
        let mut map = Map::new();
        map.insert("params".to_string(), other.clone());
        map
      }
    };
    // Layer in dependency outputs, keyed by dependency step id.
    for (key, flow_value) in inputs {
      merged.insert(key.clone(), flow_value_to_raw_json(flow_value));
    }

    let output = self
      .registry
      .execute(&self.tool, Value::Object(merged))
      .await
      .map_err(|err| AgentFlowError::NodeExecutionFailed {
        message: format!("step '{}' tool '{}' failed: {err}", self.id, self.tool),
      })?;

    if output.is_error {
      return Err(AgentFlowError::NodeExecutionFailed {
        message: format!(
          "step '{}' tool '{}' reported an error: {}",
          self.id, self.tool, output.content
        ),
      });
    }

    let mut result = HashMap::new();
    result.insert(
      "result".to_string(),
      FlowValue::Json(Value::String(output.content)),
    );
    Ok(result)
  }
}

/// Compile a [`WorkflowPlan`] into an executable [`Flow`].
///
/// Each step becomes a graph node that invokes its tool; `depends_on` becomes
/// graph dependencies (so independent steps run concurrently and dependents wait
/// for their inputs). Each dependency's `result` output is wired into the
/// dependent node's input pool keyed by the dependency's step id.
///
/// Run the result via `agentflow_core::FlowExt` — typically configured with
/// `FlowExecutionMode::Concurrent` to realize the plan's parallelism.
///
/// Validation is up front: duplicate step ids and dangling dependency
/// references are rejected as `FlowDefinitionError` (cycles are caught by the
/// executor's topological sort at run time).
pub fn compile_plan_to_flow(
  plan: &WorkflowPlan,
  registry: Arc<ToolRegistry>,
) -> Result<Flow, AgentFlowError> {
  let ids: HashSet<&str> = plan.steps.iter().map(|step| step.id.as_str()).collect();
  if ids.len() != plan.steps.len() {
    return Err(AgentFlowError::FlowDefinitionError {
      message: "workflow plan has duplicate step ids".to_string(),
    });
  }
  for step in &plan.steps {
    for dependency in &step.depends_on {
      if !ids.contains(dependency.as_str()) {
        return Err(AgentFlowError::FlowDefinitionError {
          message: format!(
            "step '{}' depends on unknown step '{}'",
            step.id, dependency
          ),
        });
      }
    }
  }

  let nodes = plan
    .steps
    .iter()
    .map(|step| {
      // Wire each dependency's `result` output into this node's input pool,
      // keyed by the dependency's step id.
      let input_mapping = if step.depends_on.is_empty() {
        None
      } else {
        Some(
          step
            .depends_on
            .iter()
            .map(|dependency| {
              (
                dependency.clone(),
                (dependency.clone(), "result".to_string()),
              )
            })
            .collect(),
        )
      };
      GraphNode {
        id: step.id.clone(),
        node_type: NodeType::Standard(Arc::new(ToolCallNode {
          id: step.id.clone(),
          registry: Arc::clone(&registry),
          tool: step.tool.clone(),
          params: step.params.clone(),
        })),
        dependencies: step.depends_on.clone(),
        input_mapping,
        run_if: None,
        initial_inputs: HashMap::new(),
      }
    })
    .collect();

  Ok(Flow::new(nodes))
}

/// Errors from the LLM-driven [`DynamicWorkflowAgent`].
#[derive(Debug, thiserror::Error)]
pub enum DynamicWorkflowError {
  /// The LLM planning call failed.
  #[error("LLM planning call failed: {0}")]
  Llm(String),
  /// The LLM's reply could not be parsed as a `WorkflowPlan`.
  #[error("could not parse the LLM plan as JSON: {0}")]
  PlanParse(String),
  /// Compiling or executing the plan failed.
  #[error(transparent)]
  Flow(#[from] AgentFlowError),
}

/// An agent that **generates** a workflow plan with an LLM, compiles it to a
/// `Flow`, and executes it — the full dynamic-workflow paradigm (P-A4.4).
///
/// This is the binding-time inversion the four-paradigm model describes: instead
/// of an agent deciding every step in a loop, it makes *one* up-front planning
/// call, then the engine runs the resulting DAG deterministically (and in
/// parallel where the plan allows).
pub struct DynamicWorkflowAgent {
  model: String,
  tools: Arc<ToolRegistry>,
}

/// Pull the JSON object out of an LLM reply (it may wrap it in prose or a
/// ```json fence): take the span from the first `{` to the last `}`.
fn extract_json(text: &str) -> &str {
  match (text.find('{'), text.rfind('}')) {
    (Some(start), Some(end)) if end >= start => &text[start..=end],
    _ => text,
  }
}

impl DynamicWorkflowAgent {
  /// Build an agent that plans with `model` and executes against `tools`.
  pub fn new(model: impl Into<String>, tools: Arc<ToolRegistry>) -> Self {
    Self {
      model: model.into(),
      tools,
    }
  }

  /// Ask the LLM for a [`WorkflowPlan`] for `goal`, given the available tools.
  pub async fn plan(&self, goal: &str) -> Result<WorkflowPlan, DynamicWorkflowError> {
    let tools_desc: String = self
      .tools
      .list()
      .iter()
      .map(|tool| format!("- {}: {}", tool.name(), tool.description()))
      .collect::<Vec<_>>()
      .join("\n");
    let system = "You are a workflow planner. Given a goal and a list of tools, \
      respond with ONLY a JSON object of the form \
      {\"steps\":[{\"id\":\"...\",\"tool\":\"...\",\"params\":{...},\"depends_on\":[\"...\"]}]}. \
      Steps with no dependency between them run in parallel; use depends_on to \
      order dependent steps. Use only the listed tools.";
    let user = format!("Goal: {goal}\n\nAvailable tools:\n{tools_desc}");

    let response = AgentFlow::model(&self.model)
      .multimodal_messages(vec![
        MultimodalMessage::text("system", system),
        MultimodalMessage::text("user", user),
      ])
      .execute_full()
      .await
      .map_err(|err| DynamicWorkflowError::Llm(err.to_string()))?;

    serde_json::from_str(extract_json(&response.content))
      .map_err(|err| DynamicWorkflowError::PlanParse(err.to_string()))
  }

  /// Plan with the LLM, compile to a `Flow`, and execute it concurrently.
  /// Returns the executed state pool (node id → result).
  pub async fn run(
    &self,
    goal: &str,
  ) -> Result<HashMap<String, AsyncNodeResult>, DynamicWorkflowError> {
    let plan = self.plan(goal).await?;
    let flow = compile_plan_to_flow(&plan, Arc::clone(&self.tools))?;
    Ok(
      flow
        .execute_from_inputs_with_config(HashMap::new(), FlowExecutionConfig::concurrent(8))
        .await?,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::{FlowExecutionConfig, FlowExt};
  use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput};
  use serde_json::json;

  /// Echoes its received params back as JSON `content` — lets a test trace
  /// exactly what reached each tool call (static params + dependency outputs).
  struct EchoTool;

  #[async_trait::async_trait]
  impl Tool for EchoTool {
    fn name(&self) -> &str {
      "echo"
    }
    fn description(&self) -> &str {
      "echoes its params as JSON"
    }
    fn parameters_schema(&self) -> Value {
      json!({ "type": "object" })
    }
    fn metadata(&self) -> ToolMetadata {
      ToolMetadata::builtin_named("echo")
    }
    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
      Ok(ToolOutput::success(params.to_string()))
    }
  }

  fn registry_with_echo() -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    Arc::new(registry)
  }

  fn result_text(state: &HashMap<String, AsyncNodeResult>, node: &str) -> String {
    match state
      .get(node)
      .and_then(|r| r.as_ref().ok())
      .and_then(|o| o.get("result"))
    {
      Some(FlowValue::Json(Value::String(s))) => s.clone(),
      other => panic!("node '{node}' produced unexpected result: {other:?}"),
    }
  }

  #[tokio::test]
  async fn diamond_plan_runs_with_dependencies_wired() {
    // a, b run independently (concurrently); c depends on both and sees their
    // outputs keyed by step id.
    let plan = WorkflowPlan {
      steps: vec![
        WorkflowPlanStep {
          id: "a".into(),
          tool: "echo".into(),
          params: json!({"v": "A"}),
          depends_on: vec![],
        },
        WorkflowPlanStep {
          id: "b".into(),
          tool: "echo".into(),
          params: json!({"v": "B"}),
          depends_on: vec![],
        },
        WorkflowPlanStep {
          id: "c".into(),
          tool: "echo".into(),
          params: json!({}),
          depends_on: vec!["a".into(), "b".into()],
        },
      ],
    };
    let flow = compile_plan_to_flow(&plan, registry_with_echo()).expect("compile");

    let state = flow
      .execute_from_inputs_with_config(HashMap::new(), FlowExecutionConfig::concurrent(8))
      .await
      .expect("run");

    // a and b echoed their own static params.
    assert!(result_text(&state, "a").contains("\"v\":\"A\""));
    assert!(result_text(&state, "b").contains("\"v\":\"B\""));
    // c received BOTH dependency results, keyed by step id.
    let c = result_text(&state, "c");
    assert!(c.contains("\"a\":"), "c should see a's output: {c}");
    assert!(c.contains("\"b\":"), "c should see b's output: {c}");
    assert!(
      c.contains('A') && c.contains('B'),
      "c should carry both payloads: {c}"
    );
  }

  #[test]
  fn rejects_dangling_dependency() {
    let plan = WorkflowPlan {
      steps: vec![WorkflowPlanStep {
        id: "x".into(),
        tool: "echo".into(),
        params: json!({}),
        depends_on: vec!["missing".into()],
      }],
    };
    let result = compile_plan_to_flow(&plan, registry_with_echo());
    assert!(matches!(
      result,
      Err(AgentFlowError::FlowDefinitionError { .. })
    ));
  }

  #[test]
  fn rejects_duplicate_ids() {
    let step = WorkflowPlanStep {
      id: "dup".into(),
      tool: "echo".into(),
      params: json!({}),
      depends_on: vec![],
    };
    let plan = WorkflowPlan {
      steps: vec![step.clone(), step],
    };
    let result = compile_plan_to_flow(&plan, registry_with_echo());
    assert!(matches!(
      result,
      Err(AgentFlowError::FlowDefinitionError { .. })
    ));
  }

  /// Point a mock text model at `response` and load it as the active config.
  /// Serialized by `crate::LLM_TEST_LOCK` (mutates process-wide LLM config/env).
  async fn init_mock_model(model: &str, response: &str) {
    // SAFETY: callers hold LLM_TEST_LOCK while mutating these process-wide vars.
    unsafe {
      std::env::set_var("AGENTFLOW_MOCK_RESPONSE", response);
    }
    let config_path =
      std::env::temp_dir().join(format!("agentflow-dyn-wf-{}.yml", uuid::Uuid::new_v4()));
    std::fs::write(
      &config_path,
      format!(
        "models:\n  {model}:\n    vendor: mock\n    type: text\n    model_id: {model}\n\
         providers:\n  mock:\n    api_key_env: MOCK_API_KEY\n"
      ),
    )
    .expect("write mock config");
    agentflow_llm::AgentFlow::init_with_config(config_path.to_str().expect("utf8 path"))
      .await
      .expect("init mock model");
  }

  #[tokio::test]
  async fn llm_plans_then_engine_executes_in_parallel() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    // The "LLM" returns a parallel plan: a and b are independent, c joins them.
    init_mock_model(
      "mock-dyn-wf",
      r#"{"steps":[
        {"id":"a","tool":"echo","params":{"v":"A"}},
        {"id":"b","tool":"echo","params":{"v":"B"}},
        {"id":"c","tool":"echo","depends_on":["a","b"]}
      ]}"#,
    )
    .await;

    let agent = DynamicWorkflowAgent::new("mock-dyn-wf", registry_with_echo());
    let state = agent.run("combine A and B").await.expect("run");

    assert!(result_text(&state, "a").contains('A'));
    assert!(result_text(&state, "b").contains('B'));
    let c = result_text(&state, "c");
    assert!(
      c.contains("\"a\":") && c.contains("\"b\":"),
      "c should receive both planned dependencies: {c}"
    );
  }
}
