use anyhow::Result;
use std::env;
use tokio::fs;
use base64::Engine;
use reqwest;
use serde_json::{json, Value};

pub async fn execute(
  image_path: String,
  prompt: String,
  model: Option<String>,
  temperature: Option<f32>,
  max_tokens: Option<u32>,
  output: Option<String>,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-1v-8k".to_string());

  println!("👁️  AgentFlow Image Understanding");
  println!("Model: {}", model);
  println!("Image: {}", image_path);
  println!("Prompt: {}", prompt);
  if let Some(output) = &output {
    println!("Output: {}", output);
  }
  println!();

  // For StepFun vision models, we need to set up the API key environment
  // but we can't use the general model registry since StepFun models aren't configured there
  println!("🔧 Setting up vision analysis...");
  
  // Get the API key we already validated
  let api_key = env::var("STEPFUN_API_KEY")
    .or_else(|_| env::var("STEP_API_KEY"))
    .map_err(|_| anyhow::anyhow!("STEPFUN_API_KEY or STEP_API_KEY environment variable must be set"))?;

  // Check if image file exists
  if !std::path::Path::new(&image_path).exists() {
    return Err(anyhow::anyhow!("Image file not found: {}", image_path));
  }

  // Read image file as base64
  println!("📸 Reading image file...");
  let image_bytes = fs::read(&image_path).await?;
  let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
  
  // Determine image format
  let image_format = match image_path.to_lowercase().split('.').last() {
    Some("jpg") | Some("jpeg") => "image/jpeg",
    Some("png") => "image/png",
    Some("gif") => "image/gif",
    Some("webp") => "image/webp", 
    Some("bmp") => "image/bmp",
    _ => "image/jpeg", // Default fallback
  };

  println!("🖼️  Image format: {}", image_format);
  println!("💾 Image size: {} bytes", image_bytes.len());

  // Create direct HTTP request to StepFun API
  println!("🔍 Building multimodal analysis request...");
  
  let client = reqwest::Client::new();
  let request_body = json!({
    "model": model,
    "messages": [
      {
        "role": "user",
        "content": [
          {
            "type": "text",
            "text": prompt
          },
          {
            "type": "image_url",
            "image_url": {
              "url": format!("data:{};base64,{}", image_format, image_base64)
            }
          }
        ]
      }
    ],
    "temperature": temperature.unwrap_or(0.7),
    "max_tokens": max_tokens.unwrap_or(800)
  });

  // Perform image analysis
  println!("🚀 Analyzing image...");
  let start_time = std::time::Instant::now();
  
  let response = client
    .post("https://api.stepfun.com/v1/chat/completions")
    .header("Authorization", format!("Bearer {}", api_key))
    .header("Content-Type", "application/json")
    .json(&request_body)
    .send()
    .await?;

  if !response.status().is_success() {
    let status = response.status();
    let error_text = response.text().await?;
    return Err(anyhow::anyhow!("HTTP request failed: {} - {}", status, error_text));
  }

  let response_json: Value = response.json().await?;
  let response_text = response_json["choices"][0]["message"]["content"]
    .as_str()
    .unwrap_or("No response received")
    .to_string();
  
  let duration = start_time.elapsed();
  println!("✅ Analysis completed in {:?}", duration);
  println!();

  // Display results
  println!("📝 Analysis Results:");
  println!("===================");
  println!("{}", response_text);
  println!();

  // Save to file if specified
  if let Some(output_path) = output {
    println!("💾 Saving results to: {}", output_path);
    let output_content = format!("# Image Analysis Results\n\n");
    let output_content = format!("{}**Model:** {}\n", output_content, model);
    let output_content = format!("{}**Image:** {}\n", output_content, image_path);
    let output_content = format!("{}**Prompt:** {}\n\n", output_content, prompt);
    let output_content = format!("{}**Analysis:**\n{}\n", output_content, response_text);
    
    fs::write(&output_path, &output_content).await?;
    println!("✅ Results saved successfully");
  }

  println!("🎉 Image understanding completed successfully!");
  Ok(())
}