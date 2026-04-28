use crate::redaction::redact_cli_text;
use agentflow_llm::AgentFlow;
use anyhow::{Context, Result};
use std::io::{self, BufRead, Write};

pub async fn execute(
  model: Option<String>,
  system: Option<String>,
  save: Option<String>,
  load: Option<String>,
) -> Result<()> {
  if let Some(path) = &load {
    eprintln!("⚠  --load is not implemented yet; ignoring '{}'", path);
  }
  if let Some(path) = &save {
    eprintln!("⚠  --save is not implemented yet; ignoring '{}'", path);
  }

  AgentFlow::init()
    .await
    .context("Failed to initialise AgentFlow — is your API key configured?")?;

  let model = model.unwrap_or_else(|| "gpt-4o".to_string());
  println!("🤖 Model: {}", model);
  println!("Type a message, /exit, or /quit. Ctrl-C to exit.\n");

  let stdin = io::stdin();
  let mut stdout = io::stdout();
  print!("You: ");
  stdout.flush().ok();

  for line in stdin.lock().lines() {
    let line = line.context("Failed to read from stdin")?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
      print!("You: ");
      stdout.flush().ok();
      continue;
    }
    if matches!(trimmed, "/exit" | "/quit") {
      println!("Bye.");
      break;
    }

    let mut request = AgentFlow::model(&model).prompt(trimmed);
    if let Some(system) = &system {
      request = request.system(system);
    }

    match request.execute().await {
      Ok(answer) => println!("Agent: {}", redact_cli_text(&answer)),
      Err(err) => eprintln!("Error: {}", redact_cli_text(err.to_string())),
    }

    print!("You: ");
    stdout.flush().ok();
  }

  Ok(())
}
