use agentflow_core::{Flow, Node, Result, SharedState};
use serde_json::Value;

// A simple greeting node
struct GreetingNode {
  name: String,
}

impl Node for GreetingNode {
  fn prep(&self, shared: &SharedState) -> Result<Value> {
    let user_name = shared
      .get("user_name")
      .and_then(|v| v.as_str().map(|s| s.to_string()))
      .unwrap_or_else(|| "World".to_string());
    Ok(Value::String(user_name))
  }

  fn exec(&self, prep_result: Value) -> Result<Value> {
    let user_name = prep_result.as_str().unwrap_or("World");
    let greeting = format!("Hello, {}! Welcome to AgentFlow.", user_name);
    Ok(Value::String(greeting))
  }

  fn post(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    shared.insert("greeting".to_string(), exec_result);
    Ok(Some("process".to_string()))
  }
}

// A processing node
struct ProcessingNode;

impl Node for ProcessingNode {
  fn prep(&self, shared: &SharedState) -> Result<Value> {
    let greeting = shared.get("greeting").unwrap_or_default();
    Ok(greeting)
  }

  fn exec(&self, prep_result: Value) -> Result<Value> {
    let message = prep_result.as_str().unwrap_or("");
    let processed = format!("Processed: {}", message);
    Ok(Value::String(processed))
  }

  fn post(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    shared.insert("final_result".to_string(), exec_result);
    Ok(None) // End the flow
  }
}

fn main() -> Result<()> {
  println!("🚀 AgentFlow Hello World Example");

  // Create shared state
  let shared = SharedState::new();
  shared.insert(
    "user_name".to_string(),
    Value::String("Rust Developer".to_string()),
  );

  // Create nodes
  let greeting_node = GreetingNode {
    name: "greeting".to_string(),
  };
  let processing_node = ProcessingNode;

  // Create flow
  let mut flow = Flow::new(Box::new(greeting_node));
  flow.add_node("process".to_string(), Box::new(processing_node));

  // Run the flow
  match flow.run(&shared) {
    Ok(_) => {
      if let Some(result) = shared.get("final_result") {
        println!("✅ Result: {}", result.as_str().unwrap_or(""));
      }
    }
    Err(e) => {
      println!("❌ Error: {}", e);
    }
  }

  Ok(())
}
