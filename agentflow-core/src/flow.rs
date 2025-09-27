use crate::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub enum NodeType {
    Standard(Arc<dyn AsyncNode>),
    Map { template: Vec<GraphNode> },
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub async fn run(&self) -> Result<HashMap<String, AsyncNodeResult>, AgentFlowError> {
        let run_id = Uuid::new_v4().to_string();
        let run_dir = PathBuf::from("runs").join(&run_id);
        fs::create_dir_all(&run_dir).map_err(|e| AgentFlowError::PersistenceError { message: e.to_string() })?;
        println!("💾 Run artifacts will be saved to: {}", run_dir.display());

        let sorted_nodes = self.topological_sort()?;
        let mut state_pool: HashMap<String, AsyncNodeResult> = HashMap::new();

        for node_id in sorted_nodes {
            let graph_node = self.nodes.get(&node_id).unwrap();

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
                Some(mapping) => self.gather_inputs(&node_id, mapping, &state_pool)?,
                None => HashMap::new(),
            };
            for (k, v) in &graph_node.initial_inputs {
                inputs.insert(k.clone(), v.clone());
            }

            println!("▶️  Executing node '{}'", node_id);
            let result = match &graph_node.node_type {
                NodeType::Standard(node) => node.execute(&inputs).await,
                NodeType::Map { template } => self.execute_map_node_sequential(&inputs, template).await,
            };

            self.persist_step_result(&run_dir, &node_id, &result)?;
            state_pool.insert(node_id.to_string(), result);
        }

        Ok(state_pool)
    }

    async fn execute_map_node_sequential(&self, inputs: &AsyncNodeInputs, template: &[GraphNode]) -> AsyncNodeResult {
        let input_list = match inputs.get("input_list") {
            Some(FlowValue::Json(Value::Array(arr))) => arr,
            _ => return Err(AgentFlowError::NodeInputError { message: "Input 'input_list' must be a JSON array for a Map node".to_string() }),
        };

        let mut all_results = Vec::new();
        for item in input_list {
            let mut sub_flow = Flow::new();
            for node_template in template {
                sub_flow.add_node(node_template.clone());
            }

            let mut initial_inputs = HashMap::new();
            initial_inputs.insert("item".to_string(), FlowValue::Json(item.clone()));

            if let Some(first_node_id) = sub_flow.topological_sort()?.first().map(|s| (*s).clone()) {
                if let Some(node) = sub_flow.nodes.get_mut(&first_node_id) {
                    node.initial_inputs.extend(initial_inputs);
                }
            }

            let sub_flow_result = Box::pin(sub_flow.run()).await?;
            let json_state = serde_json::to_value(sub_flow_result)?;
            all_results.push(json_state);
        }

        let mut outputs = HashMap::new();
        outputs.insert("results".to_string(), FlowValue::Json(Value::Array(all_results)));
        Ok(outputs)
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
            let source_result = state_pool.get(source_node_id).ok_or_else(|| AgentFlowError::FlowExecutionFailed {
                message: format!("Dependency node '{}' has not been executed.", source_node_id),
            })?;

            match source_result {
                Ok(source_outputs) => {
                    let input_value = source_outputs.get(source_output_name).ok_or_else(|| AgentFlowError::NodeInputError {
                        message: format!("Output '{}' not found in source node '{}'", source_output_name, source_node_id),
                    })?;
                    inputs.insert(input_name.clone(), input_value.clone());
                }
                Err(AgentFlowError::NodeSkipped) => {
                    return Err(AgentFlowError::DependencyNotMet { 
                        node_id: node_id.to_string(), 
                        dependency_id: source_node_id.clone() 
                    });
                }
                Err(e) => return Err(e.clone()),
            }
        }
        Ok(inputs)
    }

    fn evaluate_condition(&self, condition: &str, state_pool: &HashMap<String, AsyncNodeResult>) -> Result<bool, AgentFlowError> {
        let path = condition.trim_start_matches("{{ ").trim_end_matches(" }}");
        let parts: Vec<&str> = path.split('.').collect();

        if parts.len() != 4 || parts[0] != "nodes" || parts[2] != "outputs" {
            return Err(AgentFlowError::FlowDefinitionError{ message: format!("Invalid run_if path: {}", path) });
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
                    _ => Ok(false)
                }
            }
            Err(AgentFlowError::NodeSkipped) => Ok(false),
            Err(e) => Err(e.clone()),
        }
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

        let mut flow = Flow::new();
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
            node_type: NodeType::Map { template: vec![sub_flow_node] },
            dependencies: vec![],
            input_mapping: None,
            run_if: None,
            initial_inputs: {
                let mut inputs = HashMap::new();
                inputs.insert("input_list".to_string(), FlowValue::Json(json!([1, 2, 3])));
                inputs
            },
        };

        flow.add_node(map_node);

        let final_state = flow.run().await.unwrap();
        let map_result = final_state.get("map_node").unwrap().as_ref().unwrap();
        let results_array = match map_result.get("results").unwrap() {
            FlowValue::Json(Value::Array(arr)) => arr,
            _ => panic!("Not an array"),
        };

        assert_eq!(results_array.len(), 3);
    }
}
