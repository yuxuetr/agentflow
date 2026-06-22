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

use agentflow_graph::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_graph::flow::{Flow, GraphNode, NodeType};
use agentflow_graph::{AgentFlowError, FlowRunner, FlowValue};
use agentflow_llm::{AgentFlow, MultimodalMessage};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::nodes::AgentNode;
use crate::react::{ReActAgent, ReActConfig};

/// One step of a declarative workflow plan — the shape an LLM emits as JSON:
///
/// ```json
/// { "id": "summarize", "tool": "llm", "params": {"prompt": "..."},
///   "depends_on": ["fetch"] }
/// ```
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepKind {
  /// Invoke a registered tool (the default). Uses `tool` + `params`; emits its
  /// output as `result`.
  #[default]
  Tool,
  /// Run an embedded ReAct agent step (P-A2.2). Uses `params.model` (required),
  /// `params.persona` (optional), and `params.prompt` (the agent's `message`,
  /// required); emits the agent's answer as `response`. The agent shares the
  /// plan's tool registry, so it inherits the same sandbox / approval governance.
  Agent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowPlanStep {
  /// Unique step id (also the graph node id).
  pub id: String,
  /// What this step is: a tool call (default) or an embedded agent.
  #[serde(default)]
  pub kind: PlanStepKind,
  /// The registered tool to invoke (required for `kind = "tool"`; ignored for
  /// `kind = "agent"`).
  #[serde(default)]
  pub tool: String,
  /// Static parameters. For a tool step, merged with dependency outputs (each
  /// keyed by the dependency's step id) before the call. For an agent step,
  /// carries `model` / `persona` / `prompt`.
  #[serde(default)]
  pub params: Value,
  /// Ids of steps this one depends on; their output feeds this step (keyed by
  /// the dependency's step id), and the edges drive ordering + parallelism.
  #[serde(default)]
  pub depends_on: Vec<String>,
}

impl WorkflowPlanStep {
  /// The state-pool key this step emits its primary output under: tool steps
  /// emit `result`, agent steps emit `response`. Used to wire a dependent
  /// step's `input_mapping` to the correct key.
  fn output_key(&self) -> &'static str {
    match self.kind {
      PlanStepKind::Tool => "result",
      PlanStepKind::Agent => "response",
    }
  }
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
    // Per-kind required fields.
    match step.kind {
      PlanStepKind::Tool => {
        if step.tool.trim().is_empty() {
          return Err(AgentFlowError::FlowDefinitionError {
            message: format!("tool step '{}' must name a tool", step.id),
          });
        }
      }
      PlanStepKind::Agent => {
        if step.params.get("model").and_then(Value::as_str).is_none() {
          return Err(AgentFlowError::FlowDefinitionError {
            message: format!("agent step '{}' requires params.model (a string)", step.id),
          });
        }
        if step.params.get("prompt").and_then(Value::as_str).is_none() {
          return Err(AgentFlowError::FlowDefinitionError {
            message: format!("agent step '{}' requires params.prompt (a string)", step.id),
          });
        }
      }
    }
  }

  // Each step's primary output key, so a dependent step wires to the right one
  // (`result` for a tool step, `response` for an agent step).
  let output_keys: HashMap<&str, &'static str> = plan
    .steps
    .iter()
    .map(|step| (step.id.as_str(), step.output_key()))
    .collect();

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
              let key = output_keys
                .get(dependency.as_str())
                .copied()
                .unwrap_or("result");
              (dependency.clone(), (dependency.clone(), key.to_string()))
            })
            .collect(),
        )
      };

      let (node_type, initial_inputs): (NodeType, HashMap<String, FlowValue>) = match step.kind {
        PlanStepKind::Tool => (
          NodeType::Standard(Arc::new(ToolCallNode {
            id: step.id.clone(),
            registry: Arc::clone(&registry),
            tool: step.tool.clone(),
            params: step.params.clone(),
          })),
          HashMap::new(),
        ),
        PlanStepKind::Agent => {
          // Validated above: model + prompt are present strings.
          let model = step
            .params
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default();
          let prompt = step
            .params
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
          let mut config = ReActConfig::new(model);
          if let Some(persona) = step.params.get("persona").and_then(Value::as_str) {
            config = config.with_persona(persona);
          }
          let agent = ReActAgent::new(
            config,
            Box::new(SessionMemory::default_window()),
            Arc::clone(&registry),
          );
          let node = AgentNode::from_agent(step.id.clone(), agent);
          // AgentNode reads its `message` input; dependency outputs gate
          // ordering (and are available in the pool) but the message is static.
          let initial = HashMap::from([(
            "message".to_string(),
            FlowValue::Json(Value::String(prompt.to_string())),
          )]);
          (NodeType::Standard(Arc::new(node)), initial)
        }
      };

      GraphNode {
        id: step.id.clone(),
        node_type,
        dependencies: step.depends_on.clone(),
        input_mapping,
        run_if: None,
        initial_inputs,
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
  runner: Arc<dyn FlowRunner>,
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
  /// Build an agent that plans with `model`, executes against `tools`, and
  /// runs the compiled `Flow` via the injected `runner` (the surface passes
  /// `agentflow_core::CoreFlowRunner::concurrent(...)`).
  pub fn new(
    model: impl Into<String>,
    tools: Arc<ToolRegistry>,
    runner: Arc<dyn FlowRunner>,
  ) -> Self {
    Self {
      model: model.into(),
      tools,
      runner,
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
      order dependent steps. Use only the listed tools. For a step that needs \
      open-ended reasoning rather than a single tool call, emit an agent step: \
      {\"id\":\"...\",\"kind\":\"agent\",\"params\":{\"model\":\"<model>\",\"prompt\":\"<instruction>\"},\"depends_on\":[...]} \
      (it runs a sub-agent that may call the same tools). Prefer plain tool steps; use agent steps sparingly.";
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
    Ok(self.runner.run(&flow, HashMap::new()).await?)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::CoreFlowRunner;
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
          kind: PlanStepKind::Tool,
          tool: "echo".into(),
          params: json!({"v": "A"}),
          depends_on: vec![],
        },
        WorkflowPlanStep {
          id: "b".into(),
          kind: PlanStepKind::Tool,
          tool: "echo".into(),
          params: json!({"v": "B"}),
          depends_on: vec![],
        },
        WorkflowPlanStep {
          id: "c".into(),
          kind: PlanStepKind::Tool,
          tool: "echo".into(),
          params: json!({}),
          depends_on: vec!["a".into(), "b".into()],
        },
      ],
    };
    let flow = compile_plan_to_flow(&plan, registry_with_echo()).expect("compile");

    let state = CoreFlowRunner::concurrent(8)
      .run(&flow, HashMap::new())
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
        kind: PlanStepKind::Tool,
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
      kind: PlanStepKind::Tool,
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

    let agent = DynamicWorkflowAgent::new(
      "mock-dyn-wf",
      registry_with_echo(),
      Arc::new(CoreFlowRunner::concurrent(8)),
    );
    let state = agent.run("combine A and B").await.expect("run");

    assert!(result_text(&state, "a").contains('A'));
    assert!(result_text(&state, "b").contains('B'));
    let c = result_text(&state, "c");
    assert!(
      c.contains("\"a\":") && c.contains("\"b\":"),
      "c should receive both planned dependencies: {c}"
    );
  }

  // ── P-A2.2: agent steps in a plan ─────────────────────────────────────────

  fn agent_step(id: &str, params: Value, deps: Vec<&str>) -> WorkflowPlanStep {
    WorkflowPlanStep {
      id: id.into(),
      kind: PlanStepKind::Agent,
      tool: String::new(),
      params,
      depends_on: deps.into_iter().map(String::from).collect(),
    }
  }

  #[test]
  fn agent_step_compiles_to_a_flow_node() {
    let plan = WorkflowPlan {
      steps: vec![agent_step(
        "think",
        json!({"model": "m", "prompt": "hello", "persona": "be terse"}),
        vec![],
      )],
    };
    let flow = compile_plan_to_flow(&plan, registry_with_echo()).expect("compiles");
    assert_eq!(flow.nodes().len(), 1);
  }

  #[test]
  fn agent_step_requires_model_and_prompt() {
    for bad in [json!({"prompt": "hi"}), json!({"model": "m"})] {
      let plan = WorkflowPlan {
        steps: vec![agent_step("a", bad, vec![])],
      };
      assert!(
        matches!(
          compile_plan_to_flow(&plan, registry_with_echo()),
          Err(AgentFlowError::FlowDefinitionError { .. })
        ),
        "agent step missing model or prompt must be rejected"
      );
    }
  }

  #[test]
  fn tool_step_with_empty_tool_is_rejected() {
    let plan = WorkflowPlan {
      steps: vec![WorkflowPlanStep {
        id: "x".into(),
        kind: PlanStepKind::Tool,
        tool: String::new(),
        params: json!({}),
        depends_on: vec![],
      }],
    };
    assert!(matches!(
      compile_plan_to_flow(&plan, registry_with_echo()),
      Err(AgentFlowError::FlowDefinitionError { .. })
    ));
  }

  /// The key correctness check: a tool step depending on an *agent* step must
  /// wire its input_mapping to the agent's `response` output, not `result`.
  #[test]
  fn dependent_step_wires_to_the_agent_response_key() {
    let plan = WorkflowPlan {
      steps: vec![
        agent_step("plan", json!({"model": "m", "prompt": "p"}), vec![]),
        WorkflowPlanStep {
          id: "use".into(),
          kind: PlanStepKind::Tool,
          tool: "echo".into(),
          params: json!({}),
          depends_on: vec!["plan".into()],
        },
      ],
    };
    let flow = compile_plan_to_flow(&plan, registry_with_echo()).expect("compiles");
    let mapping = flow
      .nodes()
      .get("use")
      .expect("use node")
      .input_mapping
      .as_ref()
      .expect("use has an input mapping");
    assert_eq!(
      mapping.get("plan"),
      Some(&("plan".to_string(), "response".to_string())),
      "a dependent of an agent step must read its `response` output"
    );
  }
}
