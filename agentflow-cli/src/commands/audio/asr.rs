use agentflow_llm::{AgentFlow, AsrRequest};
use anyhow::Result;
use tokio::fs;

pub async fn execute(
  audio_path: String,
  model: Option<String>,
  format: String,
  language: Option<String>,
  output: Option<String>,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-asr".to_string());

  AgentFlow::init().await?;

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

  if !std::path::Path::new(&audio_path).exists() {
    return Err(anyhow::anyhow!("Audio file not found: {}", audio_path));
  }

  println!("🎵 Reading audio file...");
  let audio_data = fs::read(&audio_path).await?;
  println!("💾 Audio size: {} bytes", audio_data.len());

  let filename = std::path::Path::new(&audio_path)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("audio")
    .to_string();

  let provider = AgentFlow::asr(&model).await?;
  println!(
    "🔍 Transcribing via provider '{}' (model '{}')...",
    provider.name(),
    model
  );

  let request = AsrRequest {
    model: model.clone(),
    response_format: format.clone(),
    audio_data,
    filename,
    language: language.clone(),
    temperature: None,
    prompt: None,
  };

  let start_time = std::time::Instant::now();
  let response = provider.transcribe(request).await?;
  let transcription = response.text;
  let duration = start_time.elapsed();
  println!("✅ Transcription completed in {:?}", duration);
  println!();

  println!("📝 Transcription Results:");
  println!("========================");
  match format.as_str() {
    "json" => {
      if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&transcription) {
        println!("{}", serde_json::to_string_pretty(&json_value)?);
      } else {
        println!("{}", transcription);
      }
    }
    _ => println!("{}", transcription),
  }
  println!();

  if let Some(output_path) = output {
    println!("💾 Saving transcription to: {}", output_path);

    let output_content = match format.as_str() {
      "json" | "srt" | "vtt" | "text" => transcription,
      _ => format!(
        "# Speech Transcription Results\n\n**Model:** {}\n**Audio:** {}\n**Format:** {}\n\n**Transcription:**\n{}\n",
        model, audio_path, format, transcription
      ),
    };

    fs::write(&output_path, &output_content).await?;
    println!("✅ Transcription saved successfully");
  }

  println!("🎉 Speech-to-text transcription completed successfully!");
  Ok(())
}
