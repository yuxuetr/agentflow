//! `tts` — text-to-speech `Tool` adapter.
//!
//! Thin wrapper over `agentflow_llm::AgentFlow::tts(model)`: resolves the
//! model's vendor from the shared model registry YAML and dispatches to
//! that vendor's `TtsProvider` implementation. All per-vendor request/
//! response-shape reconciliation (StepFun's direct-bytes response vs
//! DashScope's submit-then-fetch-URL two-step protocol, etc.) already
//! lives behind that trait in `agentflow-llm`; this tool adds nothing but
//! the `Tool` contract on top.

use agentflow_llm::{AgentFlow, TtsRequest};
use agentflow_tool::{Tool, ToolError, ToolMetadata, ToolOutput, ToolOutputPart};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::common::{
  encode_base64, map_resolution_error, modality_tool_metadata, optional_f32, optional_str,
  optional_u32, required_str,
};

/// Synthesize speech audio from text via any TTS-capable model declared
/// in the model registry (`vendor:`/`accepts:` in
/// `agentflow-llm/templates/models/*.yml`).
pub struct TtsTool;

impl TtsTool {
  pub fn new() -> Self {
    Self
  }
}

impl Default for TtsTool {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Tool for TtsTool {
  fn name(&self) -> &str {
    "tts"
  }

  fn description(&self) -> &str {
    "Synthesize speech audio from text using a configured TTS model. \
     Returns the audio inline as a base64 data URI."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "model": {
          "type": "string",
          "description": "TTS model name from the model registry (e.g. \"step-tts-mini\")."
        },
        "input": {
          "type": "string",
          "description": "Text to synthesize."
        },
        "voice": {
          "type": "string",
          "description": "Voice identifier. Format is vendor-specific — see the model's documentation."
        },
        "format": {
          "type": "string",
          "description": "Audio container format, e.g. \"wav\" | \"mp3\" | \"flac\" | \"opus\". Vendor-specific superset allowed."
        },
        "speed": {
          "type": "number",
          "description": "Playback speed multiplier (1.0 = normal). Vendor-specific range."
        },
        "volume": {
          "type": "number",
          "description": "Output volume multiplier. Vendor-specific."
        },
        "sample_rate": {
          "type": "integer",
          "description": "Sample rate hint in Hz. Vendor-specific."
        }
      },
      "required": ["model", "input", "voice"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    modality_tool_metadata()
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let model = required_str(&params, "model")?;
    let input = required_str(&params, "input")?.to_string();
    let voice = required_str(&params, "voice")?.to_string();

    let provider = AgentFlow::tts(model)
      .await
      .map_err(|err| map_resolution_error("failed to resolve TTS model", err))?;

    let request = TtsRequest {
      model: model.to_string(),
      input,
      voice,
      response_format: optional_str(&params, "format"),
      speed: optional_f32(&params, "speed"),
      volume: optional_f32(&params, "volume"),
      sample_rate: optional_u32(&params, "sample_rate"),
    };

    match provider.synthesize(request).await {
      Ok(response) => {
        let data_uri = format!(
          "data:{};base64,{}",
          response.mime_type,
          encode_base64(&response.audio)
        );
        Ok(ToolOutput::success_parts(
          format!(
            "Synthesized {} bytes of {} audio.",
            response.audio.len(),
            response.mime_type
          ),
          vec![ToolOutputPart::Resource {
            uri: data_uri,
            mime_type: Some(response.mime_type),
            text: None,
          }],
        ))
      }
      Err(err) => Ok(ToolOutput::error(format!("TTS synthesis failed: {err}"))),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parameters_schema_lists_required_fields() {
    let tool = TtsTool::new();
    let schema = tool.parameters_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("model")));
    assert!(required.contains(&json!("input")));
    assert!(required.contains(&json!("voice")));
  }

  #[test]
  fn metadata_is_network_nonidempotent() {
    let tool = TtsTool::new();
    let meta = tool.metadata();
    assert_eq!(
      meta.idempotency,
      agentflow_tool::ToolIdempotency::NonIdempotent
    );
    assert!(
      meta
        .permissions
        .allows(&agentflow_tool::ToolPermission::Network)
    );
  }

  #[tokio::test]
  async fn execute_rejects_missing_required_params() {
    let tool = TtsTool::new();
    let err = tool
      .execute(json!({"model": "step-tts-mini"}))
      .await
      .expect_err("missing input/voice must fail params validation");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_surfaces_unknown_model_as_invalid_params() {
    let tool = TtsTool::new();
    let err = tool
      .execute(json!({
        "model": "definitely-not-a-real-model",
        "input": "hello",
        "voice": "default"
      }))
      .await
      .expect_err("unknown model must fail resolution, not panic or hang");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}
