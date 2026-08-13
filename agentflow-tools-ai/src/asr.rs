//! `asr` — automatic speech recognition `Tool` adapter.
//!
//! Thin wrapper over `agentflow_llm::AgentFlow::asr(model)` — see `tts.rs`'s
//! module docs for the shared design rationale (vendor reconciliation stays
//! entirely in `agentflow-llm`; this tool adds only the `Tool` contract).

use agentflow_llm::{AgentFlow, AsrRequest};
use agentflow_tool::{Tool, ToolError, ToolMetadata, ToolOutput};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::common::{
  map_resolution_error, modality_tool_metadata, optional_f32, optional_str, required_base64,
  required_str,
};

/// Transcribe audio into text via any ASR-capable model declared in the
/// model registry.
pub struct AsrTool;

impl AsrTool {
  pub fn new() -> Self {
    Self
  }
}

impl Default for AsrTool {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Tool for AsrTool {
  fn name(&self) -> &str {
    "asr"
  }

  fn description(&self) -> &str {
    "Transcribe audio into text using a configured ASR model. \
     Audio is supplied inline as base64."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "model": {
          "type": "string",
          "description": "ASR model name from the model registry (e.g. \"step-asr\", \"whisper-1\")."
        },
        "audio_data": {
          "type": "string",
          "description": "Base64-encoded audio bytes (mp3/wav/flac/m4a/opus/etc.)."
        },
        "filename": {
          "type": "string",
          "description": "Original filename — the extension is used by some vendors to infer codec. Defaults to \"audio.wav\"."
        },
        "response_format": {
          "type": "string",
          "description": "Wire response format: \"json\" | \"text\" | \"srt\" | \"vtt\". Defaults to \"json\"."
        },
        "language": {
          "type": "string",
          "description": "Optional BCP-47 language hint (e.g. \"en\", \"zh\")."
        },
        "temperature": {
          "type": "number",
          "description": "Optional sampling temperature (0.0-1.0). Not every vendor honors this."
        },
        "prompt": {
          "type": "string",
          "description": "Optional context prompt to bias recognition toward domain vocabulary."
        }
      },
      "required": ["model", "audio_data"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    modality_tool_metadata()
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let model = required_str(&params, "model")?;
    let audio_data = required_base64(&params, "audio_data")?;

    let provider = AgentFlow::asr(model)
      .await
      .map_err(|err| map_resolution_error("failed to resolve ASR model", err))?;

    let request = AsrRequest {
      model: model.to_string(),
      audio_data,
      filename: optional_str(&params, "filename").unwrap_or_else(|| "audio.wav".to_string()),
      response_format: optional_str(&params, "response_format")
        .unwrap_or_else(|| "json".to_string()),
      language: optional_str(&params, "language"),
      temperature: optional_f32(&params, "temperature"),
      prompt: optional_str(&params, "prompt"),
    };

    match provider.transcribe(request).await {
      Ok(response) => Ok(ToolOutput::success(response.text)),
      Err(err) => Ok(ToolOutput::error(format!(
        "ASR transcription failed: {err}"
      ))),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn execute_rejects_missing_audio_data() {
    let tool = AsrTool::new();
    let err = tool
      .execute(json!({"model": "step-asr"}))
      .await
      .expect_err("missing audio_data must fail params validation");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_rejects_invalid_base64() {
    let tool = AsrTool::new();
    let err = tool
      .execute(json!({"model": "step-asr", "audio_data": "not-valid-base64!!!"}))
      .await
      .expect_err("malformed base64 must fail params validation");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_surfaces_unknown_model_as_invalid_params() {
    let tool = AsrTool::new();
    let err = tool
      .execute(json!({"model": "definitely-not-a-real-model", "audio_data": "AAAA"}))
      .await
      .expect_err("unknown model must fail resolution, not panic or hang");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}
