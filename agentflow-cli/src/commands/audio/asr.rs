use agentflow_llm::{ASRRequest, AgentFlow};
use anyhow::Result;
use std::env;
use tokio::fs;

pub async fn execute(
  audio_path: String,
  model: Option<String>,
  format: String,
  language: Option<String>,
  output: Option<String>,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-asr".to_string());

  // Get API key from environment
  let api_key = env::var("STEPFUN_API_KEY")
    .or_else(|_| env::var("STEP_API_KEY"))
    .map_err(|_| {
      anyhow::anyhow!("STEPFUN_API_KEY or STEP_API_KEY environment variable must be set")
    })?;

  println!("🎧 AgentFlow Speech-to-Text");
  println!("Model: {}", model);
  println!("Audio: {}", audio_path);
  println!("Format: {}", format);
  if let Some(lang) = &language {
    println!("Language: {}", lang);
  }
  if let Some(output) = &output {
    println!("Output: {}", output);
  }
  println!();

  // Skip general AgentFlow init for specialized StepFun client

  // Check if audio file exists
  if !std::path::Path::new(&audio_path).exists() {
    return Err(anyhow::anyhow!("Audio file not found: {}", audio_path));
  }

  // Read audio file
  println!("🎵 Reading audio file...");
  let audio_data = fs::read(&audio_path).await?;
  println!("💾 Audio size: {} bytes", audio_data.len());

  // Extract filename for the request
  let filename = std::path::Path::new(&audio_path)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("audio")
    .to_string();

  // Create StepFun specialized client
  println!("📡 Creating StepFun client...");
  let stepfun_client = AgentFlow::stepfun_client(&api_key).await?;

  // Build ASR request
  println!("🔍 Building speech recognition request...");
  let request = ASRRequest {
    model: model.clone(),
    response_format: format.clone(),
    audio_data,
    filename,
  };

  // Transcribe audio
  println!("🚀 Transcribing audio...");
  let start_time = std::time::Instant::now();

  let transcription = stepfun_client.speech_to_text(request).await?;

  let duration = start_time.elapsed();
  println!("✅ Transcription completed in {:?}", duration);
  println!();

  // Display results
  println!("📝 Transcription Results:");
  println!("========================");
  match format.as_str() {
    "json" => {
      // Try to pretty-print JSON if possible
      if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&transcription) {
        println!("{}", serde_json::to_string_pretty(&json_value)?);
      } else {
        println!("{}", transcription);
      }
    }
    "srt" | "vtt" => {
      println!("{}", transcription);
    }
    "text" => {
      println!("{}", transcription);
    }
    _ => {
      println!("{}", transcription);
    }
  }
  println!();

  // Save to file if specified
  if let Some(output_path) = output {
    println!("💾 Saving transcription to: {}", output_path);

    let output_content = match format.as_str() {
      "json" | "srt" | "vtt" => transcription,
      "text" => transcription,
      _ => format!("# Speech Transcription Results\n\n**Model:** {}\n**Audio:** {}\n**Format:** {}\n\n**Transcription:**\n{}\n", 
                   model, audio_path, format, transcription),
    };

    fs::write(&output_path, &output_content).await?;
    println!("✅ Transcription saved successfully");
  }

  println!("🎉 Speech-to-text transcription completed successfully!");
  Ok(())
}
