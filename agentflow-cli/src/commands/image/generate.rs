use agentflow_llm::{AgentFlow, Text2ImageBuilder, ImageGenerationResponse};
use anyhow::Result;
use std::env;
use tokio::fs;
use base64::{engine::general_purpose, Engine as _};

pub async fn execute(
  prompt: String,
  model: Option<String>,
  size: String,
  output: String,
  format: String,
  steps: u32,
  cfg_scale: f32,
  seed: Option<u64>,
  strength: Option<f32>,
  input_image: Option<String>,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-1x-medium".to_string());
  
  // Get API key from environment
  let api_key = env::var("STEPFUN_API_KEY")
    .or_else(|_| env::var("STEP_API_KEY"))
    .map_err(|_| anyhow::anyhow!("STEPFUN_API_KEY or STEP_API_KEY environment variable must be set"))?;

  println!("🎨 AgentFlow Image Generation");
  println!("Model: {}", model);
  println!("Prompt: {}", prompt);
  println!("Size: {}", size);
  println!("Format: {}", format);
  println!("Output: {}", output);
  if let Some(seed) = seed {
    println!("Seed: {}", seed);
  }
  if let Some(input) = &input_image {
    println!("Input image: {}", input);
  }
  println!();

  // Create StepFun specialized client directly (no need for general AgentFlow init)
  println!("📡 Creating StepFun client...");
  let stepfun_client = AgentFlow::stepfun_client(&api_key).await?;

  // Build image generation request
  println!("🖼️  Building image generation request...");
  let mut request_builder = Text2ImageBuilder::new(&model, &prompt)
    .size(&size)
    .response_format(&format)
    .steps(steps)
    .cfg_scale(cfg_scale);

  if let Some(seed) = seed {
    request_builder = request_builder.seed(seed as i32);
  }

  // Note: strength and input_image parameters are for image-to-image generation
  // which requires a separate API endpoint not yet implemented in the Text2ImageBuilder
  if strength.is_some() || input_image.is_some() {
    println!("⚠️  Warning: image-to-image generation (strength, input_image) not yet implemented in Text2ImageBuilder");
    println!("    Only text-to-image generation is currently supported");
  }

  let request = request_builder.build();

  // Generate image with timeout and retry logic
  println!("🚀 Generating image...");
  let start_time = std::time::Instant::now();
  
  let response: ImageGenerationResponse = match tokio::time::timeout(
    std::time::Duration::from_secs(120), // 2 minute timeout
    stepfun_client.text_to_image(request)
  ).await {
    Ok(result) => result.map_err(|e| {
      eprintln!("Error: Internal LLM error: {}", e);
      anyhow::anyhow!("Image generation failed: {}", e)
    })?,
    Err(_) => {
      return Err(anyhow::anyhow!("Image generation timed out after 2 minutes. This may be due to:\n  - Network connectivity issues\n  - API server overload\n  - Complex prompt requiring more processing time\n\nTry again with a simpler prompt or check your internet connection."));
    }
  };
  
  let duration = start_time.elapsed();
  println!("✅ Image generated in {:?}", duration);
  println!();

  // Save image
  println!("💾 Saving image to: {}", output);
  
  if let Some(image_data) = response.data.first() {
    match format.as_str() {
      "b64_json" => {
        if let Some(b64_data) = &image_data.b64_json {
          let image_bytes = general_purpose::STANDARD.decode(b64_data)?;
          fs::write(&output, &image_bytes).await?;
          println!("✅ Image saved as base64 data ({} bytes)", image_bytes.len());
        } else {
          return Err(anyhow::anyhow!("No base64 image data received"));
        }
      }
      "url" => {
        if let Some(url) = &image_data.url {
          // For URL format, we'd need to download the image
          // For now, just save the URL to a text file
          let url_output = format!("{}.url", output);
          fs::write(&url_output, url).await?;
          println!("✅ Image URL saved to: {}", url_output);
          println!("🔗 Image URL: {}", url);
        } else {
          return Err(anyhow::anyhow!("No image URL received"));
        }
      }
      _ => {
        return Err(anyhow::anyhow!("Unsupported format: {}", format));
      }
    }
  } else {
    return Err(anyhow::anyhow!("No image data received in response"));
  }

  // Display generation details
  if let Some(image_data) = response.data.first() {
    println!("🎯 Generation seed: {}", image_data.seed);
    println!("✨ Status: {}", image_data.finish_reason);
  }

  println!("🎉 Image generation completed successfully!");
  Ok(())
}