// Test the Rust Interview Questions Workflow using agentflow-core directly
// This demonstrates the workflow functionality with mock LLM responses

use agentflow_core::{AsyncNode, SharedState};
use agentflow_core::nodes::LlmNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Rust Interview Questions Workflow Test");
    
    // Initialize shared state
    let shared_state = SharedState::new();
    
    println!("\n📝 Setting up workflow nodes...");
    
    // Node 1: Question Generator
    let question_generator = LlmNode::new("question_generator", "mock-model")
        .with_system("A senior Rust engineer with extensive backend development experience")
        .with_prompt("Please help me create 5 Rust backend interview questions")
        .with_temperature(0.7)
        .with_max_tokens(800);
    
    println!("✅ Node 1 (question_generator) created");
    
    // Execute Node 1
    println!("\n🔧 Executing Node 1: Question Generator...");
    let _node1_result = question_generator.run_async(&shared_state).await?;
    
    // Get the generated questions from shared state
    let interview_questions = shared_state.get("question_generator_output")
        .map(|v| v.as_str().unwrap_or("Mock interview questions generated").to_string())
        .unwrap_or_else(|| "Mock interview questions generated".to_string());
    
    println!("✅ Node 1 completed. Questions generated:");
    println!("   {}", interview_questions);
    
    // Node 2: Question Evaluator (uses output from Node 1)
    let question_evaluator = LlmNode::new("question_evaluator", "mock-model")
        .with_system("You are a senior Rust backend interviewer, help me evaluate whether the following interview questions meet the standards for 3-5 years of Rust backend development")
        .with_prompt("{{ question_generator_output }}")  // Template referencing Node 1 output
        .with_temperature(0.6)
        .with_max_tokens(600);
    
    println!("\n✅ Node 2 (question_evaluator) created");
    
    // Execute Node 2
    println!("\n🔧 Executing Node 2: Question Evaluator...");
    let _node2_result = question_evaluator.run_async(&shared_state).await?;
    
    // Get the evaluation result from shared state
    let evaluation_result = shared_state.get("question_evaluator_output")
        .map(|v| v.as_str().unwrap_or("Mock evaluation completed").to_string())
        .unwrap_or_else(|| "Mock evaluation completed".to_string());
    
    println!("✅ Node 2 completed. Evaluation result:");
    println!("   {}", evaluation_result);
    
    println!("\n🎯 Workflow Results:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    println!("\n📋 Generated Questions:");
    if let Some(questions) = shared_state.get("question_generator_output") {
        println!("{}", questions.as_str().unwrap_or("N/A"));
    }
    
    println!("\n📊 Quality Evaluation:");
    if let Some(evaluation) = shared_state.get("question_evaluator_output") {
        println!("{}", evaluation.as_str().unwrap_or("N/A"));
    }
    
    println!("\n✨ Workflow execution completed successfully!");
    
    // Show how template resolution worked
    println!("\n🔍 Template Resolution Demo:");
    println!("   Original template: {{ question_generator_output }}");
    let resolved = shared_state.resolve_template_advanced("{{ question_generator_output }}");
    println!("   Resolved to: {}", resolved);
    
    Ok(())
}