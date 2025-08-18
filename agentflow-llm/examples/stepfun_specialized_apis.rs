//! # StepFun Specialized APIs Demo
//!
//! This example demonstrates the comprehensive StepFun specialized API support
//! including text2image, image2image, imageedit, TTS, ASR, and voice cloning.

use agentflow_llm::{
  ASRRequest, AgentFlow, Image2ImageRequest, ImageEditRequest, Result, StepFunSpecializedClient,
  TTSBuilder, Text2ImageBuilder, Text2ImageRequest, VoiceCloningRequest,
};

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🎨 AgentFlow StepFun Specialized APIs Demo");
  println!("Demonstrating image generation, TTS, ASR, and voice cloning capabilities\n");

  // Get API key from environment
  let api_key =
    std::env::var("STEPFUN_API_KEY").expect("STEPFUN_API_KEY environment variable must be set");

  // Create StepFun specialized client
  let stepfun_client = AgentFlow::stepfun_client(&api_key).await?;

  // Demo 1: Text-to-Image Generation
  println!("🖼️  Demo 1: Text-to-Image Generation");
  demo_text_to_image(&stepfun_client).await?;
  println!();

  // Demo 2: Image-to-Image Transformation
  println!("🔄 Demo 2: Image-to-Image Transformation");
  demo_image_to_image(&stepfun_client).await?;
  println!();

  // Demo 3: Image Editing with Text Instructions
  println!("✏️  Demo 3: Image Editing with Text Instructions");
  demo_image_editing(&stepfun_client).await?;
  println!();

  // Demo 4: Text-to-Speech
  println!("🔊 Demo 4: Text-to-Speech Synthesis");
  demo_text_to_speech(&stepfun_client).await?;
  println!();

  // Demo 5: Speech Recognition (ASR)
  println!("🎤 Demo 5: Automatic Speech Recognition");
  demo_speech_recognition(&stepfun_client).await?;
  println!();

  // Demo 6: Voice Cloning
  println!("👥 Demo 6: Voice Cloning");
  demo_voice_cloning(&stepfun_client).await?;
  println!();

  // Demo 7: Voice Management
  println!("📋 Demo 7: Voice Management");
  demo_voice_management(&stepfun_client).await?;

  println!("\n✨ All StepFun specialized API demos completed successfully!");
  Ok(())
}

/// Demo text-to-image generation with various parameters
async fn demo_text_to_image(client: &StepFunSpecializedClient) -> Result<()> {
  // Basic text-to-image generation
  let basic_request = Text2ImageBuilder::new(
    "step-1x-medium",
    "A majestic mountain landscape at sunset with golden clouds",
  )
  .size("1024x1024")
  .response_format("b64_json")
  .build();

  println!("Generating basic image...");
  let response = client.text_to_image(basic_request).await?;
  println!("✅ Generated {} image(s)", response.data.len());

  // Advanced generation with style reference
  let advanced_request = Text2ImageBuilder::new(
    "step-1x-medium",
    "A futuristic cityscape with neon lights and flying cars",
  )
  .size("1280x800")
  .response_format("b64_json")
  .seed(42)
  .steps(50)
  .cfg_scale(7.5)
  .style_reference("https://example.com/reference.jpg", Some(0.8))
  .build();

  println!("Generating advanced image with style reference...");
  let advanced_response = client.text_to_image(advanced_request).await?;
  println!(
    "✅ Generated {} styled image(s) with seed {}",
    advanced_response.data.len(),
    advanced_response.data.first().map(|d| d.seed).unwrap_or(0)
  );

  Ok(())
}

/// Demo image-to-image transformation
async fn demo_image_to_image(client: &StepFunSpecializedClient) -> Result<()> {
  let request = Image2ImageRequest {
    model: "step-1x-medium".to_string(),
    prompt: "Transform this image into a watercolor painting style".to_string(),
    source_url: "https://example.com/source-image.jpg".to_string(),
    source_weight: 0.7,
    size: Some("1024x1024".to_string()),
    response_format: Some("b64_json".to_string()),
    seed: Some(123),
    steps: Some(40),
    cfg_scale: Some(6.0),
    n: None,
  };

  println!("Transforming image with prompt: '{}'", request.prompt);
  let response = client.image_to_image(request).await?;
  println!("✅ Transformed {} image(s)", response.data.len());

  Ok(())
}

/// Demo image editing with text instructions
async fn demo_image_editing(client: &StepFunSpecializedClient) -> Result<()> {
  // In a real application, you would load actual image data
  let mock_image_data = vec![0u8; 1024]; // Mock image data

  let request = ImageEditRequest {
    model: "step-1x-medium".to_string(),
    image_data: mock_image_data,
    image_filename: "edit-source.jpg".to_string(),
    prompt: "Remove the background and make it transparent".to_string(),
    seed: Some(456),
    steps: Some(28),
    cfg_scale: Some(6.0),
    size: Some("1024x1024".to_string()),
    response_format: Some("b64_json".to_string()),
  };

  println!("Editing image with instruction: '{}'", request.prompt);
  let response = client.edit_image(request).await?;
  println!("✅ Edited {} image(s)", response.data.len());

  Ok(())
}

/// Demo text-to-speech synthesis with various voice options
async fn demo_text_to_speech(client: &StepFunSpecializedClient) -> Result<()> {
  // Basic TTS
  let basic_request = TTSBuilder::new(
    "step-tts-mini",
    "Hello! This is a demonstration of StepFun's text-to-speech capabilities.",
    "default_voice",
  )
  .response_format("mp3")
  .speed(1.0)
  .build();

  println!("Synthesizing basic speech...");
  let audio_data = client.text_to_speech(basic_request).await?;
  println!("✅ Generated {} bytes of MP3 audio", audio_data.len());

  // Advanced TTS with Chinese text and emotion
  let chinese_request = TTSBuilder::new(
    "step-tts-vivid",
    "你好！欢迎使用AgentFlow的语音合成功能！",
    "chinese_voice_01",
  )
  .response_format("wav")
  .speed(1.2)
  .volume(1.5)
  .language("中文")
  .emotion("高兴")
  .style("正常")
  .sample_rate(24000)
  .build();

  println!("Synthesizing Chinese speech with emotion...");
  let chinese_audio = client.text_to_speech(chinese_request).await?;
  println!(
    "✅ Generated {} bytes of Chinese WAV audio",
    chinese_audio.len()
  );

  Ok(())
}

/// Demo automatic speech recognition
async fn demo_speech_recognition(client: &StepFunSpecializedClient) -> Result<()> {
  // In a real application, you would load actual audio data
  let mock_audio_data = vec![0u8; 4096]; // Mock audio data

  let request = ASRRequest {
    model: "step-asr".to_string(),
    response_format: "json".to_string(),
    audio_data: mock_audio_data,
    filename: "test-audio.mp3".to_string(),
  };

  println!("Transcribing audio file: {}", request.filename);
  let transcription = client.speech_to_text(request).await?;
  println!("✅ Transcription: '{}'", transcription);

  // Demo with different response formats
  let srt_request = ASRRequest {
    model: "step-asr".to_string(),
    response_format: "srt".to_string(),
    audio_data: vec![0u8; 2048],
    filename: "subtitles.wav".to_string(),
  };

  println!("Generating SRT subtitles...");
  let srt_output = client.speech_to_text(srt_request).await?;
  println!("✅ SRT subtitles generated ({} chars)", srt_output.len());

  Ok(())
}

/// Demo voice cloning from audio samples
async fn demo_voice_cloning(client: &StepFunSpecializedClient) -> Result<()> {
  let request = VoiceCloningRequest {
    model: "step-voice-clone".to_string(),
    text: "This is a test of voice cloning technology.".to_string(),
    file_id: "uploaded-audio-sample-123".to_string(), // From a previous file upload
    sample_text: Some("Hello, this is the original voice sample.".to_string()),
  };

  println!("Cloning voice from audio sample...");
  let cloned_voice = client.clone_voice(request).await?;
  println!("✅ Created voice clone with ID: {}", cloned_voice.id);

  if let Some(sample_audio) = cloned_voice.sample_audio {
    println!(
      "   Sample audio: {} characters (base64)",
      sample_audio.len()
    );
  }

  if cloned_voice.duplicated.unwrap_or(false) {
    println!("   ⚠️  This voice was detected as a duplicate of an existing voice");
  }

  Ok(())
}

/// Demo voice management and listing
async fn demo_voice_management(client: &StepFunSpecializedClient) -> Result<()> {
  println!("Listing available voices...");
  let voices = client
    .list_voices(Some(10), Some("desc".to_string()), None, None)
    .await?;

  println!(
    "✅ Found {} voices (has_more: {})",
    voices.data.len(),
    voices.has_more
  );

  for (i, voice) in voices.data.iter().enumerate() {
    println!(
      "   {}. Voice ID: {} (File ID: {}, Created: {})",
      i + 1,
      voice.id,
      voice.file_id,
      voice.created_at
    );
  }

  if let Some(first_id) = &voices.first_id {
    println!("   First voice ID: {}", first_id);
  }

  if let Some(last_id) = &voices.last_id {
    println!("   Last voice ID: {}", last_id);
  }

  Ok(())
}

/// Demo using AgentFlow convenience methods
#[allow(dead_code)]
async fn demo_convenience_methods() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Demo: AgentFlow Convenience Methods");

  let _api_key = std::env::var("STEPFUN_API_KEY")?;

  // Use convenience methods from AgentFlow
  let image_request = AgentFlow::text2image("step-1x-medium", "A serene lake at dawn")
    .size("1024x1024")
    .cfg_scale(7.0)
    .response_format("b64_json")
    .build();

  let tts_request = AgentFlow::text_to_speech("step-tts-mini", "Welcome to AgentFlow!", "default")
    .response_format("mp3")
    .speed(1.1)
    .emotion("friendly")
    .build();

  println!("✅ Built requests using AgentFlow convenience methods:");
  println!(
    "   Image request: {} -> {}",
    image_request.model, image_request.prompt
  );
  println!(
    "   TTS request: {} -> '{}'",
    tts_request.model, tts_request.input
  );

  Ok(())
}

/// Demo error handling and validation
#[allow(dead_code)]
async fn demo_error_handling() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🚨 Demo: Error Handling");

  let api_key = std::env::var("STEPFUN_API_KEY")?;
  let client = AgentFlow::stepfun_client(&api_key).await?;

  // Demo validation errors (e.g., invalid parameters)
  let invalid_request = Text2ImageRequest {
    model: "invalid-model".to_string(),
    prompt: "Test".to_string(),
    size: Some("invalid-size".to_string()),
    response_format: Some("invalid-format".to_string()),
    n: Some(0), // Invalid: must be 1 for StepFun
    seed: None,
    steps: Some(200),      // Invalid: max is 100
    cfg_scale: Some(15.0), // Invalid: max is 10
    style_reference: None,
  };

  match client.text_to_image(invalid_request).await {
    Ok(_) => println!("❌ Expected error but got success"),
    Err(e) => println!("✅ Caught expected error: {}", e),
  }

  Ok(())
}
