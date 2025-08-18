use agentflow_core::ConfigWorkflowRunner;
use std::collections::HashMap;

/// Demonstration of configuration-first workflow execution
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 AgentFlow Core - Configuration-First Workflow Demo\n");

  // Load workflow from YAML configuration
  let workflow_path = "../examples/workflows/hello_world_v2.yml";
  println!("📄 Loading workflow from: {}", workflow_path);

  let runner = match ConfigWorkflowRunner::from_file(workflow_path).await {
    Ok(runner) => {
      println!("✅ Workflow loaded and validated successfully\n");
      runner
    }
    Err(e) => {
      println!("❌ Failed to load workflow: {}", e);
      return Err(e);
    }
  };

  // Prepare inputs
  let mut inputs = HashMap::new();
  inputs.insert("question".to_string(), "What is 2 + 2?".to_string());
  inputs.insert("model".to_string(), "step-2-mini".to_string());
  inputs.insert("temperature".to_string(), "0.8".to_string());

  println!("📝 Workflow inputs:");
  for (key, value) in &inputs {
    println!("   {}: {}", key, value);
  }
  println!();

  // Execute the workflow
  match runner.run(inputs).await {
    Ok(outputs) => {
      println!("\n🎯 Workflow outputs:");
      for (key, value) in outputs {
        println!("   {}: {:?}", key, value);
      }
      println!("\n✅ Configuration-first workflow completed successfully!");
    }
    Err(e) => {
      println!("\n❌ Workflow execution failed: {}", e);
      return Err(e);
    }
  }

  println!("\n🔄 Comparison with code-first approach:");
  println!("✅ Same functionality, cleaner separation");
  println!("✅ Configuration can be validated and modified without recompiling");
  println!("✅ Non-technical users can create workflows");
  println!("✅ Code-first nodes provide the implementation foundation");

  Ok(())
}
