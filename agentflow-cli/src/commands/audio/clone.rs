use anyhow::Result;

pub async fn execute(
  reference_audio: String,
  text: String,
  model: Option<String>,
  format: String,
  output: String,
) -> Result<()> {
  let model = model.unwrap_or_else(|| "step-speech".to_string());

  println!("🎭 AgentFlow Voice Cloning");
  println!("Model: {}", model);
  println!("Reference Audio: {}", reference_audio);
  println!("Text: {}", text);
  println!("Format: {}", format);
  println!("Output: {}", output);
  println!();

  // Voice cloning has no cross-vendor trait yet (it's StepFun-only on
  // the server side and needs a separate `VoiceCloningProvider` to land
  // in `agentflow-llm::providers::modality`). Until that trait exists,
  // this CLI stays a placeholder rather than reaching directly into
  // StepFun's internal types.
  Err(anyhow::anyhow!(
    "❌ Voice cloning is not yet implemented.\n\n\
     This feature is blocked on a `VoiceCloningProvider` trait in \
     `agentflow-llm::providers::modality`. Track progress under P-LLM \
     follow-ups in TODOs.md."
  ))
}
