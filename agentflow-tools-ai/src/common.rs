//! Shared helpers for the modality `Tool` adapters in this crate.
//!
//! Every tool in this crate follows the same three-phase shape:
//! parse `params` defensively (never `unwrap`/`panic` on caller input),
//! resolve + call the matching `agentflow_llm::AgentFlow::*` dispatch
//! function, then translate the response into a [`ToolOutput`]. These
//! helpers cover the parts identical across all of them so each tool file
//! only has to state what's actually modality-specific.

use agentflow_llm::ImageGenerationResponse;
use agentflow_tool::{
  ToolError, ToolIdempotency, ToolMetadata, ToolOutput, ToolOutputPart, ToolPermission,
  ToolPermissionSet, ToolSource,
};
use base64::Engine;
use serde_json::Value;

/// Metadata shared by every tool in this crate: all of them are billed,
/// non-replayable-safe calls to a vendor HTTP API (same bucket as the
/// built-in `shell`/`code_exec` tools), reached over the network.
///
/// Built directly rather than via `ToolMetadata::builtin_named` — that
/// helper's `ToolPermissionSet::builtin`/`builtin_tool_idempotency` match
/// only knows the tool-tier names (`shell`/`file`/`http`/`code_exec`); it
/// has no reason to know about modality tool names, and teaching the L0
/// kernel crate about them would be the wrong direction of dependency.
pub(crate) fn modality_tool_metadata() -> ToolMetadata {
  ToolMetadata {
    source: ToolSource::Builtin,
    permissions: ToolPermissionSet::new([ToolPermission::Network]),
    idempotency: ToolIdempotency::NonIdempotent,
    mcp_server_name: None,
    mcp_tool_name: None,
  }
}

/// Read a required string field, or a clear `InvalidParams` error naming
/// the missing/malformed field.
pub(crate) fn required_str<'a>(params: &'a Value, field: &str) -> Result<&'a str, ToolError> {
  params[field]
    .as_str()
    .filter(|s| !s.is_empty())
    .ok_or_else(|| ToolError::InvalidParams {
      message: format!("missing or empty required field `{field}`"),
    })
}

pub(crate) fn optional_str(params: &Value, field: &str) -> Option<String> {
  params[field].as_str().map(str::to_string)
}

pub(crate) fn optional_u32(params: &Value, field: &str) -> Option<u32> {
  params[field].as_u64().and_then(|n| u32::try_from(n).ok())
}

pub(crate) fn optional_i32(params: &Value, field: &str) -> Option<i32> {
  params[field].as_i64().and_then(|n| i32::try_from(n).ok())
}

pub(crate) fn optional_f32(params: &Value, field: &str) -> Option<f32> {
  params[field].as_f64().map(|n| n as f32)
}

/// Decode a required base64 field into raw bytes, or a clear
/// `InvalidParams` error — distinguishing "field missing" from "field
/// present but not valid base64" so the caller (often an LLM agent) gets
/// an actionable message either way.
pub(crate) fn required_base64(params: &Value, field: &str) -> Result<Vec<u8>, ToolError> {
  let encoded = required_str(params, field)?;
  base64::engine::general_purpose::STANDARD
    .decode(encoded)
    .map_err(|err| ToolError::InvalidParams {
      message: format!("field `{field}` is not valid base64: {err}"),
    })
}

pub(crate) fn encode_base64(data: &[u8]) -> String {
  base64::engine::general_purpose::STANDARD.encode(data)
}

/// Map an `AgentFlow::<modality>(model)` dispatch failure (unknown model,
/// vendor doesn't implement this modality, missing API key) into
/// `InvalidParams` — from the caller's perspective this is almost always
/// "you named a `model` this tool can't use", which is actionable the same
/// way a malformed parameter is: try a different `model` value. Contrast
/// with a failure from the vendor *call itself* (after resolution
/// succeeded), which each tool surfaces as a soft `ToolOutput::error(...)`
/// instead — see the module docs on this distinction in `lib.rs`.
pub(crate) fn map_resolution_error(context: &str, err: agentflow_llm::LLMError) -> ToolError {
  ToolError::InvalidParams {
    message: format!("{context}: {err}"),
  }
}

/// Translate an [`ImageGenerationResponse`] (shared by the text-to-image,
/// image-to-image, and image-edit modalities) into a [`ToolOutput`].
///
/// Each [`agentflow_llm::GeneratedImage`] carries exactly one of `url` /
/// `b64_json`, mirroring the request's `response_format` choice
/// (`"url"` vs `"b64_json"`) — a URL is preferred when present (a short
/// reference, not a multi-KB blob, keeps an agent's conversation context
/// small), falling back to embedding the base64 bytes inline via
/// [`ToolOutputPart::Image`] when the vendor only returned that.
pub(crate) fn image_generation_output(
  label: &str,
  response: ImageGenerationResponse,
) -> ToolOutput {
  let count = response.images.len();
  let parts: Vec<ToolOutputPart> = response
    .images
    .into_iter()
    .map(|image| match (image.url, image.b64_json) {
      (Some(url), _) => ToolOutputPart::Resource {
        uri: url,
        mime_type: None,
        text: None,
      },
      (None, Some(b64)) => ToolOutputPart::Image {
        data: b64,
        // Vendors don't report a MIME type alongside `b64_json`; PNG is
        // the near-universal default across every provider this crate's
        // dispatch tables route to (OpenAI, Google, StepFun, DashScope,
        // GLM all default image output to PNG).
        mime_type: "image/png".to_string(),
      },
      (None, None) => ToolOutputPart::Text {
        text: "(image entry carried neither a url nor base64 data)".to_string(),
      },
    })
    .collect();
  ToolOutput::success_parts(format!("Generated {count} image(s) via {label}."), parts)
}
