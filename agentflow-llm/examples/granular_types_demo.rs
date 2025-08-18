//! # Granular Model Types Demo
//!
//! This example demonstrates the new granular model type system that provides
//! specific input/output classifications for different model capabilities.

use agentflow_llm::{AgentFlow, InputType, ModelType, MultimodalMessage, OutputType, Result};

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🎯 AgentFlow Granular Model Types Demo");
  println!("Demonstrating automatic model capability detection and validation\n");

  // Initialize AgentFlow LLM system
  AgentFlow::init().await?;

  // Demo 1: Text-only model usage
  println!("📝 Demo 1: Text-only model (step-1-8k)");
  demo_text_model().await?;
  println!();

  // Demo 2: Image understanding model
  println!("🖼️  Demo 2: Image understanding model (step-1o-turbo-vision)");
  demo_image_understanding().await?;
  println!();

  // Demo 3: Model capability inspection
  println!("🔍 Demo 3: Model capability inspection");
  demo_capability_inspection().await?;
  println!();

  // Demo 4: Request validation
  println!("⚡ Demo 4: Automatic request validation");
  demo_request_validation().await?;
  println!();

  // Demo 5: Model type comparison
  println!("📊 Demo 5: Model type comparison");
  demo_model_comparison().await?;

  println!("\n✨ Demo completed successfully!");
  Ok(())
}

/// Demo using a text-only model with automatic type detection
async fn demo_text_model() -> Result<String> {
  // The system automatically detects this is a text-only model
  let response = AgentFlow::model("step-1-8k")
    .prompt("Explain what granular model types mean in AI systems")
    .temperature(0.7)
    .execute()
    .await?;

  println!(
    "Text model response: {}",
    response.chars().take(100).collect::<String>()
  );

  Ok(response)
}

/// Demo using an image understanding model
async fn demo_image_understanding() -> Result<String> {
  // The system detects this model supports image understanding (imageunderstand type)
  let message = MultimodalMessage::text_and_image(
    "user",
    "What architectural elements can you identify in this image?",
    "https://www.stepfun.com/assets/section-1-CTe4nZiO.webp",
  );

  let response = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .temperature(0.8)
    .execute()
    .await?;

  println!(
    "Vision model response: {}",
    response.chars().take(100).collect::<String>()
  );

  Ok(response)
}

/// Demo inspecting model capabilities
async fn demo_capability_inspection() -> std::result::Result<(), Box<dyn std::error::Error>> {
  use agentflow_llm::ModelRegistry;

  let registry = ModelRegistry::global();

  // Inspect different model types
  let models_to_check = vec![
    ("step-1-8k", "Text model"),
    ("step-1o-turbo-vision", "Vision model"),
    ("step-tts-mini", "Text-to-Speech model"),
    ("step-asr", "Speech recognition model"),
  ];

  for (model_name, description) in models_to_check {
    if let Ok(config) = registry.get_model(model_name) {
      let granular_type = config.granular_type();
      let capabilities = config.get_capabilities();

      println!("🤖 {} ({}):", description, model_name);
      println!("   Type: {:?}", granular_type);
      println!("   Description: {}", granular_type.description());
      println!("   Supports streaming: {}", capabilities.supports_streaming);
      println!("   Supports tools: {}", capabilities.supports_tools);
      println!("   Is multimodal: {}", capabilities.is_multimodal());
      println!("   Primary output: {:?}", capabilities.expected_output());

      let supported_inputs: Vec<String> = granular_type
        .supported_inputs()
        .iter()
        .map(|input| format!("{:?}", input))
        .collect();
      println!("   Supported inputs: [{}]", supported_inputs.join(", "));

      let use_cases = granular_type.use_cases();
      println!("   Use cases: {}", use_cases.join(", "));
      println!();
    }
  }

  Ok(())
}

/// Demo automatic request validation
async fn demo_request_validation() -> std::result::Result<(), Box<dyn std::error::Error>> {
  use agentflow_llm::ModelRegistry;

  let registry = ModelRegistry::global();

  println!("Testing request validation:");

  // Test 1: Valid text request to text model
  if let Ok(config) = registry.get_model("step-1-8k") {
    match config.validate_request(true, false, false, false, false, false) {
      Ok(_) => println!("✅ Text request to text model: VALID"),
      Err(e) => println!("❌ Text request to text model: {}", e),
    }
  }

  // Test 2: Image request to text-only model (should fail)
  if let Ok(config) = registry.get_model("step-1-8k") {
    match config.validate_request(true, true, false, false, false, false) {
      Ok(_) => println!("❌ Image request to text model: VALID (unexpected)"),
      Err(e) => println!("✅ Image request to text model: INVALID ({})", e),
    }
  }

  // Test 3: Image request to vision model (should pass)
  if let Ok(config) = registry.get_model("step-1o-turbo-vision") {
    match config.validate_request(true, true, false, false, false, false) {
      Ok(_) => println!("✅ Image request to vision model: VALID"),
      Err(e) => println!("❌ Image request to vision model: {}", e),
    }
  }

  // Test 4: Streaming to non-streaming model
  if let Ok(config) = registry.get_model("step-tts-mini") {
    match config.validate_request(true, false, false, false, true, false) {
      Ok(_) => println!("❌ Streaming to TTS model: VALID (unexpected)"),
      Err(e) => println!("✅ Streaming to TTS model: INVALID ({})", e),
    }
  }

  Ok(())
}

/// Demo comparing different model types
async fn demo_model_comparison() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("Model type capabilities comparison:\n");

  let types_to_compare = vec![
    ModelType::Text,
    ModelType::ImageUnderstand,
    ModelType::Text2Image,
    ModelType::Tts,
    ModelType::Asr,
    ModelType::CodeGen,
    ModelType::FunctionCalling,
  ];

  println!("| Type | Multimodal | Streaming | Tools | Input Types | Output |");
  println!("|------|------------|-----------|-------|-------------|---------|");

  for model_type in types_to_compare {
    let is_multimodal = model_type.is_multimodal();
    let supports_streaming = model_type.supports_streaming();
    let supports_tools = model_type.supports_tools();
    let primary_output = model_type.primary_output();

    let input_types: Vec<String> = model_type
      .supported_inputs()
      .iter()
      .map(|t| format!("{:?}", t))
      .collect();

    println!(
      "| {:?} | {} | {} | {} | {} | {:?} |",
      model_type,
      if is_multimodal { "✅" } else { "❌" },
      if supports_streaming { "✅" } else { "❌" },
      if supports_tools { "✅" } else { "❌" },
      input_types.join("+"),
      primary_output
    );
  }

  println!("\nDetailed capabilities:");
  for model_type in &[
    ModelType::ImageUnderstand,
    ModelType::Text2Image,
    ModelType::Tts,
  ] {
    println!("\n🎯 {:?}:", model_type);
    println!("   Description: {}", model_type.description());
    println!("   Use cases: {}", model_type.use_cases().join(", "));
  }

  Ok(())
}

/// Demo error handling with granular types
#[allow(dead_code)]
async fn demo_error_handling() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🚨 Demo: Error handling with model validation");

  // Try to use an image with a text-only model
  let message = MultimodalMessage::text_and_image(
    "user",
    "Describe this image",
    "https://example.com/image.jpg",
  );

  match AgentFlow::model("step-1-8k") // Text-only model
    .multimodal_prompt(message)
    .execute()
    .await
  {
    Ok(_) => println!("❌ Unexpected success with image on text model"),
    Err(e) => println!("✅ Expected error: {}", e),
  }

  Ok(())
}

/// Demo streaming validation
#[allow(dead_code)]
async fn demo_streaming_validation() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("📡 Demo: Streaming validation");

  // Try streaming with a model that doesn't support it
  match AgentFlow::model("step-asr") // ASR typically doesn't support streaming
    .prompt("Test")
    .execute_streaming()
    .await
  {
    Ok(_) => println!("❌ Unexpected streaming success"),
    Err(e) => println!("✅ Expected streaming error: {}", e),
  }

  Ok(())
}
