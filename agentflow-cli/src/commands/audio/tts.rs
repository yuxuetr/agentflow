use agentflow_llm::{AgentFlow, TtsRequest};
use anyhow::Result;
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

  // Initialize AgentFlow so the registry knows about all configured
  // models. The TTS dispatcher then resolves vendor + API key from
  // the registered model entry (`type: tts`).
  AgentFlow::init().await?;

  println!("🎙️  AgentFlow Text-to-Speech");
  println!("Model: {}", model);
  println!("Text: {}", text);
  println!("Voice: {}", voice);
  println!("Format: {}", format);
  println!("Speed: {}", speed);
  if emotion.is_some() {
    // `emotion` was a StepFun voice-label extension. The cross-vendor
    // TtsRequest doesn't carry it; the dispatcher hides vendor-specific
    // niceties for now. Surface the warning so operators know it's
    // dropped rather than silently ignored.
    println!("⚠️  --emotion is currently dropped (vendor-specific knob).");
  }
  println!("Output: {}", output);
  println!();

  let provider = AgentFlow::tts(&model).await?;
  println!(
    "🎵 Synthesising via provider '{}' (model '{}')...",
    provider.name(),
    model
  );

  let request = TtsRequest {
    model: model.clone(),
    input: text,
    voice,
    response_format: Some(format),
    speed: Some(speed),
    volume: None,
    sample_rate: None,
  };

  let start_time = std::time::Instant::now();
  let response = provider.synthesize(request).await?;
  let duration = start_time.elapsed();

  println!("✅ Speech generated in {:?}", duration);
  println!(
    "💾 Audio size: {} bytes ({})",
    response.audio.len(),
    response.mime_type
  );
  println!();

  println!("💾 Saving audio to: {}", output);
  fs::write(&output, &response.audio).await?;
  println!("✅ Audio saved successfully");

  println!("🎉 Text-to-speech conversion completed successfully!");
  Ok(())
}
