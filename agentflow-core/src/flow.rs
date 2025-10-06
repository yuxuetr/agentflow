use crate::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque, HashSet};
use std::fs;
use std::future::Future;
use std::pin::Pin;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;
use dirs;

#[derive(Clone)]
pub enum NodeType {
    Standard(Arc<dyn AsyncNode>),
    Map { template: Vec<GraphNode>, parallel: bool },
    While {
        condition: String,
        max_iterations: u32,
        template: Vec<GraphNode>
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
}

impl Flow {
    pub fn new(nodes: Vec<GraphNode>) -> Self {
        let nodes_map = nodes.into_iter().map(|n| (n.id.clone(), n)).collect();
        Self { nodes: nodes_map }
    }

    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub async fn run(&self) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
        self.execute_from_inputs(HashMap::new()).await
    }

    pub async fn execute_from_inputs(&self, initial_inputs: AsyncNodeInputs) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
        let run_id = Uuid::new_v4().to_string();
        let base_dir = dirs::home_dir()
            .ok_or_else(|| AgentFlowError::ConfigurationError { message: "Could not find home directory".to_string() })?
            .join(".agentflow")
            .join("runs");
        let run_dir = base_dir.join(&run_id);
        fs::create_dir_all(&run_dir).map_err(|e| AgentFlowError::PersistenceError { message: e.to_string() })?;

        let sorted_nodes = self.topological_sort()?;
        let mut state_pool: HashMap<String, AsyncNodeResult> = HashMap::new();

        for node_id in &sorted_nodes {
            let graph_node = self.nodes.get(node_id).unwrap();

            let should_run = match &graph_node.run_if {
                Some(condition) => self.evaluate_condition(condition, &state_pool)?,
                None => true,
            };

            if !should_run {
                println!("⏭️  Skipping node '{}' due to condition.", node_id);
                let result = Err(AgentFlowError::NodeSkipped);
                self.persist_step_result(&run_dir, &node_id, &result)?;
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

            println!("▶️  Executing node '{}'", node_id);
            let result = match &graph_node.node_type {
                NodeType::Standard(node) => node.execute(&inputs).await,
                NodeType::Map { template, parallel } => {
                    if *parallel {
                        self.execute_map_node_parallel(&inputs, template).await
                    } else {
                        self.execute_map_node_sequential(&inputs, template).await
                    }
                }
                NodeType::While { condition, max_iterations, template } => self.execute_while_node(&inputs, condition, *max_iterations, template).await,
            };

            self.persist_step_result(&run_dir, &node_id, &result)?;
            state_pool.insert(node_id.to_string(), result);
        }

        Ok(state_pool)
    }

    fn execute_while_node<'a>(&'a self, inputs: &'a AsyncNodeInputs, condition_template: &'a str, max_iterations: u32, template: &'a [GraphNode]) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
        Box::pin(async move {
            let mut loop_inputs = inputs.clone();
            let mut iteration_count = 0u32;

            while iteration_count < max_iterations {
                println!("--- While Loop Iteration: {}, State: {:?} ---", iteration_count + 1, loop_inputs);
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
                let condition_value = !resolved_condition.is_empty() && resolved_condition.to_lowercase() != "false" && resolved_condition.to_lowercase() != "0";

                if !condition_value {
                    break;
                }

                let sub_flow = Flow::new(template.to_vec());
                let sub_flow_state_pool = sub_flow.execute_from_inputs(loop_inputs.clone()).await?;

                let exit_nodes = sub_flow.find_exit_nodes();
                println!("--- While Loop: Found {} exit nodes: {:?} ---", exit_nodes.len(), exit_nodes);
                let mut next_loop_inputs = AsyncNodeInputs::new();
                for node_id in &exit_nodes {
                    println!("--- While Loop: Checking exit node '{}' in state pool ---", node_id);
                    match sub_flow_state_pool.get(node_id) {
                        Some(Ok(outputs)) => {
                            println!("--- While Loop: Exit node '{}' has {} outputs ---", node_id, outputs.len());
                            next_loop_inputs.extend(outputs.clone());
                        }
                        Some(Err(e)) => {
                            println!("--- While Loop: Exit node '{}' failed with error: {:?} ---", node_id, e);
                        }
                        None => {
                            println!("--- While Loop: Exit node '{}' not found in state pool ---", node_id);
                        }
                    }
                }
                println!("--- While Loop End of Iteration: {}, Sub-flow outputs: {:?} ---", iteration_count + 1, next_loop_inputs);
                loop_inputs.extend(next_loop_inputs);

                iteration_count += 1;
            }

            Ok(loop_inputs)
        })
    }

    fn execute_map_node_sequential<'a>(&'a self, inputs: &'a AsyncNodeInputs, template: &'a [GraphNode]) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
        Box::pin(async move {
            let input_list = match inputs.get("input_list") {
                Some(FlowValue::Json(Value::Array(arr))) => arr,
                _ => return Err(AgentFlowError::NodeInputError { message: "Input 'input_list' must be a JSON array for a Map node".to_string() }),
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
            outputs.insert("results".to_string(), FlowValue::Json(Value::Array(all_results)));
            Ok(outputs)
        })
    }

    fn execute_map_node_parallel<'a>(&'a self, inputs: &'a AsyncNodeInputs, template: &'a [GraphNode]) -> Pin<Box<dyn Future<Output = AsyncNodeResult> + Send + 'a>> {
        Box::pin(async move {
            let input_list = match inputs.get("input_list") {
                Some(FlowValue::Json(Value::Array(arr))) => arr.clone(),
                _ => return Err(AgentFlowError::NodeInputError { message: "Input 'input_list' must be a JSON array for a Map node".to_string() }),
            };

            let mut handles = Vec::new();
            for item in input_list {
                let sub_flow = Flow::new(template.to_vec());
                let mut initial_inputs = HashMap::new();
                initial_inputs.insert("item".to_string(), FlowValue::Json(item.clone()));

                let handle = tokio::spawn(async move {
                    sub_flow.execute_from_inputs(initial_inputs).await
                });
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
                    Err(e) => return Err(AgentFlowError::FlowExecutionFailed{ message: e.to_string() }),
                }
            }

            let mut outputs = HashMap::new();
            outputs.insert("results".to_string(), FlowValue::Json(Value::Array(all_results)));
            Ok(outputs)
        })
    }

    fn persist_step_result(&self, run_dir: &PathBuf, node_id: &str, result: &AsyncNodeResult) -> Result<(), AgentFlowError> {
        let file_path = run_dir.join(format!("{}_outputs.json", node_id));
        let content = serde_json::to_string_pretty(result)?;
        fs::write(&file_path, content).map_err(|e| AgentFlowError::PersistenceError { message: e.to_string() })?;
        Ok(())
    }

    fn gather_inputs(&self, node_id: &str, input_mapping: &HashMap<String, (String, String)>, state_pool: &HashMap<String, AsyncNodeResult>) -> Result<AsyncNodeInputs, AgentFlowError> {
        let mut inputs = AsyncNodeInputs::new();
        for (input_name, (source_node_id, source_output_name)) in input_mapping {
            // Check if source node is in dependencies (required) or not (optional)
            let graph_node = self.nodes.get(node_id).ok_or_else(|| AgentFlowError::FlowExecutionFailed {
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
                                message: format!("Output '{}' not found in source node '{}'", source_output_name, source_node_id),
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
                        dependency_id: source_node_id.clone()
                    });
                }
                Some(Err(e)) => return Err(e.clone()),
                None if !is_required_dependency => {
                    // Optional dependency not executed - skip this input
                    continue;
                }
                None => {
                    return Err(AgentFlowError::FlowExecutionFailed {
                        message: format!("Dependency node '{}' has not been executed.", source_node_id),
                    });
                }
            }
        }
        Ok(inputs)
    }

    fn evaluate_condition(&self, condition: &str, state_pool: &HashMap<String, AsyncNodeResult>) -> Result<bool, AgentFlowError> {
        let expr = condition.trim_start_matches("{{ ").trim_end_matches(" }}").trim();
        println!("🔍 Evaluating condition: '{}'", expr);

        // Check for comparison operators
        if expr.contains("!=") {
            let parts: Vec<&str> = expr.split("!=").map(|s| s.trim()).collect();
            if parts.len() == 2 {
                let left_val = self.evaluate_condition_value(parts[0], state_pool)?;
                let right_val = self.evaluate_condition_literal(parts[1])?;
                let result = left_val != right_val;
                println!("🔍 Comparison: '{}' != '{}' = {}", left_val, right_val, result);
                return Ok(result);
            }
        } else if expr.contains("==") {
            let parts: Vec<&str> = expr.split("==").map(|s| s.trim()).collect();
            if parts.len() == 2 {
                let left_val = self.evaluate_condition_value(parts[0], state_pool)?;
                let right_val = self.evaluate_condition_literal(parts[1])?;
                let result = left_val == right_val;
                println!("🔍 Comparison: '{}' == '{}' = {}", left_val, right_val, result);
                return Ok(result);
            }
        }

        // Simple path reference (no operators)
        let parts: Vec<&str> = expr.split('.').collect();
        if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
            return Err(AgentFlowError::FlowDefinitionError{ message: format!("Invalid run_if path: {}", expr) });
        }
        let node_id = parts[1];
        let output_name = parts[3];

        let source_result = state_pool.get(node_id).ok_or_else(|| AgentFlowError::FlowDefinitionError {
            message: format!("Node '{}' referenced in condition not found in state.", node_id)
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
                    _ => Ok(false)
                }
            }
            Err(AgentFlowError::NodeSkipped) => Ok(false),
            Err(e) => Err(e.clone()),
        }
    }

    fn evaluate_condition_value(&self, path: &str, state_pool: &HashMap<String, AsyncNodeResult>) -> Result<String, AgentFlowError> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
            return Err(AgentFlowError::FlowDefinitionError{ message: format!("Invalid path in condition: {}", path) });
        }
        let node_id = parts[1];
        let output_name = parts[3];

        let source_result = state_pool.get(node_id).ok_or_else(|| AgentFlowError::FlowDefinitionError {
            message: format!("Node '{}' referenced in condition not found in state.", node_id)
        })?;

        match source_result {
            Ok(outputs) => {
                let value = outputs.get(output_name).ok_or_else(|| AgentFlowError::FlowDefinitionError {
                    message: format!("Output '{}' not found in node '{}'", output_name, node_id)
                })?;
                match value {
                    FlowValue::Json(Value::String(s)) => Ok(s.clone()),
                    FlowValue::Json(Value::Number(n)) => Ok(n.to_string()),
                    FlowValue::Json(Value::Bool(b)) => Ok(b.to_string()),
                    FlowValue::Json(v) => Ok(v.to_string()),
                    _ => Ok(String::new())
                }
            }
            Err(e) => Err(e.clone()),
        }
    }

    fn evaluate_condition_literal(&self, literal: &str) -> Result<String, AgentFlowError> {
        // Remove quotes from string literals
        let trimmed = literal.trim();
        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
            Ok(trimmed[1..trimmed.len()-1].to_string())
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
        self.nodes.keys()
            .filter(|id| !all_deps.contains(id.as_str()))
            .cloned()
            .collect()
    }

    fn topological_sort(&self) -> Result<Vec<String>, AgentFlowError> {
        let mut in_degree: HashMap<String, usize> = self.nodes.keys().cloned().map(|id| (id, 0)).collect();
        let mut adj: HashMap<String, Vec<String>> = self.nodes.keys().cloned().map(|id| (id, vec![])).collect();

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
                    if *in_degree.get(v).unwrap() == 0 {
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

    #[tokio::test]
    async fn test_map_node_sequential_execution() {
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
            node_type: NodeType::Map { template: vec![sub_flow_node], parallel: false },
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
    async fn test_map_node_parallel_execution() {
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
            node_type: NodeType::Map { template: vec![sub_flow_node], parallel: true },
            dependencies: vec![],
            input_mapping: None,
            run_if: None,
            initial_inputs: {
                let mut inputs = HashMap::new();
                inputs.insert("input_list".to_string(), FlowValue::Json(json!([1, 2, 3, 4, 5])));
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
                outputs.insert("continue_loop".to_string(), FlowValue::Json(json!(counter < 4)));
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