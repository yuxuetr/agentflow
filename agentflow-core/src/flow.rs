use crate::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  checkpoint::{Checkpoint, CheckpointConfig, CheckpointManager, WorkflowStatus},
  error::AgentFlowError,
  events::{EventListener, WorkflowEvent},
  value::FlowValue,
};
use dirs;
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone)]
pub enum NodeType {
  Standard(Arc<dyn AsyncNode>),
  Map {
    template: Vec<GraphNode>,
    parallel: bool,
  },
  While {
    condition: String,
    max_iterations: u32,
    template: Vec<GraphNode>,
  },
}

#[derive(Clone)]
pub struct GraphNode {
  pub id: String,
  pub node_type: NodeType,
  pub dependencies: Vec<String>,
  pub input_mapping: Option<HashMap<String, (String, String)>>,
  pub run_if: Option<String>,
  pub initial_inputs: HashMap<String, FlowValue>,
}

#[derive(Default, Clone)]
pub struct Flow {
  nodes: HashMap<String, GraphNode>,
  checkpoint_enabled: bool,
  checkpoint_manager: Option<Arc<CheckpointManager>>,
  event_listener: Option<Arc<dyn EventListener>>,
}

impl Flow {
  pub fn new(nodes: Vec<GraphNode>) -> Self {
    let nodes_map = nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
    Self {
      nodes: nodes_map,
      checkpoint_enabled: false,
      checkpoint_manager: None,
      event_listener: None,
    }
  }

  /// Enable checkpointing with custom configuration
  pub fn with_checkpointing(mut self, config: CheckpointConfig) -> Result<Self, AgentFlowError> {
    let manager = CheckpointManager::new(config)?;
    self.checkpoint_enabled = true;
    self.checkpoint_manager = Some(Arc::new(manager));
    Ok(self)
  }

  /// Enable checkpointing with default configuration
  pub fn with_default_checkpointing(self) -> Result<Self, AgentFlowError> {
    self.with_checkpointing(CheckpointConfig::default())
  }

  pub fn add_node(&mut self, node: GraphNode) {
    self.nodes.insert(node.id.clone(), node);
  }

  /// Return the workflow execution order after dependency validation.
  ///
  /// This is useful for dry-run planning, debugging, and scheduler benchmarks.
  /// It does not execute nodes or create run artifacts.
  pub fn execution_order(&self) -> Result<Vec<String>, AgentFlowError> {
    self.topological_sort()
  }

  /// Attach a workflow event listener for tracing, metrics, or logs.
  pub fn with_event_listener(mut self, listener: Arc<dyn EventListener>) -> Self {
    self.event_listener = Some(listener);
    self
  }

  pub async fn run(&self) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    self.execute_from_inputs(HashMap::new()).await
  }

  /// Resume workflow from the latest checkpoint for a given workflow ID
  pub async fn resume(
    &self,
    workflow_id: &str,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    if !self.checkpoint_enabled {
      return Err(AgentFlowError::ConfigurationError {
        message: "Checkpointing is not enabled. Call with_checkpointing() first.".to_string(),
      });
    }

    let manager =
      self
        .checkpoint_manager
        .as_ref()
        .ok_or_else(|| AgentFlowError::ConfigurationError {
          message: "Checkpoint manager not initialized despite checkpointing being enabled"
            .to_string(),
        })?;
    let checkpoint = manager
      .load_latest_checkpoint(workflow_id)
      .await?
      .ok_or_else(|| AgentFlowError::ConfigurationError {
        message: format!("No checkpoint found for workflow '{}'", workflow_id),
      })?;

    println!(
      "📥 Resuming workflow '{}' from checkpoint at node '{}'",
      workflow_id, checkpoint.last_completed_node
    );
    self.execute_from_checkpoint(workflow_id, checkpoint).await
  }

  pub async fn execute_from_inputs(
    &self,
    initial_inputs: AsyncNodeInputs,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    self
      .execute_with_workflow_id(None, initial_inputs, None, None, None)
      .await
  }

  /// Execute workflow with optional checkpoint recovery
  async fn execute_with_workflow_id(
    &self,
    workflow_id: Option<String>,
    initial_inputs: AsyncNodeInputs,
    skip_until: Option<String>,
    restored_state_pool: Option<HashMap<String, AsyncNodeResult>>,
    restored_last_completed_node: Option<String>,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    let run_id = workflow_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let workflow_started_at = Instant::now();
    self.emit_event(WorkflowEvent::WorkflowStarted {
      workflow_id: run_id.clone(),
      timestamp: workflow_started_at,
    });
    let base_dir = dirs::home_dir()
      .ok_or_else(|| AgentFlowError::ConfigurationError {
        message: "Could not find home directory".to_string(),
      })?
      .join(".agentflow")
      .join("runs");
    let run_dir = base_dir.join(&run_id);
    fs::create_dir_all(&run_dir).map_err(|e| AgentFlowError::PersistenceError {
      message: e.to_string(),
    })?;

    let sorted_nodes = self.topological_sort()?;
    let mut state_pool: HashMap<String, AsyncNodeResult> = restored_state_pool.unwrap_or_default();
    let mut last_completed_node = restored_last_completed_node.or_else(|| {
      state_pool
        .iter()
        .filter_map(|(node_id, result)| result.as_ref().ok().map(|_| node_id.as_str()))
        .filter_map(|node_id| {
          sorted_nodes
            .iter()
            .position(|sorted_node| sorted_node == node_id)
            .map(|idx| (idx, node_id.to_string()))
        })
        .max_by_key(|(idx, _)| *idx)
        .map(|(_, node_id)| node_id)
    });

    // Flag to skip nodes until we reach the checkpoint resume point
    let mut should_skip = skip_until.is_some();

    for node_id in &sorted_nodes {
      // Check if we should resume from this node
      if should_skip {
        if let Some(ref resume_node) = skip_until {
          if node_id == resume_node {
            should_skip = false;
            println!("▶️  Resuming execution from node '{}'", node_id);
          } else {
            println!(
              "⏭️  Skipping node '{}' (already completed in checkpoint)",
              node_id
            );
            // For skipped nodes, we don't execute but mark them as complete
            // Their outputs should be restored from checkpoint
            continue;
          }
        }
      }

      let graph_node =
        self
          .nodes
          .get(node_id)
          .ok_or_else(|| AgentFlowError::FlowDefinitionError {
            message: format!("Node '{}' not found in flow definition", node_id),
          })?;

      let should_run = match &graph_node.run_if {
        Some(condition) => self.evaluate_condition(condition, &state_pool)?,
        None => true,
      };

      if !should_run {
        println!("⏭️  Skipping node '{}' due to condition.", node_id);
        let result = Err(AgentFlowError::NodeSkipped);
        self.persist_step_result(&run_dir, node_id, &result)?;
        state_pool.insert(node_id.to_string(), result);
        continue;
      }

      let mut inputs = match &graph_node.input_mapping {
        Some(mapping) => self.gather_inputs(node_id, mapping, &state_pool)?,
        None => HashMap::new(),
      };

      inputs.extend(graph_node.initial_inputs.clone());

      // Inject initial inputs from execute_from_inputs (for while loops and map nodes)
      // These provide loop variables and context that should be available to all nodes
      inputs.extend(initial_inputs.clone());

      if let Some(Ok(restored_outputs)) = state_pool.get(node_id) {
        inputs.extend(restored_outputs.clone());
      }

      println!("▶️  Executing node '{}'", node_id);
      let node_started_at = Instant::now();
      self.emit_event(WorkflowEvent::NodeStarted {
        workflow_id: run_id.clone(),
        node_id: node_id.clone(),
        timestamp: node_started_at,
      });
      let result = match &graph_node.node_type {
        NodeType::Standard(node) => node.execute(&inputs).await,
        NodeType::Map { template, parallel } => {
          if *parallel {
            self.execute_map_node_parallel(&inputs, template).await
          } else {
            self.execute_map_node_sequential(&inputs, template).await
          }
        }
        NodeType::While {
          condition,
          max_iterations,
          template,
        } => {
          self
            .execute_while_node(&inputs, condition, *max_iterations, template)
            .await
        }
      };

      self.persist_step_result(&run_dir, node_id, &result)?;

      match &result {
        Ok(outputs) => {
          last_completed_node = Some(node_id.clone());
          self.emit_event(WorkflowEvent::NodeOutputCaptured {
            workflow_id: run_id.clone(),
            node_id: node_id.clone(),
            output: Self::outputs_to_json(outputs),
            timestamp: Instant::now(),
          });
          self.emit_event(WorkflowEvent::NodeCompleted {
            workflow_id: run_id.clone(),
            node_id: node_id.clone(),
            duration: node_started_at.elapsed(),
            timestamp: Instant::now(),
          });
        }
        Err(AgentFlowError::NodeSkipped) => {
          self.emit_event(WorkflowEvent::NodeSkipped {
            workflow_id: run_id.clone(),
            node_id: node_id.clone(),
            reason: "run_if evaluated to false".to_string(),
            timestamp: Instant::now(),
          });
        }
        Err(err) => {
          self.emit_event(WorkflowEvent::NodeFailed {
            workflow_id: run_id.clone(),
            node_id: node_id.clone(),
            error: err.to_string(),
            duration: node_started_at.elapsed(),
            timestamp: Instant::now(),
          });
        }
      }

      state_pool.insert(node_id.to_string(), result);

      // Save checkpoint if enabled
      if self.checkpoint_enabled
        && state_pool
          .get(node_id)
          .map(|result| result.is_ok())
          .unwrap_or(false)
      {
        if let Some(ref manager) = self.checkpoint_manager {
          let checkpoint_state = self.state_pool_to_checkpoint_state(&state_pool);
          if let Err(e) = manager
            .save_checkpoint(&run_id, node_id, &checkpoint_state)
            .await
          {
            eprintln!(
              "⚠️  Warning: Failed to save checkpoint after node '{}': {}",
              node_id, e
            );
          } else {
            println!("💾 Checkpoint saved after node '{}'", node_id);
          }
        }
      }
    }

    let workflow_failed = state_pool.values().any(Result::is_err);
    if workflow_failed {
      let error = state_pool
        .values()
        .find_map(|result| result.as_ref().err().map(ToString::to_string))
        .unwrap_or_else(|| "workflow failed".to_string());
      self.emit_event(WorkflowEvent::WorkflowFailed {
        workflow_id: run_id.clone(),
        error,
        duration: workflow_started_at.elapsed(),
        timestamp: Instant::now(),
      });
    } else {
      self.emit_event(WorkflowEvent::WorkflowCompleted {
        workflow_id: run_id.clone(),
        duration: workflow_started_at.elapsed(),
        timestamp: Instant::now(),
      });
    }

    // Mark workflow as completed or failed
    if self.checkpoint_enabled {
      if let Some(ref manager) = self.checkpoint_manager {
        let checkpoint_state = self.state_pool_to_checkpoint_state(&state_pool);
        let status = if state_pool.values().all(|r| r.is_ok()) {
          WorkflowStatus::Completed
        } else {
          WorkflowStatus::Failed
        };
        let final_checkpoint_node = last_completed_node.as_deref().unwrap_or("");

        if let Err(e) = manager
          .save_checkpoint_with_status(&run_id, final_checkpoint_node, &checkpoint_state, status)
          .await
        {
          eprintln!("⚠️  Warning: Failed to save final checkpoint: {}", e);
        }
      }
    }

    Ok(state_pool)
  }

  /// Execute workflow from a checkpoint
  async fn execute_from_checkpoint(
    &self,
    workflow_id: &str,
    checkpoint: Checkpoint,
  ) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
    // Restore state pool from checkpoint
    let state_pool = Self::checkpoint_state_to_state_pool(&checkpoint.state);

    // Find the next node to execute after the checkpoint
    let sorted_nodes = self.topological_sort()?;
    let resume_from = checkpoint.last_completed_node.clone();

    // Find the next node in the execution order
    let mut next_node_idx = None;
    for (idx, node_id) in sorted_nodes.iter().enumerate() {
      if node_id == &resume_from {
        next_node_idx = Some(idx + 1);
        break;
      }
    }

    let next_node = match next_node_idx {
      Some(idx) => sorted_nodes.get(idx).map(|s| s.to_string()),
      None if resume_from.is_empty() => sorted_nodes.first().map(|s| s.to_string()),
      None => None,
    };

    if let Some(next_node_id) = next_node {
      // Continue execution from next node
      self
        .execute_with_workflow_id(
          Some(workflow_id.to_string()),
          HashMap::new(),
          Some(next_node_id),
          Some(state_pool),
          Some(checkpoint.last_completed_node),
        )
        .await
    } else {
      println!("✅ Workflow '{}' was already completed", workflow_id);
      Ok(state_pool)
    }
  }

  /// Convert state pool to checkpoint-compatible format
  fn state_pool_to_checkpoint_state(
    &self,
    state_pool: &HashMap<String, AsyncNodeResult>,
  ) -> HashMap<String, serde_json::Value> {
    state_pool
      .iter()
      .filter_map(|(node_id, result)| {
        if let Some(outputs) = Self::checkpointable_outputs(result) {
          // Convert outputs to JSON value
          let json_outputs: HashMap<String, serde_json::Value> = outputs
            .iter()
            .map(|(k, v)| {
              (
                k.clone(),
                match v {
                  FlowValue::Json(json) => json.clone(),
                  _ => serde_json::json!(null),
                },
              )
            })
            .collect();
          Some((node_id.clone(), serde_json::to_value(json_outputs).ok()?))
        } else {
          None
        }
      })
      .collect()
  }

  fn checkpointable_outputs(result: &AsyncNodeResult) -> Option<&HashMap<String, FlowValue>> {
    match result {
      Ok(outputs) => Some(outputs),
      Err(AgentFlowError::NodePartialExecutionFailed {
        partial_outputs, ..
      }) => Some(partial_outputs),
      Err(_) => None,
    }
  }

  fn outputs_to_json(outputs: &HashMap<String, FlowValue>) -> serde_json::Value {
    let json_outputs: HashMap<String, serde_json::Value> = outputs
      .iter()
      .map(|(key, value)| {
        let json = match value {
          FlowValue::Json(json) => json.clone(),
          FlowValue::File { path, mime_type } => serde_json::json!({
            "$type": "file",
            "path": path,
            "mime_type": mime_type,
          }),
          FlowValue::Url { url, mime_type } => serde_json::json!({
            "$type": "url",
            "url": url,
            "mime_type": mime_type,
          }),
        };
        (key.clone(), json)
      })
      .collect();
    serde_json::to_value(json_outputs).unwrap_or_else(|_| serde_json::json!({}))
  }

  fn emit_event(&self, event: WorkflowEvent) {
    if let Some(listener) = &self.event_listener {
      listener.on_event(&event);
    }
  }

  fn checkpoint_state_to_state_pool(
    checkpoint_state: &HashMap<String, serde_json::Value>,
  ) -> HashMap<String, AsyncNodeResult> {
    checkpoint_state
      .iter()
      .map(|(node_id, value)| {
        let outputs = match value {
          serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| {
              let flow_value = serde_json::from_value::<FlowValue>(value.clone())
                .unwrap_or_else(|_| FlowValue::Json(value.clone()));
              (key.clone(), flow_value)
            })
            .collect(),
          other => vec![("result".to_string(), FlowValue::Json(other.clone()))]
            .into_iter()
            .collect(),
        };
        (node_id.clone(), Ok(outputs))
      })
      .collect()
  }

  fn execute_while_node<'a>(
    &'a self,
    inputs: &'a AsyncNodeInputs,
    condition_template: &'a str,
    max_iterations: u32,
    template: &'a [GraphNode],
  ) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
    Box::pin(async move {
      let mut loop_inputs = inputs.clone();
      let mut iteration_count = 0u32;

      while iteration_count < max_iterations {
        println!(
          "--- While Loop Iteration: {}, State: {:?} ---",
          iteration_count + 1,
          loop_inputs
        );
        let mut resolved_condition = condition_template.to_string();
        for (key, value) in &loop_inputs {
          let placeholder = format!("{{{{{}}}}}", key);
          if resolved_condition.contains(&placeholder) {
            let replacement = match value {
              FlowValue::Json(Value::String(s)) => s.clone(),
              FlowValue::Json(Value::Bool(b)) => b.to_string(),
              FlowValue::Json(v) => v.to_string().trim_matches('"').to_string(),
              _ => "".to_string(),
            };
            resolved_condition = resolved_condition.replace(&placeholder, &replacement);
          }
        }
        let condition_value = !resolved_condition.is_empty()
          && resolved_condition.to_lowercase() != "false"
          && resolved_condition.to_lowercase() != "0";

        if !condition_value {
          break;
        }

        let sub_flow = Flow::new(template.to_vec());
        let sub_flow_state_pool = sub_flow.execute_from_inputs(loop_inputs.clone()).await?;

        let exit_nodes = sub_flow.find_exit_nodes();
        println!(
          "--- While Loop: Found {} exit nodes: {:?} ---",
          exit_nodes.len(),
          exit_nodes
        );
        let mut next_loop_inputs = AsyncNodeInputs::new();
        for node_id in &exit_nodes {
          println!(
            "--- While Loop: Checking exit node '{}' in state pool ---",
            node_id
          );
          match sub_flow_state_pool.get(node_id) {
            Some(Ok(outputs)) => {
              println!(
                "--- While Loop: Exit node '{}' has {} outputs ---",
                node_id,
                outputs.len()
              );
              next_loop_inputs.extend(outputs.clone());
            }
            Some(Err(e)) => {
              println!(
                "--- While Loop: Exit node '{}' failed with error: {:?} ---",
                node_id, e
              );
            }
            None => {
              println!(
                "--- While Loop: Exit node '{}' not found in state pool ---",
                node_id
              );
            }
          }
        }
        println!(
          "--- While Loop End of Iteration: {}, Sub-flow outputs: {:?} ---",
          iteration_count + 1,
          next_loop_inputs
        );
        loop_inputs.extend(next_loop_inputs);

        iteration_count += 1;
      }

      Ok(loop_inputs)
    })
  }

  fn execute_map_node_sequential<'a>(
    &'a self,
    inputs: &'a AsyncNodeInputs,
    template: &'a [GraphNode],
  ) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
    Box::pin(async move {
      let input_list = match inputs.get("input_list") {
        Some(FlowValue::Json(Value::Array(arr))) => arr,
        _ => {
          return Err(AgentFlowError::NodeInputError {
            message: "Input 'input_list' must be a JSON array for a Map node".to_string(),
          })
        }
      };

      let mut all_results = Vec::new();
      for item in input_list {
        let sub_flow = Flow::new(template.to_vec());
        let mut initial_inputs = HashMap::new();
        initial_inputs.insert("item".to_string(), FlowValue::Json(item.clone()));

        let sub_flow_result = sub_flow.execute_from_inputs(initial_inputs).await?;
        let json_state = serde_json::to_value(sub_flow_result)?;
        all_results.push(json_state);
      }

      let mut outputs = HashMap::new();
      outputs.insert(
        "results".to_string(),
        FlowValue::Json(Value::Array(all_results)),
      );
      Ok(outputs)
    })
  }

  fn execute_map_node_parallel<'a>(
    &'a self,
    inputs: &'a AsyncNodeInputs,
    template: &'a [GraphNode],
  ) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
    Box::pin(async move {
      let input_list = match inputs.get("input_list") {
        Some(FlowValue::Json(Value::Array(arr))) => arr.clone(),
        _ => {
          return Err(AgentFlowError::NodeInputError {
            message: "Input 'input_list' must be a JSON array for a Map node".to_string(),
          })
        }
      };

      let mut handles = Vec::new();
      for item in input_list {
        let sub_flow = Flow::new(template.to_vec());
        let mut initial_inputs = HashMap::new();
        initial_inputs.insert("item".to_string(), FlowValue::Json(item.clone()));

        let handle =
          tokio::spawn(async move { sub_flow.execute_from_inputs(initial_inputs).await });
        handles.push(handle);
      }

      let results = futures::future::join_all(handles).await;

      let mut all_results = Vec::new();
      for result in results {
        match result {
          Ok(Ok(sub_flow_result)) => {
            let json_state = serde_json::to_value(sub_flow_result)?;
            all_results.push(json_state);
          }
          Ok(Err(e)) => return Err(e),
          Err(e) => {
            return Err(AgentFlowError::FlowExecutionFailed {
              message: e.to_string(),
            })
          }
        }
      }

      let mut outputs = HashMap::new();
      outputs.insert(
        "results".to_string(),
        FlowValue::Json(Value::Array(all_results)),
      );
      Ok(outputs)
    })
  }

  fn persist_step_result(
    &self,
    run_dir: &Path,
    node_id: &str,
    result: &AsyncNodeResult,
  ) -> Result<(), AgentFlowError> {
    let file_path = run_dir.join(format!("{}_outputs.json", node_id));
    let content = serde_json::to_string_pretty(result)?;
    fs::write(&file_path, content).map_err(|e| AgentFlowError::PersistenceError {
      message: e.to_string(),
    })?;
    Ok(())
  }

  fn gather_inputs(
    &self,
    node_id: &str,
    input_mapping: &HashMap<String, (String, String)>,
    state_pool: &HashMap<String, AsyncNodeResult>,
  ) -> Result<AsyncNodeInputs, AgentFlowError> {
    let mut inputs = AsyncNodeInputs::new();
    for (input_name, (source_node_id, source_output_name)) in input_mapping {
      // Check if source node is in dependencies (required) or not (optional)
      let graph_node =
        self
          .nodes
          .get(node_id)
          .ok_or_else(|| AgentFlowError::FlowExecutionFailed {
            message: format!("Node '{}' not found in graph", node_id),
          })?;
      let is_required_dependency = graph_node.dependencies.contains(source_node_id);

      match state_pool.get(source_node_id) {
        Some(Ok(source_outputs)) => {
          match source_outputs.get(source_output_name) {
            Some(input_value) => {
              inputs.insert(input_name.clone(), input_value.clone());
            }
            None if !is_required_dependency => {
              // Optional input, source node exists but output key not found - skip it
              continue;
            }
            None => {
              return Err(AgentFlowError::NodeInputError {
                message: format!(
                  "Output '{}' not found in source node '{}'",
                  source_output_name, source_node_id
                ),
              });
            }
          }
        }
        Some(Err(AgentFlowError::NodeSkipped)) if !is_required_dependency => {
          // Optional dependency was skipped - skip this input
          continue;
        }
        Some(Err(AgentFlowError::NodeSkipped)) => {
          // Required dependency was skipped - error
          return Err(AgentFlowError::DependencyNotMet {
            node_id: node_id.to_string(),
            dependency_id: source_node_id.clone(),
          });
        }
        Some(Err(e)) => return Err(e.clone()),
        None if !is_required_dependency => {
          // Optional dependency not executed - skip this input
          continue;
        }
        None => {
          return Err(AgentFlowError::FlowExecutionFailed {
            message: format!(
              "Dependency node '{}' has not been executed.",
              source_node_id
            ),
          });
        }
      }
    }
    Ok(inputs)
  }

  fn evaluate_condition(
    &self,
    condition: &str,
    state_pool: &HashMap<String, AsyncNodeResult>,
  ) -> Result<bool, AgentFlowError> {
    let expr = condition
      .trim_start_matches("{{ ")
      .trim_end_matches(" }}")
      .trim();
    println!("🔍 Evaluating condition: '{}'", expr);

    // Check for comparison operators
    if expr.contains("!=") {
      let parts: Vec<&str> = expr.split("!=").map(|s| s.trim()).collect();
      if parts.len() == 2 {
        let left_val = self.evaluate_condition_value(parts[0], state_pool)?;
        let right_val = self.evaluate_condition_literal(parts[1])?;
        let result = left_val != right_val;
        println!(
          "🔍 Comparison: '{}' != '{}' = {}",
          left_val, right_val, result
        );
        return Ok(result);
      }
    } else if expr.contains("==") {
      let parts: Vec<&str> = expr.split("==").map(|s| s.trim()).collect();
      if parts.len() == 2 {
        let left_val = self.evaluate_condition_value(parts[0], state_pool)?;
        let right_val = self.evaluate_condition_literal(parts[1])?;
        let result = left_val == right_val;
        println!(
          "🔍 Comparison: '{}' == '{}' = {}",
          left_val, right_val, result
        );
        return Ok(result);
      }
    }

    // Simple path reference (no operators)
    let parts: Vec<&str> = expr.split('.').collect();
    if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
      return Err(AgentFlowError::FlowDefinitionError {
        message: format!("Invalid run_if path: {}", expr),
      });
    }
    let node_id = parts[1];
    let output_name = parts[3];

    let source_result =
      state_pool
        .get(node_id)
        .ok_or_else(|| AgentFlowError::FlowDefinitionError {
          message: format!(
            "Node '{}' referenced in condition not found in state.",
            node_id
          ),
        })?;

    match source_result {
      Ok(outputs) => {
        let value = match outputs.get(output_name) {
          Some(v) => v,
          None => return Ok(false),
        };
        match value {
          FlowValue::Json(Value::Bool(b)) => Ok(*b),
          FlowValue::Json(Value::String(s)) => Ok(s.to_lowercase() == "true"),
          _ => Ok(false),
        }
      }
      Err(AgentFlowError::NodeSkipped) => Ok(false),
      Err(e) => Err(e.clone()),
    }
  }

  fn evaluate_condition_value(
    &self,
    path: &str,
    state_pool: &HashMap<String, AsyncNodeResult>,
  ) -> Result<String, AgentFlowError> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
      return Err(AgentFlowError::FlowDefinitionError {
        message: format!("Invalid path in condition: {}", path),
      });
    }
    let node_id = parts[1];
    let output_name = parts[3];

    let source_result =
      state_pool
        .get(node_id)
        .ok_or_else(|| AgentFlowError::FlowDefinitionError {
          message: format!(
            "Node '{}' referenced in condition not found in state.",
            node_id
          ),
        })?;

    match source_result {
      Ok(outputs) => {
        let value =
          outputs
            .get(output_name)
            .ok_or_else(|| AgentFlowError::FlowDefinitionError {
              message: format!("Output '{}' not found in node '{}'", output_name, node_id),
            })?;
        match value {
          FlowValue::Json(Value::String(s)) => Ok(s.clone()),
          FlowValue::Json(Value::Number(n)) => Ok(n.to_string()),
          FlowValue::Json(Value::Bool(b)) => Ok(b.to_string()),
          FlowValue::Json(v) => Ok(v.to_string()),
          _ => Ok(String::new()),
        }
      }
      Err(e) => Err(e.clone()),
    }
  }

  fn evaluate_condition_literal(&self, literal: &str) -> Result<String, AgentFlowError> {
    // Remove quotes from string literals
    let trimmed = literal.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
      || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
      Ok(trimmed[1..trimmed.len() - 1].to_string())
    } else {
      Ok(trimmed.to_string())
    }
  }

  fn find_exit_nodes(&self) -> Vec<String> {
    let mut all_deps = HashSet::new();
    for node in self.nodes.values() {
      for dep in &node.dependencies {
        all_deps.insert(dep.as_str());
      }
    }
    self
      .nodes
      .keys()
      .filter(|id| !all_deps.contains(id.as_str()))
      .cloned()
      .collect()
  }

  fn topological_sort(&self) -> Result<Vec<String>, AgentFlowError> {
    let mut in_degree: HashMap<String, usize> =
      self.nodes.keys().cloned().map(|id| (id, 0)).collect();
    let mut adj: HashMap<String, Vec<String>> =
      self.nodes.keys().cloned().map(|id| (id, vec![])).collect();

    for (id, node) in &self.nodes {
      for dep_id in &node.dependencies {
        if !self.nodes.contains_key(dep_id) {
          return Err(AgentFlowError::FlowDefinitionError {
            message: format!("Node '{}' has an invalid dependency: '{}'", id, dep_id),
          });
        }
        in_degree.entry(id.clone()).and_modify(|d| *d += 1);
        adj.entry(dep_id.clone()).or_default().push(id.clone());
      }
    }

    let mut queue: VecDeque<String> = in_degree
      .iter()
      .filter(|(_, &d)| d == 0)
      .map(|(id, _)| id.clone())
      .collect();

    let mut sorted_order = Vec::new();
    while let Some(u) = queue.pop_front() {
      sorted_order.push(u.clone());
      if let Some(neighbors) = adj.get(&u) {
        for v in neighbors {
          in_degree.entry(v.clone()).and_modify(|d| *d -= 1);
          if *in_degree.get(v).unwrap_or(&1) == 0 {
            queue.push_back(v.clone());
          }
        }
      }
    }

    if sorted_order.len() != self.nodes.len() {
      Err(AgentFlowError::CircularFlow)
    } else {
      Ok(sorted_order)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use async_trait::async_trait;
  use serde_json::json;
  use std::sync::Mutex;

  struct RecordingListener {
    events: Arc<Mutex<Vec<&'static str>>>,
  }

  impl EventListener for RecordingListener {
    fn on_event(&self, event: &WorkflowEvent) {
      self.events.lock().unwrap().push(event.event_type());
    }
  }

  fn use_writable_home() {
    let home = std::env::temp_dir().join(format!("agentflow-flow-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", home);
  }

  #[tokio::test]
  async fn test_map_node_sequential_execution() {
    use_writable_home();
    struct MultiplyNode;
    #[async_trait]
    impl AsyncNode for MultiplyNode {
      async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let val = match inputs.get("item").unwrap() {
          FlowValue::Json(Value::Number(n)) => n.as_i64().unwrap(),
          _ => 0,
        };
        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), FlowValue::Json(json!(val * 2)));
        Ok(outputs)
      }
    }

    let sub_flow_node = GraphNode {
      id: "multiply".to_string(),
      node_type: NodeType::Standard(Arc::new(MultiplyNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    };

    let map_node = GraphNode {
      id: "map_node".to_string(),
      node_type: NodeType::Map {
        template: vec![sub_flow_node],
        parallel: false,
      },
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: {
        let mut inputs = HashMap::new();
        inputs.insert("input_list".to_string(), FlowValue::Json(json!([1, 2, 3])));
        inputs
      },
    };

    let flow = Flow::new(vec![map_node]);

    let final_state = flow.run().await.unwrap();
    let map_result = final_state.get("map_node").unwrap().as_ref().unwrap();
    let results_array = match map_result.get("results").unwrap() {
      FlowValue::Json(Value::Array(arr)) => arr,
      _ => panic!("Not an array"),
    };

    assert_eq!(results_array.len(), 3);
  }

  #[tokio::test]
  async fn test_flow_emits_workflow_node_and_output_events() {
    use_writable_home();
    struct TestNode;
    #[async_trait]
    impl AsyncNode for TestNode {
      async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), FlowValue::Json(json!("ok")));
        Ok(outputs)
      }
    }

    let node = GraphNode {
      id: "node".to_string(),
      node_type: NodeType::Standard(Arc::new(TestNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    };
    let events = Arc::new(Mutex::new(Vec::new()));
    let listener = Arc::new(RecordingListener {
      events: events.clone(),
    });

    Flow::new(vec![node])
      .with_event_listener(listener)
      .run()
      .await
      .unwrap();

    let events = events.lock().unwrap().clone();
    assert!(events.contains(&"workflow.started"));
    assert!(events.contains(&"node.started"));
    assert!(events.contains(&"node.output.captured"));
    assert!(events.contains(&"node.completed"));
    assert!(events.contains(&"workflow.completed"));
  }

  #[tokio::test]
  async fn test_map_node_parallel_execution() {
    use_writable_home();
    struct MultiplyNode;
    #[async_trait]
    impl AsyncNode for MultiplyNode {
      async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let val = match inputs.get("item").unwrap() {
          FlowValue::Json(Value::Number(n)) => n.as_i64().unwrap(),
          _ => 0,
        };
        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), FlowValue::Json(json!(val * 2)));
        Ok(outputs)
      }
    }

    let sub_flow_node = GraphNode {
      id: "multiply".to_string(),
      node_type: NodeType::Standard(Arc::new(MultiplyNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    };

    let map_node = GraphNode {
      id: "map_node".to_string(),
      node_type: NodeType::Map {
        template: vec![sub_flow_node],
        parallel: true,
      },
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: {
        let mut inputs = HashMap::new();
        inputs.insert(
          "input_list".to_string(),
          FlowValue::Json(json!([1, 2, 3, 4, 5])),
        );
        inputs
      },
    };

    let flow = Flow::new(vec![map_node]);

    let final_state = flow.run().await.unwrap();
    let map_result = final_state.get("map_node").unwrap().as_ref().unwrap();
    let results_array = match map_result.get("results").unwrap() {
      FlowValue::Json(Value::Array(arr)) => arr,
      _ => panic!("Not an array"),
    };

    assert_eq!(results_array.len(), 5);
  }

  #[tokio::test]
  async fn test_while_node_basic_loop() {
    use_writable_home();
    struct IncrementNode;
    #[async_trait]
    impl AsyncNode for IncrementNode {
      async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let counter = match inputs.get("counter") {
          Some(FlowValue::Json(Value::Number(n))) => n.as_i64().unwrap(),
          _ => 1,
        };
        let mut outputs = HashMap::new();
        outputs.insert("counter".to_string(), FlowValue::Json(json!(counter + 1)));
        outputs.insert(
          "continue_loop".to_string(),
          FlowValue::Json(json!(counter < 4)),
        );
        Ok(outputs)
      }
    }

    let increment_node = GraphNode {
      id: "increment".to_string(),
      node_type: NodeType::Standard(Arc::new(IncrementNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    };

    let while_node = GraphNode {
      id: "while_loop".to_string(),
      node_type: NodeType::While {
        condition: "{{continue_loop}}".to_string(),
        max_iterations: 10,
        template: vec![increment_node],
      },
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: {
        let mut inputs = HashMap::new();
        inputs.insert("counter".to_string(), FlowValue::Json(json!(1)));
        inputs.insert("continue_loop".to_string(), FlowValue::Json(json!(true)));
        inputs
      },
    };

    let flow = Flow::new(vec![while_node]);
    let final_state = flow.run().await.unwrap();
    let while_result = final_state.get("while_loop").unwrap().as_ref().unwrap();

    let counter = match while_result.get("counter").unwrap() {
      FlowValue::Json(Value::Number(n)) => n.as_i64().unwrap(),
      _ => panic!("Counter should be a number"),
    };

    // Loop runs while continue_loop=true
    // Iteration 1: counter=1, sets counter=2, continue_loop=true (1 < 4 = true)
    // Iteration 2: counter=2, sets counter=3, continue_loop=true (2 < 4 = true)
    // Iteration 3: counter=3, sets counter=4, continue_loop=true (3 < 4 = true)
    // Iteration 4: counter=4, sets counter=5, continue_loop=false (4 < 4 = false)
    // Next iteration checks: continue_loop=false, loop exits
    assert_eq!(counter, 5);
  }

  #[tokio::test]
  async fn test_while_node_condition_check() {
    use_writable_home();
    struct CheckNode;
    #[async_trait]
    impl AsyncNode for CheckNode {
      async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let count = match inputs.get("count") {
          Some(FlowValue::Json(Value::Number(n))) => n.as_i64().unwrap(),
          _ => 0,
        };
        let mut outputs = HashMap::new();
        outputs.insert("count".to_string(), FlowValue::Json(json!(count + 1)));
        outputs.insert("continue".to_string(), FlowValue::Json(json!(count < 2)));
        Ok(outputs)
      }
    }

    let check_node = GraphNode {
      id: "check".to_string(),
      node_type: NodeType::Standard(Arc::new(CheckNode)),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    };

    let while_node = GraphNode {
      id: "while_loop".to_string(),
      node_type: NodeType::While {
        condition: "{{continue}}".to_string(),
        max_iterations: 10,
        template: vec![check_node],
      },
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: {
        let mut inputs = HashMap::new();
        inputs.insert("count".to_string(), FlowValue::Json(json!(0)));
        inputs.insert("continue".to_string(), FlowValue::Json(json!(true)));
        inputs
      },
    };

    let flow = Flow::new(vec![while_node]);
    let final_state = flow.run().await.unwrap();
    let while_result = final_state.get("while_loop").unwrap().as_ref().unwrap();

    let count = match while_result.get("count").unwrap() {
      FlowValue::Json(Value::Number(n)) => n.as_i64().unwrap(),
      _ => panic!("Count should be a number"),
    };

    // Loop runs while continue=true
    // Iteration 1: count=0, sets count=1, continue=true (0 < 2 = true)
    // Iteration 2: count=1, sets count=2, continue=true (1 < 2 = true)
    // Iteration 3: count=2, sets count=3, continue=false (2 < 2 = false)
    // Next iteration checks: continue=false, loop exits
    assert_eq!(count, 3);
  }
}
