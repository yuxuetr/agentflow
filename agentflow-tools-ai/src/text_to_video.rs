//! `text_to_video` — text-to-video generation `Tool` adapter.
//!
//! Thin wrapper over `agentflow_llm::AgentFlow::text2video_for(model)` — see
//! `tts.rs`'s module docs for the shared design rationale.
//!
//! Unlike the other five tools, video generation is an async job API
//! (`Text2VideoProvider::submit` + `::poll`, minutes not seconds) rather
//! than a synchronous request/response — see that trait's module docs in
//! `agentflow-llm/src/providers/modality/text_to_video.rs`. This tool uses
//! the trait's `generate_and_wait` convenience default, blocking the
//! calling tool call for up to `max_wait_secs` rather than exposing a
//! separate submit/poll tool pair — simpler for a first cut, at the cost
//! of tying up an agent turn for the duration of generation. The
//! `poll_interval_secs`-spaced `tokio::time::sleep` loop inside
//! `generate_and_wait` is cleanly cancellable (dropped, not aborted mid-
//! syscall) under the `Tool::execute` cancellation contract documented on
//! that trait.

use std::time::Duration;

use agentflow_llm::{AgentFlow, Text2VideoRequest};
use agentflow_tool::{Tool, ToolError, ToolMetadata, ToolOutput, ToolOutputPart};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::common::{
  map_resolution_error, modality_tool_metadata, optional_str, optional_u32, required_str,
};

const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const DEFAULT_MAX_WAIT_SECS: u64 = 300;

/// Generate a video from a text prompt via any text-to-video-capable
/// model declared in the model registry, waiting for the (typically
/// minutes-long) generation job to finish.
pub struct Text2VideoTool;

impl Text2VideoTool {
  pub fn new() -> Self {
    Self
  }
}

impl Default for Text2VideoTool {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl Tool for Text2VideoTool {
  fn name(&self) -> &str {
    "text_to_video"
  }

  fn description(&self) -> &str {
    "Generate a video from a text prompt using a configured text-to-video \
     model. Blocks until the (typically minutes-long) generation job \
     completes or a timeout elapses. Returns the video as a URL reference \
     or inline base64 data."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "model": {
          "type": "string",
          "description": "Text-to-video model name from the model registry (e.g. \"veo-3.1-generate-preview\")."
        },
        "prompt": {
          "type": "string",
          "description": "Text prompt describing the desired video."
        },
        "aspect_ratio": {
          "type": "string",
          "description": "Output aspect ratio, vendor-specific format (e.g. \"16:9\")."
        },
        "duration_seconds": {
          "type": "integer",
          "description": "Output duration in seconds. Vendor-specific allowed values."
        },
        "resolution": {
          "type": "string",
          "description": "Output resolution, vendor-specific format (e.g. \"1080p\")."
        },
        "seed": {
          "type": "integer",
          "description": "Random seed for reproducible generation. Vendor support varies."
        },
        "poll_interval_secs": {
          "type": "integer",
          "description": "How often to poll job status, in seconds. Defaults to 5."
        },
        "max_wait_secs": {
          "type": "integer",
          "description": "Maximum time to wait for the job to finish, in seconds, before failing. Defaults to 300 (5 minutes)."
        }
      },
      "required": ["model", "prompt"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    modality_tool_metadata()
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let model = required_str(&params, "model")?;
    let prompt = required_str(&params, "prompt")?.to_string();

    let provider = AgentFlow::text2video_for(model)
      .await
      .map_err(|err| map_resolution_error("failed to resolve text-to-video model", err))?;

    let request = Text2VideoRequest {
      model: model.to_string(),
      prompt,
      aspect_ratio: optional_str(&params, "aspect_ratio"),
      duration_seconds: optional_u32(&params, "duration_seconds"),
      resolution: optional_str(&params, "resolution"),
      seed: params["seed"].as_i64(),
      extra: None,
    };

    let poll_interval = Duration::from_secs(
      optional_u32(&params, "poll_interval_secs")
        .map(u64::from)
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS),
    );
    let max_wait = Duration::from_secs(
      optional_u32(&params, "max_wait_secs")
        .map(u64::from)
        .unwrap_or(DEFAULT_MAX_WAIT_SECS),
    );

    match provider
      .generate_and_wait(request, poll_interval, max_wait)
      .await
    {
      Ok(response) => {
        let count = response.videos.len();
        let parts: Vec<ToolOutputPart> = response
          .videos
          .into_iter()
          .map(|video| match (video.url, video.b64_data) {
            (Some(url), _) => ToolOutputPart::Resource {
              uri: url,
              mime_type: Some("video/mp4".to_string()),
              text: None,
            },
            (None, Some(b64)) => ToolOutputPart::Resource {
              uri: format!("data:video/mp4;base64,{b64}"),
              mime_type: Some("video/mp4".to_string()),
              text: None,
            },
            (None, None) => ToolOutputPart::Text {
              text: "(video entry carried neither a url nor base64 data)".to_string(),
            },
          })
          .collect();
        Ok(ToolOutput::success_parts(
          format!("Generated {count} video(s)."),
          parts,
        ))
      }
      Err(err) => Ok(ToolOutput::error(format!("video generation failed: {err}"))),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn execute_rejects_missing_prompt() {
    let tool = Text2VideoTool::new();
    let err = tool
      .execute(json!({"model": "veo-3.1-generate-preview"}))
      .await
      .expect_err("missing prompt must fail params validation");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_surfaces_unknown_model_as_invalid_params() {
    let tool = Text2VideoTool::new();
    let err = tool
      .execute(json!({"model": "definitely-not-a-real-model", "prompt": "a cat running"}))
      .await
      .expect_err("unknown model must fail resolution, not panic or hang");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}
