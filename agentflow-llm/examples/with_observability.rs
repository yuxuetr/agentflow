use agentflow_core::observability::MetricsCollector;
use agentflow_llm::{AgentFlow, LLMError};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  // Set up environment variables
  dotenvy::from_filename(".env").ok();
  dotenvy::from_filename("examples/.env").ok(); // Also try examples directory

  // Initialize the LLM system with configuration
  match AgentFlow::init_with_config("examples/models.yml").await {
    Ok(()) => println!("✅ Configuration loaded successfully"),
    Err(e) => {
      println!("❌ Failed to load configuration: {}", e);
      println!("💡 Make sure to:");
      println!("  1. Copy examples/.env.example to .env");
      println!("  2. Add your actual API keys");
      println!("  3. Ensure examples/models.yml exists");
      return Err(e);
    }
  }

  // Create a metrics collector for observability
  let metrics_collector = Arc::new(MetricsCollector::new());

  println!("=== LLM Usage with Observability ===");

  // Make several requests to different models with metrics tracking
  let models = ["gpt-4o-mini", "claude-3-haiku", "gemini-1.5-flash"];
  let prompts = [
    "What is 2+2?",
    "Name three colors.",
    "Write a haiku about code.",
  ];

  for (i, &model) in models.iter().enumerate() {
    let prompt = prompts[i % prompts.len()];

    println!("\n--- Testing {} ---", model);
    println!("Prompt: {}", prompt);

    match AgentFlow::model(model)
      .prompt(prompt)
      .temperature(0.7)
      .with_metrics(metrics_collector.clone())
      .execute()
      .await
    {
      Ok(response) => {
        println!("Response: {}", response.trim());
      }
      Err(e) => {
        println!("Error: {}", e);
      }
    }
  }

  // Test streaming with observability
  println!("\n--- Streaming with Observability ---");
  match AgentFlow::model("gpt-4o-mini")
    .prompt("Count from 1 to 5, one number per sentence.")
    .with_metrics(metrics_collector.clone())
    .execute_streaming()
    .await
  {
    Ok(mut stream) => {
      print!("Streaming: ");
      while let Some(chunk) = stream.next_chunk().await? {
        print!("{}", chunk.content);
        if chunk.is_final {
          println!("\n[Complete]");
          break;
        }
      }
    }
    Err(e) => println!("Streaming error: {}", e),
  }

  // Display collected metrics
  println!("\n=== Collected Metrics ===");

  // Print some example metrics
  for model in &models {
    if let Some(requests) = metrics_collector.get_metric(&format!("llm.{}.requests", model)) {
      println!("{} - Total requests: {}", model, requests);
    }
    if let Some(success) = metrics_collector.get_metric(&format!("llm.{}.success", model)) {
      println!("{} - Successful requests: {}", model, success);
    }
    if let Some(duration) = metrics_collector.get_metric(&format!("llm.{}.duration_ms", model)) {
      if let Some(success) = metrics_collector.get_metric(&format!("llm.{}.success", model)) {
        if success > 0.0 {
          println!("{} - Average duration: {:.2}ms", model, duration / success);
        }
      }
    }
    if let Some(tokens) = metrics_collector.get_metric(&format!("llm.{}.total_tokens", model)) {
      println!("{} - Total tokens used: {}", model, tokens);
    }
    println!();
  }

  // Display recent events
  println!("=== Recent Events ===");
  let events = metrics_collector.get_events();
  for event in events.iter().rev().take(5) {
    println!(
      "[{}] {} - {} ({}ms)",
      event.node_id,
      event.event_type,
      event.timestamp.elapsed().as_millis(),
      event.duration_ms.unwrap_or(0)
    );
  }

  Ok(())
}
