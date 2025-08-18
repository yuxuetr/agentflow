//! # Multimodal LLM Demo
//!
//! This example demonstrates how to use multimodal LLMs (text + image)
//! with AgentFlow LLM integration, specifically using StepFun's vision model.
//!
//! Based on the StepFun API example for step-1o-turbo-vision model.

use agentflow_llm::{AgentFlow, MultimodalMessage, Result};

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🎨 AgentFlow Multimodal LLM Demo");
  println!("Using StepFun's step-1o-turbo-vision model for image analysis\n");

  // Initialize AgentFlow LLM system
  AgentFlow::init().await?;

  // Method 1: Using the simple text_and_image shortcut
  println!("📸 Method 1: Simple text + image analysis");
  let response1 = demo_simple_multimodal().await?;
  println!("Response: {}\n", response1);

  // Method 2: Using the builder pattern for more complex messages
  println!("🎭 Method 2: Complex multimodal message with system prompt");
  let response2 = demo_complex_multimodal().await?;
  println!("Response: {}\n", response2);

  // Method 3: Using multiple images in one request
  println!("🖼️  Method 3: Multiple images analysis");
  let response3 = demo_multiple_images().await?;
  println!("Response: {}\n", response3);

  // Method 4: Integration with agent flows (if AgentFlow nodes are available)
  println!("🤖 Method 4: Multimodal in Agent Flow");
  let response4 = demo_agent_flow_multimodal().await?;
  println!("Response: {}\n", response4);

  println!("✨ Demo completed successfully!");
  Ok(())
}

/// Demo 1: Simple text + image analysis (recreating the Python example)
async fn demo_simple_multimodal() -> Result<String> {
  let response = AgentFlow::model("step-1o-turbo-vision")
    .text_and_image(
      "Describe this image in elegant language",
      "https://www.stepfun.com/assets/section-1-CTe4nZiO.webp",
    )
    .temperature(0.7)
    .execute()
    .await?;

  Ok(response)
}

/// Demo 2: Complex multimodal message with system prompt and structured message
async fn demo_complex_multimodal() -> Result<String> {
  // Create a system message
  let system_message = MultimodalMessage::system()
    .add_text("You are an AI assistant provided by StepFun. In addition to being proficient in Chinese, English, and multiple other languages, you can also provide accurate textual descriptions of images while ensuring user data security. You should respond quickly and precisely to user queries while rejecting content related to pornography, gambling, drugs, violence, or terrorism.")
    .build();

  // Create a user message with text and image
  let user_message = MultimodalMessage::user()
    .add_text("Describe this image in elegant language")
    .add_image_url_with_detail(
      "https://www.stepfun.com/assets/section-1-CTe4nZiO.webp",
      "high", // Request high detail analysis
    )
    .build();

  let response = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_messages(vec![system_message, user_message])
    .temperature(0.7)
    .max_tokens(1000)
    .execute()
    .await?;

  Ok(response)
}

/// Demo 3: Multiple images analysis
async fn demo_multiple_images() -> Result<String> {
  let message = MultimodalMessage::user()
    .add_text("Compare these images and describe the similarities and differences:")
    .add_image_url("https://www.stepfun.com/assets/section-1-CTe4nZiO.webp")
    .add_image_url("https://www.stepfun.com/assets/hero-image.webp")
    .build();

  let response = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .temperature(0.8)
    .max_tokens(1500)
    .execute()
    .await?;

  Ok(response)
}

/// Demo 4: Using multimodal in agent flows
async fn demo_agent_flow_multimodal() -> Result<String> {
  use agentflow_core::{AgentFlowError, AsyncFlow, AsyncNode, SharedState};
  use async_trait::async_trait;
  use serde_json::Value;

  // Create a custom multimodal agent node
  struct MultimodalAgentNode {
    node_id: String,
    model_name: String,
    next_node: Option<String>,
  }

  impl MultimodalAgentNode {
    pub fn new(node_id: &str, model_name: &str) -> Self {
      Self {
        node_id: node_id.to_string(),
        model_name: model_name.to_string(),
        next_node: None,
      }
    }

    pub fn with_next_node(mut self, next_node: &str) -> Self {
      self.next_node = Some(next_node.to_string());
      self
    }
  }

  #[async_trait]
  impl AsyncNode for MultimodalAgentNode {
    async fn prep_async(&self, shared: &SharedState) -> agentflow_core::Result<Value> {
      // Get image URL and prompt from shared state
      let image_url = shared
        .get("image_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://www.stepfun.com/assets/section-1-CTe4nZiO.webp");

      let user_prompt = shared
        .get("user_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("Analyze this image");

      println!(
        "🔍 [{}] Preparing multimodal request: {}",
        self.node_id, user_prompt
      );

      Ok(serde_json::json!({
        "image_url": image_url,
        "prompt": user_prompt
      }))
    }

    async fn exec_async(&self, prep_result: Value) -> agentflow_core::Result<Value> {
      let image_url = prep_result["image_url"].as_str().unwrap();
      let prompt = prep_result["prompt"].as_str().unwrap();

      println!("🎨 [{}] Executing multimodal LLM request", self.node_id);

      // Create multimodal message
      let message = MultimodalMessage::text_and_image("user", prompt, image_url);

      // Execute the request
      match AgentFlow::model(&self.model_name)
        .multimodal_prompt(message)
        .temperature(0.7)
        .execute()
        .await
      {
        Ok(response) => {
          println!("✅ [{}] Multimodal LLM response received", self.node_id);
          Ok(serde_json::json!({
            "response": response,
            "model": self.model_name,
            "success": true
          }))
        }
        Err(e) => {
          println!("❌ [{}] Multimodal LLM failed: {}", self.node_id, e);
          Err(AgentFlowError::AsyncExecutionError {
            message: format!("Multimodal LLM execution failed: {}", e),
          })
        }
      }
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      exec_result: Value,
    ) -> agentflow_core::Result<Option<String>> {
      // Store the response in shared state
      if let Some(response) = exec_result.get("response") {
        shared.insert(format!("{}_response", self.node_id), response.clone());
        shared.insert("last_multimodal_result".to_string(), exec_result);
      }

      println!("💾 [{}] Results stored in shared state", self.node_id);
      Ok(self.next_node.clone())
    }
  }

  // Create and run the flow
  let multimodal_node = MultimodalAgentNode::new("multimodal_analyzer", "step-1o-turbo-vision");
  let mut flow = AsyncFlow::new(Box::new(multimodal_node));

  // Set up shared state
  let shared = SharedState::new();
  shared.insert(
    "image_url".to_string(),
    Value::String("https://www.stepfun.com/assets/section-1-CTe4nZiO.webp".to_string()),
  );
  shared.insert(
    "user_prompt".to_string(),
    Value::String("Describe this architectural image in poetic language".to_string()),
  );

  // Execute the flow
  let result = flow.run_async(&shared).await?;

  // Extract the response
  let response = shared
    .get("multimodal_analyzer_response")
    .and_then(|v| v.as_str())
    .unwrap_or("No response generated")
    .to_string();

  Ok(response)
}

/// Demo with base64 encoded image (useful for local images)
#[allow(dead_code)]
async fn demo_base64_image() -> Result<String> {
  // This would be used for local images converted to base64
  let base64_image = "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEAAAAA..."; // Truncated for example

  let message = MultimodalMessage::user()
    .add_text("What do you see in this image?")
    .add_image_data(base64_image, "image/jpeg")
    .build();

  let response = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .execute()
    .await?;

  Ok(response)
}

/// Demo with streaming multimodal response
#[allow(dead_code)]
async fn demo_streaming_multimodal() -> Result<String> {
  use futures::StreamExt;

  let message = MultimodalMessage::text_and_image(
    "user",
    "Provide a detailed analysis of this image",
    "https://www.stepfun.com/assets/section-1-CTe4nZiO.webp",
  );

  let mut stream = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .temperature(0.7)
    .execute_streaming()
    .await?;

  let mut full_response = String::new();
  println!("📡 Streaming response:");

  while let Some(chunk) = stream.next_chunk().await? {
    print!("{}", chunk.content);
    full_response.push_str(&chunk.content);

    if chunk.is_final {
      break;
    }
  }
  println!("\n");

  Ok(full_response)
}
