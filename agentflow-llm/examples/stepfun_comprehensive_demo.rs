use serde_json::json;
use std::env;

/// Comprehensive StepFun API demonstration covering all model types
///
/// This example demonstrates the proper usage patterns for each StepFun model category:
/// - Text models via LLMClient (chat completions)
/// - Image understanding via LLMClient (multimodal chat)
/// - Multimodal via LLMClient (enhanced chat)
/// - TTS, ASR, Image generation via StepFunSpecializedClient
///
/// Set STEP_API_KEY environment variable before running:
/// ```
/// export STEP_API_KEY="your-stepfun-api-key-here"
/// cargo run --example stepfun_comprehensive_demo
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize environment and logging
  env_logger::init();

  let api_key = env::var("STEP_API_KEY").expect("STEP_API_KEY environment variable is required");

  println!("🚀 StepFun Comprehensive API Demo - Reference Implementation");
  println!("============================================================\n");
  println!(
    "This demo shows the correct API patterns and request structures for each StepFun model type."
  );
  println!("For actual working examples, see the individual test files:\n");
  println!("  • stepfun_text_models.rs - Real text API tests");
  println!("  • stepfun_image_understanding.rs - Real image understanding tests");
  println!("  • stepfun_tts_models.rs - Real TTS API tests");
  println!("  • stepfun_asr_models.rs - Real ASR API tests");
  println!("  • stepfun_image_generation.rs - Real image generation tests\n");

  // Demo 1: Text Models (Chat Completions)
  demo_text_models(&api_key).await?;

  // Demo 2: Image Understanding
  demo_image_understanding(&api_key).await?;

  // Demo 3: Multimodal
  demo_multimodal(&api_key).await?;

  // Demo 4: Text-to-Speech (TTS)
  demo_text_to_speech(&api_key).await?;

  // Demo 5: Automatic Speech Recognition (ASR)
  demo_speech_recognition(&api_key).await?;

  // Demo 6: Image Generation
  demo_image_generation(&api_key).await?;

  // Demo 7: Image Editing
  demo_image_editing(&api_key).await?;

  // Demo 8: Voice Cloning
  demo_voice_cloning(&api_key).await?;

  println!("✅ All API patterns demonstrated successfully!");
  println!("📋 To run actual tests, use the individual example files listed above.");
  Ok(())
}

/// Demo 1: Text Models - Standard chat completions
async fn demo_text_models(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("📝 Demo 1: Text Models (Chat Completions)");
  println!("Models: step-1-8k, step-1-32k, step-2-16k, step-2-mini\n");

  // Show non-streaming request structure
  println!("📤 Non-streaming request structure:");
  let non_streaming_request = json!({
      "model": "step-2-16k",
      "messages": [
          {"role": "system", "content": "你是一个专业的Python程序员。"},
          {"role": "user", "content": "用Python写快速排序算法"}
      ],
      "max_tokens": 1000,
      "temperature": 0.7,
      "stream": false
  });
  println!("   Endpoint: POST /v1/chat/completions");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&non_streaming_request)?
  );

  // Show streaming request structure
  println!("\n📤 Streaming request structure:");
  let streaming_request = json!({
      "model": "step-1-32k",
      "messages": [
          {"role": "user", "content": "解释量子计算的基本原理"}
      ],
      "stream": true,
      "max_tokens": 800,
      "temperature": 0.8
  });
  println!("   Endpoint: POST /v1/chat/completions");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&streaming_request)?
  );
  println!("   Response: Server-Sent Events (SSE) stream");

  println!("✅ Text models API pattern demonstrated\n");
  Ok(())
}

/// Demo 2: Image Understanding - Vision models with chat completions
async fn demo_image_understanding(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🖼️  Demo 2: Image Understanding");
  println!("Models: step-1o-turbo-vision, step-1v-8k, step-1v-32k\n");

  // Show multimodal request structure
  println!("📤 Image understanding request structure:");
  let vision_request = json!({
      "model": "step-1o-turbo-vision",
      "messages": [
          {
              "role": "user",
              "content": [
                  {"type": "text", "text": "描述这张图片的内容，包括主要元素和整体构图"},
                  {"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}}
              ]
          }
      ],
      "max_tokens": 500,
      "temperature": 0.7
  });

  println!("   Endpoint: POST /v1/chat/completions");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&vision_request)?
  );

  println!("\n📝 Key features:");
  println!("   • Uses same chat completions endpoint as text models");
  println!("   • Content array format with type field");
  println!("   • Supports both image URLs and base64 encoding");
  println!("   • Compatible with streaming (add \"stream\": true)");

  println!("✅ Image understanding API pattern demonstrated\n");
  Ok(())
}

/// Demo 3: Multimodal - Advanced multimodal capabilities
async fn demo_multimodal(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎭 Demo 3: Multimodal");
  println!("Models: step-3\n");

  println!("📤 Multimodal request structure:");

  // Multimodal models use the same chat completions endpoint but with enhanced capabilities
  let multimodal_request = json!({
      "model": "step-3",
      "messages": [
          {
              "role": "system",
              "content": "你是一个具有多模态理解能力的AI助手，能够综合分析多种类型的内容。"
          },
          {
              "role": "user",
              "content": [
                  {"type": "text", "text": "分析这些图片的差异，比较它们的特点"},
                  {"type": "image_url", "image_url": {"url": "https://example.com/image1.jpg"}},
                  {"type": "image_url", "image_url": {"url": "https://example.com/image2.jpg"}}
              ]
          }
      ],
      "max_tokens": 700,
      "temperature": 0.8
  });

  println!("   Endpoint: POST /v1/chat/completions");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&multimodal_request)?
  );

  println!("\n📝 Multimodal capabilities:");
  println!("   • Multiple image processing in single request");
  println!("   • Enhanced reasoning and comparison");
  println!("   • Cross-modal understanding");
  println!("   • Same API format as basic vision models");
  println!("✅ Multimodal API pattern demonstrated\n");

  Ok(())
}

/// Demo 4: Text-to-Speech - Audio synthesis
async fn demo_text_to_speech(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🔊 Demo 4: Text-to-Speech (TTS)");
  println!("Models: step-tts-vivid, step-tts-mini\n");

  // Show TTS request structure
  println!("📤 TTS request structure:");
  let tts_request = json!({
      "model": "step-tts-vivid",
      "input": "智能阶跃，十倍每一个人的可能",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "speed": 1.0,
      "volume": 1.0,
      "voice_label": {
          "emotion": "高兴"
      },
      "sample_rate": 24000
  });

  println!("   Endpoint: POST /v1/audio/speech");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&tts_request)?
  );

  println!("\n📝 Key features:");
  println!("   • Dedicated audio synthesis endpoint");
  println!("   • Multiple output formats (MP3, WAV, OPUS, FLAC)");
  println!("   • Voice customization with emotion and style");
  println!("   • Chinese voice optimization");
  println!("   • Returns binary audio data");

  println!("✅ TTS API pattern demonstrated\n");

  Ok(())
}

/// Demo 5: Speech Recognition - Audio transcription
async fn demo_speech_recognition(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎤 Demo 5: Automatic Speech Recognition (ASR)");
  println!("Models: step-asr\n");

  println!("📤 ASR request structure:");
  println!("   Endpoint: POST /v1/audio/transcriptions");
  println!("   Content-Type: multipart/form-data");
  println!("   Method: Form upload with audio file");

  println!("\n📋 Form data parameters:");
  println!("   • file: Audio file (WAV/MP3/FLAC)");
  println!("   • model: \"step-asr\"");
  println!("   • response_format: \"json\" | \"text\" | \"srt\" | \"vtt\"");

  println!("\n📝 Response format examples:");
  println!("   JSON: {{\"text\": \"transcribed content\"}}");
  println!("   Text: Raw transcribed text");
  println!("   SRT: SubRip subtitle format with timestamps");
  println!("   VTT: WebVTT format for web videos");

  println!("\n📝 Key features:");
  println!("   • Multipart form data upload");
  println!("   • Multiple response formats including subtitles");
  println!("   • Optimized for Chinese speech recognition");

  println!("✅ ASR API pattern demonstrated\n");

  Ok(())
}

/// Demo 6: Image Generation - Text to image
async fn demo_image_generation(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎨 Demo 6: Image Generation");
  println!("Models: step-2x-large, step-1x-medium\n");

  // Show image generation request structure
  println!("📤 Image generation request structure:");
  let image_request = json!({
      "model": "step-2x-large",
      "prompt": "未来科技城市夜景，霓虹灯闪烁，赛博朋克风格，高质量，4K超清",
      "size": "1024x1024",
      "n": 1,
      "response_format": "b64_json",
      "seed": 12345,
      "steps": 50,
      "cfg_scale": 7.5,
      "style_reference": {
          "source_url": "https://example.com/style.jpg",
          "weight": 1.0
      }
  });

  println!("   Endpoint: POST /v1/images/generations");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&image_request)?
  );

  println!("\n📝 Key features:");
  println!("   • Specialized image generation endpoint");
  println!("   • Style reference support for consistent aesthetics");
  println!("   • Multiple output formats (base64 or URL)");
  println!("   • Fine-grained control over generation parameters");
  println!("   • Reproducible results with seed parameter");

  println!("✅ Image generation API pattern demonstrated\n");

  Ok(())
}

/// Demo 7: Image Editing - AI-powered image modification
async fn demo_image_editing(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("✂️  Demo 7: Image Editing");
  println!("Models: step-1x-edit\n");

  println!("📤 Image editing request structure:");
  println!("   Endpoint: POST /v1/images/edits");
  println!("   Content-Type: multipart/form-data");
  println!("   Method: Form upload with image file");

  println!("\n📋 Form data parameters:");
  println!("   • model: \"step-1x-edit\"");
  println!("   • image: Image file (JPG/PNG)");
  println!("   • prompt: \"添加彩虹效果，让画面更加梦幻和色彩丰富\"");
  println!("   • response_format: \"url\" | \"b64_json\"");
  println!("   • seed: 12345 (optional, for reproducibility)");
  println!("   • steps: 28 (optional, default 28)");
  println!("   • cfg_scale: 6.0 (optional, default 6)");
  println!("   • size: \"512x512\" | \"768x768\" | \"1024x1024\"");

  println!("\n📝 Key features:");
  println!("   • Multipart form data for image upload");
  println!("   • Text-guided image editing");
  println!("   • Preserves original image structure");
  println!("   • AI-powered enhancement and modification");

  println!("✅ Image editing API pattern demonstrated\n");

  Ok(())
}

/// Demo 8: Voice Cloning - Custom voice profile creation
async fn demo_voice_cloning(_api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎭 Demo 8: Voice Cloning");
  println!("Models: step-tts-vivid (for voice creation)\n");

  println!("📤 Voice cloning workflow:");
  println!("   Step 1: Create voice profile");
  println!("   Endpoint: POST /v1/audio/voices");

  // Show voice cloning request structure
  let voice_request = json!({
      "model": "step-tts-vivid",
      "file_id": "file-abc123",  // From previous file upload
      "text": "这是用于训练的声音样本，请说出这段文字",
      "sample_text": "测试用的样本文本"
  });

  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&voice_request)?
  );

  println!("\n   Step 2: Use cloned voice in TTS");
  let tts_with_clone = json!({
      "model": "step-tts-vivid",
      "input": "使用克隆声音合成的新文本内容",
      "voice": "voice-tone-xyz789",  // Use cloned voice ID from copyvoices API
      "response_format": "mp3"
  });

  println!("   Endpoint: POST /v1/audio/speech");
  println!(
    "   Request body: {}",
    serde_json::to_string_pretty(&tts_with_clone)?
  );

  println!("\n📝 Key features:");
  println!("   • Creates custom voice profiles from audio samples");
  println!("   • Returns voice ID for use in TTS requests");
  println!("   • Includes sample audio playback for verification");
  println!("   • Two-step process: profile creation → voice synthesis");

  println!("✅ Voice cloning API pattern demonstrated\n");

  Ok(())
}

// Example configuration for running the demos
//
// ```bash
// # Set environment variable
// export STEP_API_KEY="sk-your-stepfun-api-key-here"
//
// # Run the comprehensive demo
// cargo run --example stepfun_comprehensive_demo
//
// # Or run with verbose logging
// RUST_LOG=debug cargo run --example stepfun_comprehensive_demo
// ```
//
// Expected output structure:
// - Each demo section shows the correct API pattern
// - Request structures match the real API requirements
// - Model routing is validated for each category
// - Error handling demonstrates proper usage patterns
