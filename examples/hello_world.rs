// AgentFlow Hello World Example
// Migrated from PocketFlow cookbook/pocketflow-hello-world
// Tests: Basic AsyncNode functionality, SharedState, and Flow execution

use agentflow_core::{AsyncFlow, AsyncNode, Result, SharedState};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

/// Mock LLM call for testing (simulates OpenAI API call)
async fn call_mock_llm(prompt: &str) -> String {
  // Simulate API call delay
  tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

  // Mock responses based on common questions
  match prompt.to_lowercase().as_str() {
        p if p.contains("end of universe") => {
            "The universe will likely end in heat death, with maximum entropy and no energy gradients."
        },
        p if p.contains("meaning of life") => {
            "42, according to The Hitchhiker's Guide to the Galaxy."
        },
        p if p.contains("hello") => {
            "Hello! I'm a mock LLM response from AgentFlow."
        },
        _ => {
            "I'm a mock LLM. In a real implementation, this would call OpenAI, Anthropic, or another LLM API."
        }
    }.to_string()
}

/// Answer Node - equivalent to PocketFlow's AnswerNode
/// Tests the three-phase AsyncNode lifecycle: prep -> exec -> post
struct AnswerNode {
  node_id: String,
}

impl AnswerNode {
  fn new() -> Self {
    Self {
      node_id: "answer_node".to_string(),
    }
  }
}

#[async_trait]
impl AsyncNode for AnswerNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Phase 1: Preparation - read question from shared state
    let question = shared
      .get("question")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "What is the meaning of life?".to_string());

    println!("🔍 [PREP] Retrieved question: {}", question);

    Ok(Value::String(question))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    // Phase 2: Execution - call LLM with the question
    let question = prep_result.as_str().unwrap_or("");

    println!("🤖 [EXEC] Calling LLM with: {}", question);
    let start = Instant::now();

    let answer = call_mock_llm(question).await;

    let duration = start.elapsed();
    println!("⚡ [EXEC] LLM response received in {:?}", duration);

    Ok(serde_json::json!({
        "answer": answer,
        "response_time_ms": duration.as_millis(),
        "model": "mock-llm-v1"
    }))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep: Value,
    exec: Value,
  ) -> Result<Option<String>> {
    // Phase 3: Post-processing - store answer in shared state
    let answer = exec["answer"].as_str().unwrap_or("No answer received");
    let response_time = exec["response_time_ms"].as_u64().unwrap_or(0);

    shared.insert("answer".to_string(), Value::String(answer.to_string()));
    shared.insert(
      "response_time_ms".to_string(),
      Value::Number(response_time.into()),
    );

    println!("💾 [POST] Stored answer in shared state");
    println!("📊 [POST] Response time: {}ms", response_time);

    // Return None to end the flow (equivalent to no return in PocketFlow)
    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.node_id.clone())
  }
}

/// Create Q&A flow equivalent to PocketFlow's qa_flow
fn create_qa_flow() -> AsyncFlow {
  let answer_node = Box::new(AnswerNode::new());
  AsyncFlow::new(answer_node)
}

/// Main function equivalent to PocketFlow's main.py
async fn run_hello_world_example() -> Result<()> {
  println!("🚀 AgentFlow Hello World Example");
  println!("📝 Migrated from: PocketFlow cookbook/pocketflow-hello-world");
  println!("🎯 Testing: Basic AsyncNode, SharedState, Flow execution\n");

  // Create shared state with question (equivalent to PocketFlow's shared dict)
  let shared = SharedState::new();
  shared.insert(
    "question".to_string(),
    Value::String("In one sentence, what's the end of universe?".to_string()),
  );

  println!(
    "❓ Question: {}",
    shared.get("question").unwrap().as_str().unwrap()
  );

  // Create and run the Q&A flow
  let qa_flow = create_qa_flow();
  let start_time = Instant::now();

  match qa_flow.run_async(&shared).await {
    Ok(result) => {
      let total_duration = start_time.elapsed();

      println!("\n✅ Flow completed successfully in {:?}", total_duration);
      println!("📋 Final result: {:?}", result);

      // Extract and display results
      let answer = shared
        .get("answer")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "No answer found".to_string());

      let response_time = shared
        .get("response_time_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

      println!("\n🎯 Results:");
      println!("Answer: {}", answer);
      println!("LLM Response Time: {}ms", response_time);
      println!("Total Flow Time: {:?}", total_duration);

      // Verify functionality
      assert!(!answer.is_empty(), "Answer should not be empty");
      assert!(response_time > 0, "Response time should be recorded");

      println!("\n✅ All assertions passed - AgentFlow core functionality verified!");
    }
    Err(e) => {
      println!("❌ Flow failed: {}", e);
      return Err(e);
    }
  }

  Ok(())
}

/// Advanced example: Testing with multiple questions
async fn run_batch_questions_example() -> Result<()> {
  println!("\n🔄 Running batch questions test...");

  let questions = vec![
    "What is the meaning of life?",
    "Hello, how are you?",
    "What is the capital of France?",
  ];

  for (i, question) in questions.iter().enumerate() {
    println!("\n--- Question {} ---", i + 1);

    let shared = SharedState::new();
    shared.insert("question".to_string(), Value::String(question.to_string()));

    let qa_flow = create_qa_flow();
    let start = Instant::now();

    qa_flow.run_async(&shared).await?;

    let duration = start.elapsed();
    let answer = shared
      .get("answer")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "No answer".to_string());

    println!("Q: {}", question);
    println!("A: {}", answer);
    println!("Time: {:?}", duration);
  }

  println!("\n✅ Batch questions test completed!");
  Ok(())
}

/// Comparison with PocketFlow performance
async fn performance_comparison() {
  println!("\n📊 Performance Comparison Notes:");
  println!("PocketFlow (Python):");
  println!("  - Synchronous execution");
  println!("  - Dict-based shared state");
  println!("  - String-based routing");
  println!();
  println!("AgentFlow (Rust):");
  println!("  - Async execution with tokio");
  println!("  - Type-safe SharedState with Arc<RwLock>");
  println!("  - Structured error handling with Result types");
  println!("  - Zero-cost abstractions");
  println!();
  println!("Expected improvements:");
  println!("  - 🚀 Lower memory usage");
  println!("  - ⚡ Better concurrency handling");
  println!("  - 🛡️ Memory and thread safety");
  println!("  - 📊 Built-in observability hooks");
}

#[tokio::main]
async fn main() -> Result<()> {
  // Run the basic hello world example
  run_hello_world_example().await?;

  // Run batch questions test
  run_batch_questions_example().await?;

  // Show performance comparison notes
  performance_comparison().await;

  println!("\n🎉 Hello World migration completed successfully!");
  println!("🔬 Core AgentFlow functionality verified:");
  println!("  ✅ AsyncNode three-phase lifecycle");
  println!("  ✅ SharedState thread-safe storage");
  println!("  ✅ AsyncFlow execution engine");
  println!("  ✅ Error handling with Result types");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_answer_node_lifecycle() {
    let node = AnswerNode::new();
    let shared = SharedState::new();
    shared.insert(
      "question".to_string(),
      Value::String("Test question?".to_string()),
    );

    // Test prep phase
    let prep_result = node.prep_async(&shared).await.unwrap();
    assert_eq!(prep_result.as_str().unwrap(), "Test question?");

    // Test exec phase
    let exec_result = node.exec_async(prep_result).await.unwrap();
    assert!(exec_result["answer"].as_str().unwrap().len() > 0);

    // Test post phase
    let post_result = node
      .post_async(&shared, Value::Null, exec_result)
      .await
      .unwrap();
    assert!(post_result.is_none()); // Should end flow

    // Verify shared state was updated
    assert!(shared.get("answer").is_some());
  }

  #[tokio::test]
  async fn test_qa_flow_execution() {
    let shared = SharedState::new();
    shared.insert("question".to_string(), Value::String("Test?".to_string()));

    let flow = create_qa_flow();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());
    assert!(shared.get("answer").is_some());
    assert!(shared.get("response_time_ms").is_some());
  }

  #[tokio::test]
  async fn test_mock_llm() {
    let response = call_mock_llm("What is the meaning of life?").await;
    assert!(response.contains("42"));

    let response = call_mock_llm("Hello").await;
    assert!(response.to_lowercase().contains("hello"));
  }
}
