use agentflow_core::{
    error::AgentFlowError,
    flow::{Flow, GraphNode, NodeType},
    value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::{json, Value};

// Helper function to check for API key and skip test if not present
fn check_api_key() -> bool {
    if std::env::var("STEPFUN_API_KEY").is_err() {
        println!("Skipping integration test: STEPFUN_API_KEY not set.");
        true
    } else {
        false
    }
}

#[tokio::test]
async fn test_simple_two_step_llm_workflow() {
    if check_api_key() { return; }

    let prompt_generator_node = GraphNode {
        id: "prompt_generator".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new(
            "prompt_generator",
            "Use a single word to answer: What is the capital of France?"
        ))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: HashMap::new(),
    };

    let answer_generator_node = GraphNode {
        id: "answer_generator".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec!["prompt_generator".to_string()],
        input_mapping: Some({
            let mut map = HashMap::new();
            map.insert(
                "prompt".to_string(),
                ("prompt_generator".to_string(), "output".to_string()),
            );
            map
        }),
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
            map
        },
    };

    let flow = Flow::new(vec![prompt_generator_node, answer_generator_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    let llm_result = final_state.get("answer_generator").expect("LLM node not in final state").as_ref().expect("LLM node result was an error");
    let final_answer = llm_result.get("output").expect("LLM output not found");

    if let FlowValue::Json(serde_json::Value::String(answer_str)) = final_answer {
        println!("LLM Answer: {}", answer_str);
        assert!(answer_str.to_lowercase().contains("paris"));
    } else {
        panic!("Final answer was not a string FlowValue");
    }
}

#[tokio::test]
async fn test_conditional_workflow_runs() {
    if check_api_key() { return; }

    let condition_node = GraphNode {
        id: "condition_node".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
            map.insert("prompt".to_string(), FlowValue::Json(json!("Is the sky blue? Respond with only the word 'true' or 'false'.")));
            map
        },
    };

    let conditional_node = GraphNode {
        id: "conditional_node".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new("conditional_node", "The condition was true!"))),
        dependencies: vec!["condition_node".to_string()],
        input_mapping: None,
        run_if: Some("{{ nodes.condition_node.outputs.output }}".to_string()),
        initial_inputs: HashMap::new(),
    };

    let flow = Flow::new(vec![condition_node, conditional_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    let conditional_result = final_state.get("conditional_node").expect("Conditional node not in final state").as_ref();
    assert!(conditional_result.is_ok(), "Conditional node should have run");
    let outputs = conditional_result.unwrap();
    let message = outputs.get("output").unwrap();
    assert_eq!(message, &FlowValue::Json(json!("The condition was true!")));
}

#[tokio::test]
async fn test_conditional_workflow_skips() {
    if check_api_key() { return; }

    let condition_node = GraphNode {
        id: "condition_node".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
            map.insert("prompt".to_string(), FlowValue::Json(json!("Is the earth flat? Respond with only the word 'true' or 'false'.")));
            map
        },
    };

    let conditional_node = GraphNode {
        id: "conditional_node".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new("conditional_node", "This should not be rendered."))),
        dependencies: vec!["condition_node".to_string()],
        input_mapping: None,
        run_if: Some("{{ nodes.condition_node.outputs.output }}".to_string()),
        initial_inputs: HashMap::new(),
    };

    let flow = Flow::new(vec![condition_node, conditional_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    let conditional_result = final_state.get("conditional_node").expect("Conditional node not in final state").as_ref();
    assert!(matches!(conditional_result, Err(AgentFlowError::NodeSkipped)));
}

#[tokio::test]
async fn test_parallel_map_workflow() {
    if check_api_key() { return; }

    let sub_flow_template = vec![
        GraphNode {
            id: "poem_prompt".to_string(),
            node_type: NodeType::Standard(Arc::new(TemplateNode::new("poem_prompt", "Write a four-line poem about {{item}}."))),
            dependencies: vec![],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
        GraphNode {
            id: "poem_generator".to_string(),
            node_type: NodeType::Standard(Arc::new(LlmNode::default())),
            dependencies: vec!["poem_prompt".to_string()],
            input_mapping: Some([("prompt".to_string(), ("poem_prompt".to_string(), "output".to_string()))].into()),
            run_if: None,
            initial_inputs: {
                let mut map = HashMap::new();
                map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
                map
            },
        },
    ];

    let map_node = GraphNode {
        id: "parallel_poem_map".to_string(),
        node_type: NodeType::Map { 
            template: sub_flow_template,
            parallel: true,
        },
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            let topics = vec!["the sun", "the moon", "the stars"];
            map.insert("input_list".to_string(), FlowValue::Json(json!(topics)));
            map
        },
    };

    let flow = Flow::new(vec![map_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    let map_result = final_state.get("parallel_poem_map").expect("Map node not in final state").as_ref().expect("Map node result was an error");
    let results_array = match map_result.get("results") {
        Some(FlowValue::Json(Value::Array(arr))) => arr,
        _ => panic!("Map output was not a JSON array"),
    };

    assert_eq!(results_array.len(), 3, "Should have produced 3 results for 3 inputs");
    println!("Parallel map execution successful with 3 results.");
}

#[tokio::test]
async fn test_stateful_while_loop_workflow() {
    if check_api_key() { return; }

    let sub_flow_template = vec![
        GraphNode {
            id: "decrementer_prompt".to_string(),
            node_type: NodeType::Standard(Arc::new(TemplateNode::new("decrementer_prompt", "Calculate {{counter}} - 1. Respond with only the resulting number."))),
            dependencies: vec![],
            input_mapping: None, 
            run_if: None,
            initial_inputs: HashMap::new(),
        },
        GraphNode {
            id: "decrementer_llm".to_string(),
            node_type: NodeType::Standard(Arc::new(LlmNode::default())),
            dependencies: vec!["decrementer_prompt".to_string()],
            input_mapping: Some([("prompt".to_string(), ("decrementer_prompt".to_string(), "output".to_string()))].into()),
            run_if: None,
            initial_inputs: {
                let mut map = HashMap::new();
                map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
                map
            },
        },
        GraphNode {
            id: "state_updater".to_string(),
            node_type: NodeType::Standard(Arc::new(
                TemplateNode::new("state_updater", "{{output}}").with_output_key("counter")
            )),
            dependencies: vec!["decrementer_llm".to_string()],
            input_mapping: Some([("output".to_string(), ("decrementer_llm".to_string(), "output".to_string()))].into()),
            run_if: None,
            initial_inputs: HashMap::new(),
        },
    ];

    let while_node = GraphNode {
        id: "counter_loop".to_string(),
        node_type: NodeType::While {
            condition: "{{counter}}".to_string(),
            max_iterations: 5,
            template: sub_flow_template,
        },
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("counter".to_string(), FlowValue::Json(json!("2"))); // Start with a string
            map
        },
    };

    let flow = Flow::new(vec![while_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    let loop_result = final_state.get("counter_loop").expect("Loop node not in final state").as_ref().expect("Loop node result was an error");
    let final_count = loop_result.get("counter").expect("Final count not found");

    assert_eq!(final_count, &FlowValue::Json(json!("0")));
}
