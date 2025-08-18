use crate::utils::file::detect_file_type;
use agentflow_llm::AgentFlow;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub async fn execute(
  text: String,
  model: Option<String>,
  temperature: Option<f32>,
  max_tokens: Option<u32>,
  file: Option<String>,
  output: Option<String>,
  stream: bool,
  system: Option<String>,
) -> Result<()> {
  // Initialize AgentFlow with builtin configuration
  AgentFlow::init_with_builtin_config()
    .await
    .with_context(|| "Failed to initialize AgentFlow LLM system")?;

  // Build the prompt, including file content if provided
  let prompt = if let Some(file_path) = &file {
    build_prompt_with_file(&text, file_path)?
  } else {
    text
  };

  // Use default model if none specified
  let model_name = model.unwrap_or_else(|| "step-2-mini".to_string());

  // Initialize AgentFlow with the specified model
  let mut agent = AgentFlow::model(&model_name);

  // Note: System prompt is not currently supported in the AgentFlow API
  if system.is_some() {
    eprintln!("Warning: System prompt is not yet supported");
  }

  // Add temperature if provided
  if let Some(temp) = temperature {
    agent = agent.temperature(temp);
  }

  // Add max_tokens if provided
  if let Some(tokens) = max_tokens {
    agent = agent.max_tokens(tokens);
  }

  // Set the prompt
  agent = agent.prompt(&prompt);

  // Execute the request
  let response = if stream {
    // For streaming, we'll implement a simple version for now
    println!("Streaming output:");
    agent
      .execute()
      .await
      .with_context(|| format!("Failed to execute LLM request with model {}", model_name))?
  } else {
    agent
      .execute()
      .await
      .with_context(|| format!("Failed to execute LLM request with model {}", model_name))?
  };

  // Handle output
  if let Some(output_file) = output {
    fs::write(&output_file, &response)
      .with_context(|| format!("Failed to write output to {}", output_file))?;
    println!("Output saved to: {}", output_file);
  } else {
    println!("{}", response);
  }

  Ok(())
}

fn build_prompt_with_file(text: &str, file_path: &str) -> Result<String> {
  let path = Path::new(file_path);

  if !path.exists() {
    return Err(anyhow::anyhow!("File not found: {}", file_path));
  }

  let file_type = detect_file_type(path)?;

  match file_type.as_str() {
    "text" => {
      let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read text file: {}", file_path))?;
      Ok(format!("{}\n\nFile content:\n{}", text, content))
    }
    "image" => {
      // For now, just mention the image file - multimodal support will be enhanced later
      Ok(format!("{}\n\nImage file: {}", text, file_path))
    }
    "audio" => {
      // For now, just mention the audio file - audio support will be enhanced later
      Ok(format!("{}\n\nAudio file: {}", text, file_path))
    }
    _ => {
      // Try to read as text for unknown types
      match fs::read_to_string(path) {
        Ok(content) => Ok(format!("{}\n\nFile content:\n{}", text, content)),
        Err(_) => Ok(format!("{}\n\nBinary file: {}", text, file_path)),
      }
    }
  }
}
