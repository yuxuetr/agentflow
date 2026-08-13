//! `image_understand` — image understanding `Tool` adapter.
//!
//! Unlike the other five tools in this crate, image understanding has no
//! dedicated modality-dispatch trait — `agentflow-nodes-ai`'s
//! `ImageUnderstandNode` routes through the ordinary chat path
//! (`AgentFlow::model(...).multimodal_prompt(...)`), and this tool mirrors
//! that rather than inventing a parallel path. See `agentflow-llm`'s
//! P-LLM2.4 multimodal-input work (`multimodal.rs`, `providers/{anthropic,
//! google}.rs`) for how `add_image_url` content actually reaches each
//! vendor.

use agentflow_llm::{AgentFlow, LLMError, multimodal::MultimodalMessage};
use agentflow_tool::{Tool, ToolError, ToolMetadata, ToolOutput};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::common::{modality_tool_metadata, required_str};

/// Ask a vision-capable chat model a question about an image, via any
/// model declared `accepts: image` in the model registry.
pub struct ImageUnderstandTool;

impl ImageUnderstandTool {
  pub fn new() -> Self {
    Self
  }
}

impl Default for ImageUnderstandTool {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Tool for ImageUnderstandTool {
  fn name(&self) -> &str {
    "image_understand"
  }

  fn description(&self) -> &str {
    "Ask a question about an image using a configured vision-capable chat model. \
     Returns the model's text answer."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "model": {
          "type": "string",
          "description": "Vision-capable chat model name from the model registry (must declare accepts: image)."
        },
        "prompt": {
          "type": "string",
          "description": "Question or instruction about the image."
        },
        "image_url": {
          "type": "string",
          "description": "Image reference — a public HTTP(S) URL or a `data:<mime>;base64,<payload>` URI."
        }
      },
      "required": ["model", "prompt", "image_url"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    modality_tool_metadata()
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let model = required_str(&params, "model")?.to_string();
    let prompt = required_str(&params, "prompt")?.to_string();
    let image_url = required_str(&params, "image_url")?.to_string();

    let message = MultimodalMessage::user()
      .add_text(prompt)
      .add_image_url(image_url)
      .build();

    match AgentFlow::model(&model)
      .multimodal_prompt(message)
      .execute()
      .await
    {
      Ok(text) => Ok(ToolOutput::success(text)),
      // Unlike the other five tools, this path has no separate
      // resolve-then-call step — `.execute()` does both, so a bad `model`
      // value/setup problem and a genuine vendor-call failure surface as
      // the same `Result::Err`. Distinguish by variant: the ones that mean
      // "this model/config can't do what you asked" map to `InvalidParams`
      // (actionable: try a different `model`, or fix configuration),
      // everything else (network/HTTP/rate-limit/timeout — the request was
      // well-formed, the call itself just failed) is a soft business
      // failure the caller can inspect and react to.
      Err(
        err @ (LLMError::ModelNotFound { .. }
        | LLMError::UnsupportedProvider { .. }
        | LLMError::InvalidModelConfig { .. }
        | LLMError::ConfigurationError { .. }
        | LLMError::MissingApiKey { .. }
        | LLMError::UnsupportedFeature { .. }),
      ) => Err(ToolError::InvalidParams {
        message: format!("failed to resolve image-understanding model: {err}"),
      }),
      Err(err) => Ok(ToolOutput::error(format!(
        "image understanding failed: {err}"
      ))),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn execute_rejects_missing_image_url() {
    let tool = ImageUnderstandTool::new();
    let err = tool
      .execute(json!({"model": "gpt-4o-mini", "prompt": "what is this?"}))
      .await
      .expect_err("missing image_url must fail params validation");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_surfaces_unknown_model_as_invalid_params() {
    let tool = ImageUnderstandTool::new();
    let err = tool
      .execute(json!({
        "model": "definitely-not-a-real-model",
        "prompt": "what is this?",
        "image_url": "https://example.com/cat.png"
      }))
      .await
      .expect_err("unknown model must fail resolution, not panic or hang");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}
