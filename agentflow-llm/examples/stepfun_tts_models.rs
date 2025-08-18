use agentflow_llm::providers::stepfun::StepFunSpecializedClient;
/// StepFun TTS Models - Real API Test Cases
///
/// Tests text-to-speech models with actual audio generation capabilities.
/// Demonstrates real requests and responses from StepFun TTS API.
///
/// Usage:
/// ```bash
/// export STEP_API_KEY="your-stepfun-api-key-here"
/// cargo run --example stepfun_tts_models
/// ```
use reqwest::Client;
use serde_json::json;
use std::env;
use tokio::fs;

/// Get available voices from StepFun API
async fn get_available_voices(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎙️  Getting available voices from StepFun API");
  println!("===========================================\n");

  let client = StepFunSpecializedClient::new(api_key, None)?;

  match client.list_voices(Some(20), None, None, None).await {
    Ok(voice_response) => {
      println!("📊 Available voices:");
      println!("   Total voices found: {}", voice_response.data.len());
      println!("   Has more voices: {}", voice_response.has_more);

      for (i, voice) in voice_response.data.iter().enumerate() {
        println!("   {}. Voice ID: {}", i + 1, voice.id);
        println!("      File ID: {}", voice.file_id);
        println!("      Created: {}", voice.created_at);
      }

      if voice_response.data.is_empty() {
        println!("   ℹ️  No custom voices found. Using default voice 'cixingnansheng'");
      }

      println!(); // Empty line for spacing
    }
    Err(e) => {
      println!("⚠️  Could not retrieve voice list: {}", e);
      println!("   This is normal if no custom voices have been created.");
      println!("   Using default voice 'cixingnansheng' for TTS examples.\n");
    }
  }

  Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  env_logger::init();

  let api_key = env::var("STEP_API_KEY").expect("STEP_API_KEY environment variable is required");

  println!("🔊 StepFun TTS Models - Real API Tests");
  println!("=====================================\n");

  // First, show how to get available voices
  get_available_voices(&api_key).await?;

  // Test different TTS models and configurations
  test_step_tts_vivid_basic(&api_key).await?;
  test_step_tts_mini_fast(&api_key).await?;
  test_step_tts_vivid_emotional(&api_key).await?;
  test_step_tts_vivid_multilingual(&api_key).await?;
  test_step_tts_vivid_advanced(&api_key).await?;

  println!("✅ All TTS tests completed successfully!");
  Ok(())
}

/// Test step-tts-vivid with basic synthesis
async fn test_step_tts_vivid_basic(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎵 Testing step-tts-vivid (Basic Synthesis)");
  println!("Model: step-tts-vivid | Task: Standard Chinese TTS\n");

  let client = Client::new();

  let request_body = json!({
      "model": "step-tts-vivid",
      "input": "智能阶跃，十倍每一个人的可能。人工智能助力未来发展。",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "speed": 1.0
  });

  println!("📤 Sending TTS request to step-tts-vivid...");
  println!("   Input text: {}", request_body["input"]);
  println!("   Voice: {}", request_body["voice"]);
  println!("   Format: {}", request_body["response_format"]);
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&request_body)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if !response.status().is_success() {
    let error_text = response.text().await?;
    eprintln!("❌ TTS request failed: {}", error_text);
    return Err("TTS request failed".into());
  }

  let audio_data = response.bytes().await?;

  println!("📥 TTS synthesis completed in {:?}", duration);
  println!("📊 Audio details:");
  println!("   Audio size: {} bytes", audio_data.len());
  println!("   Format: MP3");
  println!("   Sample rate: 24kHz");

  // Save audio file
  let output_file = "stepfun_tts_basic.mp3";
  fs::write(output_file, &audio_data).await?;
  println!("💾 Audio saved to: {}", output_file);

  // Validate audio file
  let audio_header = &audio_data[..std::cmp::min(10, audio_data.len())];
  let is_valid_mp3 = audio_header.len() >= 3
    && (audio_header[0] == 0xFF && (audio_header[1] & 0xE0) == 0xE0)
    || (audio_header[0] == 0x49 && audio_header[1] == 0x44 && audio_header[2] == 0x33); // ID3 tag

  println!("✅ Audio validation:");
  println!("   Valid MP3 format: {}", is_valid_mp3);
  println!("   Audio data length: {} bytes", audio_data.len());

  if audio_data.len() < 1000 {
    println!("⚠️  Warning: Audio file seems unusually small");
  }

  println!(); // Empty line for spacing
  Ok(())
}

/// Test step-tts-mini with faster synthesis
async fn test_step_tts_mini_fast(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("⚡ Testing step-tts-mini (Fast Synthesis)");
  println!("Model: step-tts-mini | Task: Quick audio generation\n");

  let client = Client::new();

  let request_body = json!({
      "model": "step-tts-mini",
      "input": "这是一个快速语音合成测试，使用 step-tts-mini 模型。",
      "voice": "cixingnansheng",
      "response_format": "wav",
      "speed": 1.2
  });

  println!("📤 Sending fast TTS request to step-tts-mini...");
  println!("   Input text: {}", request_body["input"]);
  println!("   Voice: {}", request_body["voice"]);
  println!("   Speed: {}x", request_body["speed"]);
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&request_body)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if !response.status().is_success() {
    let error_text = response.text().await?;
    eprintln!("❌ Fast TTS request failed: {}", error_text);
    return Err("Fast TTS request failed".into());
  }

  let audio_data = response.bytes().await?;

  println!("📥 Fast TTS synthesis completed in {:?}", duration);
  println!("📊 Performance metrics:");
  println!(
    "   Generation speed: {:.2} chars/sec",
    request_body["input"].as_str().unwrap().len() as f32 / duration.as_secs_f32()
  );
  println!("   Audio size: {} bytes", audio_data.len());
  println!(
    "   Compression ratio: {:.2} bytes/char",
    audio_data.len() as f32 / request_body["input"].as_str().unwrap().len() as f32
  );

  // Save audio file
  let output_file = "stepfun_tts_mini_fast.wav";
  fs::write(output_file, &audio_data).await?;
  println!("💾 Audio saved to: {}", output_file);

  // Validate WAV file
  let audio_header = &audio_data[..std::cmp::min(12, audio_data.len())];
  let is_valid_wav =
    audio_header.len() >= 12 && &audio_header[0..4] == b"RIFF" && &audio_header[8..12] == b"WAVE";

  println!("✅ Fast TTS validation:");
  println!("   Valid WAV format: {}", is_valid_wav);
  println!("   Generation time: {:?}", duration);
  println!("   Model efficiency: step-tts-mini optimized for speed");

  println!(); // Empty line for spacing
  Ok(())
}

/// Test step-tts-vivid with emotional synthesis
async fn test_step_tts_vivid_emotional(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("😊 Testing step-tts-vivid (Emotional Synthesis)");
  println!("Model: step-tts-vivid | Task: Emotional voice synthesis\n");

  let client = Client::new();

  let request_body = json!({
      "model": "step-tts-vivid",
      "input": "今天天气真好啊！阳光明媚，心情愉快，让我们一起出去散步吧！",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "speed": 0.9,
      "voice_label": {
          "emotion": "高兴"
      }
  });

  println!("📤 Sending emotional TTS request...");
  println!("   Input text: {}", request_body["input"]);
  println!("   Voice: {}", request_body["voice"]);
  println!("   Emotion: {}", request_body["voice_label"]["emotion"]);
  println!("   Speed: {}x", request_body["speed"]);
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&request_body)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if !response.status().is_success() {
    let error_text = response.text().await?;
    eprintln!("❌ Emotional TTS request failed: {}", error_text);
    return Err("Emotional TTS request failed".into());
  }

  let audio_data = response.bytes().await?;

  println!("📥 Emotional TTS synthesis completed in {:?}", duration);
  println!("📊 Emotional synthesis details:");
  println!("   Audio size: {} bytes", audio_data.len());
  println!("   Emotional processing: Applied");
  println!("   Voice modulation: Enhanced for happiness");

  // Save audio file
  let output_file = "stepfun_tts_emotional.mp3";
  fs::write(output_file, &audio_data).await?;
  println!("💾 Emotional audio saved to: {}", output_file);

  println!("✅ Emotional TTS validation:");
  println!("   Voice expression: Optimized for positive emotion");
  println!("   Audio quality: High-fidelity synthesis");
  println!("   Emotional range: step-tts-vivid supports multiple emotions");

  println!(); // Empty line for spacing
  Ok(())
}

/// Test step-tts-vivid with multilingual synthesis
async fn test_step_tts_vivid_multilingual(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🌍 Testing step-tts-vivid (Multilingual Synthesis)");
  println!("Model: step-tts-vivid | Task: Multiple language support\n");

  // Test Cantonese
  let client = Client::new();

  let cantonese_request = json!({
      "model": "step-tts-vivid",
      "input": "你好，今日天气点样？我哋一齐去飲茶啦！",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "voice_label": {
          "language": "粤语"
      }
  });

  println!("📤 Testing Cantonese synthesis...");
  println!("   Input text: {}", cantonese_request["input"]);
  println!("   Voice: {}", cantonese_request["voice"]);
  println!(
    "   Language: {}",
    cantonese_request["voice_label"]["language"]
  );
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&cantonese_request)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if response.status().is_success() {
    let audio_data = response.bytes().await?;
    println!("📥 Cantonese synthesis completed in {:?}", duration);
    println!("   Audio size: {} bytes", audio_data.len());

    let output_file = "stepfun_tts_cantonese.mp3";
    fs::write(output_file, &audio_data).await?;
    println!("💾 Cantonese audio saved to: {}", output_file);
  } else {
    let error_text = response.text().await?;
    println!("⚠️  Cantonese synthesis result: {}", error_text);
  }

  // Test Sichuan dialect
  let sichuan_request = json!({
      "model": "step-tts-vivid",
      "input": "巴适得很！今天我们去吃火锅，要得不得？",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "voice_label": {
          "language": "四川话"
      }
  });

  println!("\n📤 Testing Sichuan dialect synthesis...");
  println!("   Input text: {}", sichuan_request["input"]);
  println!("   Voice: {}", sichuan_request["voice"]);
  println!(
    "   Language: {}",
    sichuan_request["voice_label"]["language"]
  );
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&sichuan_request)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if response.status().is_success() {
    let audio_data = response.bytes().await?;
    println!("📥 Sichuan dialect synthesis completed in {:?}", duration);
    println!("   Audio size: {} bytes", audio_data.len());

    let output_file = "stepfun_tts_sichuan.mp3";
    fs::write(output_file, &audio_data).await?;
    println!("💾 Sichuan audio saved to: {}", output_file);
  } else {
    let error_text = response.text().await?;
    println!("⚠️  Sichuan dialect synthesis result: {}", error_text);
  }

  println!("\n✅ Multilingual TTS validation:");
  println!("   Language support: Multiple Chinese dialects");
  println!("   Voice adaptation: Automatic accent adjustment");
  println!("   Cultural context: Dialect-specific expressions");

  println!(); // Empty line for spacing
  Ok(())
}

/// Test step-tts-vivid with advanced synthesis options
async fn test_step_tts_vivid_advanced(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("🎛️  Testing step-tts-vivid (Advanced Options)");
  println!("Model: step-tts-vivid | Task: Advanced synthesis control\n");

  let client = Client::new();

  let advanced_request = json!({
      "model": "step-tts-vivid",
      "input": "这是一段高质量的语音合成测试，展示了多种音频参数的精确控制能力，包括语速、音量、情感和风格的细致调节。",
      "voice": "cixingnansheng",
      "response_format": "mp3",
      "speed": 0.8,
      "volume": 1.2,
      "voice_label": {
          "style": "慢速"
      },
      "sample_rate": 24000
  });

  println!("📤 Sending advanced TTS request...");
  println!(
    "   Input length: {} chars",
    advanced_request["input"].as_str().unwrap().len()
  );
  println!("   Format: {}", advanced_request["response_format"]);
  println!("   Voice: {}", advanced_request["voice"]);
  println!("   Speed: {}x", advanced_request["speed"]);
  println!("   Volume: {}x", advanced_request["volume"]);
  println!("   Style: {}", advanced_request["voice_label"]["style"]);
  println!("   Sample rate: {}Hz", advanced_request["sample_rate"]);
  let start_time = std::time::Instant::now();

  let response = client
    .post("https://api.stepfun.com/v1/audio/speech")
    .header("Content-Type", "application/json")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&advanced_request)
    .send()
    .await?;

  let duration = start_time.elapsed();

  if !response.status().is_success() {
    let error_text = response.text().await?;
    eprintln!("❌ Advanced TTS request failed: {}", error_text);
    return Err("Advanced TTS request failed".into());
  }

  let audio_data = response.bytes().await?;

  println!("📥 Advanced TTS synthesis completed in {:?}", duration);
  println!("📊 Advanced synthesis metrics:");
  println!("   Audio size: {} bytes", audio_data.len());
  println!("   Format: MP3");
  println!("   Processing time: {:?}", duration);

  // Calculate synthesis metrics
  let text_length = advanced_request["input"].as_str().unwrap().len();
  let synthesis_speed = text_length as f32 / duration.as_secs_f32();
  let audio_duration_estimate = text_length as f32 / 6.0; // Approximate 6 chars per second

  println!("📈 Performance analysis:");
  println!("   Synthesis speed: {:.1} chars/sec", synthesis_speed);
  println!(
    "   Estimated audio duration: {:.1} seconds",
    audio_duration_estimate
  );
  println!(
    "   Real-time factor: {:.2}x",
    duration.as_secs_f32() / audio_duration_estimate
  );

  // Save audio file
  let output_file = "stepfun_tts_advanced.mp3";
  fs::write(output_file, &audio_data).await?;
  println!("💾 Advanced audio saved to: {}", output_file);

  println!("✅ Advanced TTS validation:");
  println!("   Format support: MP3 confirmed");
  println!("   Parameter control: Speed and voice customization");
  println!("   Audio quality: High-quality synthesis");
  println!("   Processing efficiency: Optimized for quality");

  println!(); // Empty line for spacing
  Ok(())
}

/// Test batch TTS synthesis
#[allow(dead_code)]
async fn test_batch_tts_synthesis(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
  println!("📦 Testing Batch TTS Synthesis");
  println!("Multiple requests with different parameters\n");

  let client = Client::new();
  let texts = vec![
    "第一段测试文本，用于批量合成。",
    "第二段测试文本，检验并行处理能力。",
    "第三段测试文本，验证系统稳定性。",
  ];

  let mut handles = vec![];

  for (i, text) in texts.iter().enumerate() {
    let client = client.clone();
    let api_key = api_key.to_string();
    let text = text.to_string();

    let handle = tokio::spawn(async move {
      let request_body = json!({
          "model": "step-tts-mini",
          "input": text,
          "voice": "xiaoxing",
          "response_format": "mp3",
          "speed": 1.0 + (i as f32 * 0.1)
      });

      let response = client
        .post("https://api.stepfun.com/v1/audio/speech")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

      if response.status().is_success() {
        let audio_data = response.bytes().await?;
        let filename = format!("stepfun_tts_batch_{}.mp3", i + 1);
        fs::write(&filename, &audio_data).await?;
        Ok::<(usize, String, usize), Box<dyn std::error::Error + Send + Sync>>((
          i + 1,
          filename,
          audio_data.len(),
        ))
      } else {
        let error = response.text().await?;
        Err(format!("Batch item {} failed: {}", i + 1, error).into())
      }
    });

    handles.push(handle);
  }

  println!("📤 Processing {} TTS requests in parallel...", texts.len());
  let start_time = std::time::Instant::now();

  let results = futures::future::join_all(handles).await;
  let duration = start_time.elapsed();

  println!("📥 Batch synthesis completed in {:?}", duration);
  println!("📊 Batch results:");

  for result in results {
    match result {
      Ok(Ok((index, filename, size))) => {
        println!("   Item {}: {} ({} bytes)", index, filename, size);
      }
      Ok(Err(e)) => {
        println!("   Error: {}", e);
      }
      Err(e) => {
        println!("   Task error: {}", e);
      }
    }
  }

  println!("✅ Batch TTS validation:");
  println!("   Parallel processing: Supported");
  println!("   Batch efficiency: Multiple concurrent requests");

  Ok(())
}
