// Multimodal Agent Flow Example
// This example demonstrates how to integrate multimodal LLM capabilities
// within AgentFlow, processing images and text in automated workflows

use agentflow_core::{AgentFlowError, AsyncFlow, AsyncNode, Result, SharedState};
use agentflow_llm::{AgentFlow as LLMAgentFlow, MultimodalMessage};
use async_trait::async_trait;
use serde_json::Value;

// Multimodal image analysis node that uses StepFun's vision model
pub struct MultimodalAnalyzerNode {
  node_id: String,
  model_name: String,
  analysis_type: String, // "description", "analysis", "comparison"
  next_node: Option<String>,
}

impl MultimodalAnalyzerNode {
  pub fn new(node_id: &str, model_name: &str, analysis_type: &str) -> Self {
    Self {
      node_id: node_id.to_string(),
      model_name: model_name.to_string(),
      analysis_type: analysis_type.to_string(),
      next_node: None,
    }
  }

  pub fn with_next_node(mut self, next_node: &str) -> Self {
    self.next_node = Some(next_node.to_string());
    self
  }

  fn create_prompt(&self, analysis_type: &str) -> String {
    match analysis_type {
      "description" => "Provide a detailed and elegant description of this image, focusing on visual elements, composition, and atmosphere.".to_string(),
      "analysis" => "Analyze this image in detail, including architectural elements, design principles, lighting, and cultural context.".to_string(),
      "comparison" => "Compare the visual elements and characteristics you can observe in the images provided.".to_string(),
      "business" => "Analyze this image from a business perspective, identifying potential opportunities, market insights, or commercial applications.".to_string(),
      _ => "Describe what you see in this image.".to_string(),
    }
  }
}

#[async_trait]
impl AsyncNode for MultimodalAnalyzerNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Get image URLs and context from shared state
    let image_urls: Vec<String> = if let Some(Value::Array(urls)) = shared.get("image_urls") {
      urls.iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.to_string())
        .collect()
    } else if let Some(Value::String(url)) = shared.get("image_url") {
      vec![url.clone()]
    } else {
      return Err(AgentFlowError::AsyncExecutionError {
        message: "No image URLs found in shared state".to_string(),
      });
    };

    let context = shared
      .get("context")
      .and_then(|v| v.as_str())
      .unwrap_or("")
      .to_string();

    let prompt = self.create_prompt(&self.analysis_type);

    println!(
      "🔍 [{}] Preparing multimodal analysis: {} images, type: {}",
      self.node_id,
      image_urls.len(),
      self.analysis_type
    );

    Ok(serde_json::json!({
      "image_urls": image_urls,
      "prompt": prompt,
      "context": context,
      "analysis_type": self.analysis_type
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let image_urls: Vec<String> = prep_result["image_urls"]
      .as_array()
      .unwrap()
      .iter()
      .filter_map(|v| v.as_str())
      .map(|s| s.to_string())
      .collect();

    let prompt = prep_result["prompt"].as_str().unwrap();
    let context = prep_result["context"].as_str().unwrap_or("");

    println!(
      "🎨 [{}] Executing multimodal LLM analysis with {} images",
      self.node_id,
      image_urls.len()
    );

    // Initialize the LLM system
    match LLMAgentFlow::init().await {
      Ok(()) => println!("✅ [{}] LLM system initialized", self.node_id),
      Err(e) => {
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("LLM initialization failed: {}", e),
        });
      }
    }

    // Create multimodal message
    let mut message_builder = MultimodalMessage::user().add_text(prompt);

    // Add context if provided
    if !context.is_empty() {
      message_builder = message_builder.add_text(&format!("\n\nContext: {}", context));
    }

    // Add all images
    for url in &image_urls {
      message_builder = message_builder.add_image_url_with_detail(url, "high");
    }

    let message = message_builder.build();

    // Execute multimodal LLM request
    match LLMAgentFlow::model(&self.model_name)
      .multimodal_prompt(message)
      .temperature(0.7)
      .max_tokens(2000)
      .execute()
      .await
    {
      Ok(response) => {
        println!("✅ [{}] Multimodal analysis completed", self.node_id);
        Ok(serde_json::json!({
          "analysis_result": response,
          "model_used": self.model_name,
          "analysis_type": self.analysis_type,
          "images_analyzed": image_urls.len(),
          "node_id": self.node_id
        }))
      }
      Err(e) => {
        println!("❌ [{}] Multimodal analysis failed: {}", self.node_id, e);
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
  ) -> Result<Option<String>> {
    // Store the analysis result in shared state
    if let Some(analysis) = exec_result.get("analysis_result") {
      shared.insert(format!("{}_analysis", self.node_id), analysis.clone());
      println!("💾 [{}] Analysis stored in shared state", self.node_id);
    }

    // Store execution metadata
    shared.insert(format!("{}_executed", self.node_id), Value::Bool(true));
    shared.insert("last_multimodal_result".to_string(), exec_result);

    println!("✨ [{}] Multimodal analysis post-processing completed", self.node_id);
    Ok(self.next_node.clone())
  }
}

// Text summarization node that processes multimodal analysis results
pub struct SummarizerNode {
  node_id: String,
  summary_style: String, // "brief", "detailed", "technical", "creative"
  next_node: Option<String>,
}

impl SummarizerNode {
  pub fn new(node_id: &str, summary_style: &str) -> Self {
    Self {
      node_id: node_id.to_string(),
      summary_style: summary_style.to_string(),
      next_node: None,
    }
  }

  pub fn with_next_node(mut self, next_node: &str) -> Self {
    self.next_node = Some(next_node.to_string());
    self
  }
}

#[async_trait]
impl AsyncNode for SummarizerNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Find all analysis results in shared state
    let mut analyses = Vec::new();
    
    // Look for any keys ending with "_analysis"
    for (key, value) in shared.iter() {
      if key.ends_with("_analysis") {
        if let Some(analysis_text) = value.as_str() {
          analyses.push(analysis_text.to_string());
        }
      }
    }

    if analyses.is_empty() {
      return Err(AgentFlowError::AsyncExecutionError {
        message: "No analysis results found in shared state".to_string(),
      });
    }

    Ok(serde_json::json!({
      "analyses": analyses,
      "summary_style": self.summary_style
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let analyses: Vec<String> = prep_result["analyses"]
      .as_array()
      .unwrap()
      .iter()
      .filter_map(|v| v.as_str())
      .map(|s| s.to_string())
      .collect();

    let combined_analysis = analyses.join("\n\n---\n\n");

    let summary_prompt = match self.summary_style.as_str() {
      "brief" => "Provide a brief, 2-3 sentence summary of the key insights from this analysis:",
      "detailed" => "Create a comprehensive summary that captures all important details and insights from this analysis:",
      "technical" => "Generate a technical summary focusing on specific details, measurements, and professional observations:",
      "creative" => "Write a creative, engaging summary that captures the essence and mood of this analysis:",
      _ => "Summarize the following analysis:",
    };

    let full_prompt = format!("{}\n\n{}", summary_prompt, combined_analysis);

    println!("📝 [{}] Creating {} summary of {} analyses", self.node_id, self.summary_style, analyses.len());

    // Use a text model for summarization
    match LLMAgentFlow::model("step-1-8k") // Use a text-only model for summarization
      .prompt(&full_prompt)
      .temperature(0.5)
      .max_tokens(1000)
      .execute()
      .await
    {
      Ok(summary) => {
        println!("✅ [{}] Summary generated successfully", self.node_id);
        Ok(serde_json::json!({
          "summary": summary,
          "summary_style": self.summary_style,
          "analyses_count": analyses.len()
        }))
      }
      Err(e) => {
        println!("❌ [{}] Summary generation failed: {}", self.node_id, e);
        Err(AgentFlowError::AsyncExecutionError {
          message: format!("Summary generation failed: {}", e),
        })
      }
    }
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Store the summary
    if let Some(summary) = exec_result.get("summary") {
      shared.insert("final_summary".to_string(), summary.clone());
      shared.insert(format!("{}_result", self.node_id), exec_result);
      println!("📋 [{}] Summary stored in shared state", self.node_id);
    }

    Ok(self.next_node.clone())
  }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🌟 Multimodal Agent Flow Demo");
  println!("This demonstrates automated image analysis workflows using StepFun's vision models\n");

  // Create the multimodal analysis nodes
  let image_analyzer = MultimodalAnalyzerNode::new(
    "primary_analyzer",
    "step-1o-turbo-vision",
    "analysis"
  ).with_next_node("detail_analyzer");

  let detail_analyzer = MultimodalAnalyzerNode::new(
    "detail_analyzer", 
    "step-1o-turbo-vision",
    "business"
  ).with_next_node("summarizer");

  let summarizer = SummarizerNode::new("summarizer", "detailed");

  // Create the async flow
  let mut flow = AsyncFlow::new(Box::new(image_analyzer));
  flow.add_node("detail_analyzer".to_string(), Box::new(detail_analyzer));
  flow.add_node("summarizer".to_string(), Box::new(summarizer));

  // Enable observability
  flow.enable_tracing("multimodal_agent_flow".to_string());

  // Set up shared state with image URLs and context
  let shared = SharedState::new();
  
  // Example with StepFun's website image
  shared.insert(
    "image_url".to_string(),
    Value::String("https://www.stepfun.com/assets/section-1-CTe4nZiO.webp".to_string()),
  );

  shared.insert(
    "context".to_string(),
    Value::String("This is an architectural/building image from StepFun's website. Please analyze from both aesthetic and business perspectives.".to_string()),
  );

  println!("🚀 Starting the multimodal agent flow...\n");

  // Execute the flow
  match flow.run_async(&shared).await {
    Ok(final_result) => {
      println!("\n🎉 Multimodal flow completed successfully!");
      println!("Final result: {}", final_result);

      // Display the journey through the flow
      println!("\n📋 Flow execution summary:");
      
      // Show primary analysis
      if shared.contains_key("primary_analyzer_executed") {
        println!("✅ Primary image analysis completed");
        if let Some(analysis) = shared.get("primary_analyzer_analysis") {
          println!("   Analysis preview: {}...", 
            analysis.as_str().unwrap_or("").chars().take(100).collect::<String>()
          );
        }
      }

      // Show detail analysis  
      if shared.contains_key("detail_analyzer_executed") {
        println!("✅ Detailed business analysis completed");
        if let Some(analysis) = shared.get("detail_analyzer_analysis") {
          println!("   Business analysis preview: {}...", 
            analysis.as_str().unwrap_or("").chars().take(100).collect::<String>()
          );
        }
      }

      // Show final summary
      if shared.contains_key("final_summary") {
        println!("✅ Final summary generated");
        if let Some(summary) = shared.get("final_summary") {
          println!("\n🔍 FINAL SUMMARY:");
          println!("{}", summary.as_str().unwrap_or("No summary available"));
        }
      }

      println!("\n💡 This demonstrates how multimodal AI can be integrated into automated workflows!");
    }
    Err(e) => {
      println!("❌ Flow execution failed: {}", e);
      return Err(e.into());
    }
  }

  println!("\n🏁 Multimodal demo completed!");
  Ok(())
}