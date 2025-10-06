use agentflow_core::{
    flow::{Flow, GraphNode, NodeType},
    value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;

#[tokio::test]
async fn test_simple_two_step_llm_workflow() {
    if std::env::var("STEPFUN_API_KEY").is_err() {
        println!("Skipping workflow integration test: STEPFUN_API_KEY not set.");
        return;
    }

    // Node A: A TemplateNode to generate the initial prompt
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

    // Node B: An LlmNode that uses the output of Node A as its prompt
    let answer_generator_node = GraphNode {
        id: "answer_generator".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())), // Using a default LLM node
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
            // We can specify the model to use here
            map.insert("model".to_string(), FlowValue::Json(json!("step-2-mini")));
            map
        },
    };

    // Create and run the flow
    let flow = Flow::new(vec![prompt_generator_node, answer_generator_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    // Assert the final output
    let llm_result = final_state.get("answer_generator").expect("LLM node not in final state").as_ref().expect("LLM node result was an error");
    let final_answer = llm_result.get("output").expect("LLM output not found");

    if let FlowValue::Json(serde_json::Value::String(answer_str)) = final_answer {
        println!("LLM Answer: {}", answer_str);
        assert!(answer_str.to_lowercase().contains("paris"));
    } else {
        panic!("Final answer was not a string FlowValue");
    }
}
