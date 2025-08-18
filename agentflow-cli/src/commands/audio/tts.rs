use agentflow_llm::{AgentFlow, TTSBuilder};
use anyhow::Result;
use std::env;
use tokio::fs;

pub async fn execute(
  text: String,
  model: Option<String>,
  voice: String,
  format: String,
  speed: f32,
  output: String,
  emotion: Option<String>,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-tts-mini".to_string());

  // Get API key from environment
  let api_key = env::var("STEPFUN_API_KEY")
    .or_else(|_| env::var("STEP_API_KEY"))
    .map_err(|_| {
      anyhow::anyhow!("STEPFUN_API_KEY or STEP_API_KEY environment variable must be set")
    })?;

  println!("🎙️  AgentFlow Text-to-Speech");
  println!("Model: {}", model);
  println!("Text: {}", text);
  println!("Voice: {}", voice);
  println!("Format: {}", format);
  println!("Speed: {}", speed);
  if let Some(emotion) = &emotion {
    println!("Emotion: {}", emotion);
  }
  println!("Output: {}", output);
  println!();

  // Create StepFun specialized client directly (no need for general AgentFlow init)
  println!("📡 Creating StepFun client...");
  let stepfun_client = AgentFlow::stepfun_client(&api_key).await?;

  // Build TTS request
  println!("🎵 Building text-to-speech request...");
  let mut tts_builder = TTSBuilder::new(&model, &text, &voice)
    .response_format(&format)
    .speed(speed);

  if let Some(emotion) = emotion {
    tts_builder = tts_builder.emotion(&emotion);
  }

  let request = tts_builder.build();

  // Generate speech
  println!("🚀 Generating speech...");
  let start_time = std::time::Instant::now();

  let audio_data = stepfun_client.text_to_speech(request).await?;

  let duration = start_time.elapsed();
  println!("✅ Speech generated in {:?}", duration);
  println!("💾 Audio size: {} bytes", audio_data.len());
  println!();

  // Save audio file
  println!("💾 Saving audio to: {}", output);
  fs::write(&output, &audio_data).await?;
  println!("✅ Audio saved successfully");

  println!("🎉 Text-to-speech conversion completed successfully!");
  Ok(())
}
