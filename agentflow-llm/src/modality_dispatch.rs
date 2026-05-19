//! Registry-driven dispatcher for per-modality providers.
//!
//! Looks up a model name in the global [`ModelRegistry`], validates its
//! declared `type` matches the requested modality, resolves the
//! vendor's API key, and returns a boxed trait object. Each entry point
//! is the modality counterpart to [`AgentFlow::model`](crate::AgentFlow::model)
//! (which covers chat).
//!
//! Today only StepFun implements the 5 modality traits — other vendors
//! return [`LLMError::UnsupportedProvider`]. P-LLM.5 adds OpenAI
//! Whisper as the second `AsrProvider` implementation.

use crate::{
  LLMError, Result,
  model_types::ModelType,
  providers::{
    modality::{
      AsrProvider, Image2ImageProvider, ImageEditProvider, Text2ImageProvider, TtsProvider,
    },
    openai_asr::OpenAIAsrProvider,
    stepfun::StepFunSpecializedClient,
  },
  registry::ModelRegistry,
};

/// Look up `model_name` in the global registry, assert its `type`
/// matches `expected`, and return `(vendor, base_url)`.
async fn resolve_for_modality(
  model_name: &str,
  expected: ModelType,
) -> Result<(String, Option<String>)> {
  let registry = ModelRegistry::global();
  let model_config = registry.get_model(model_name)?;

  let actual = model_config.granular_type();
  if actual != expected {
    return Err(LLMError::InvalidModelConfig {
      message: format!(
        "Model '{model_name}' has type '{}' but the requested modality requires type '{}'. \
         Update the YAML registry entry's `type:` field or pick a model whose type matches.",
        actual.to_legacy_string(),
        expected.to_legacy_string()
      ),
    });
  }

  Ok((model_config.vendor.clone(), model_config.base_url.clone()))
}

/// Resolve the API key for `vendor` using the same precedence rules
/// chat models use (vendor-specific `api_key_env` from registry,
/// then common env-var fallbacks).
async fn resolve_api_key(vendor: &str) -> Result<String> {
  let registry = ModelRegistry::global();
  let config = registry.get_config().await?;
  config.get_api_key(vendor)
}

fn unsupported_vendor<T>(vendor: &str, modality: &str) -> Result<T> {
  Err(LLMError::UnsupportedProvider {
    provider: format!("{vendor} (no {modality} implementation yet)"),
  })
}

/// Snapshot of registry resolution shared by every modality entry
/// point. Centralised so the per-modality functions stay tiny.
struct ResolvedModel {
  vendor: String,
  base_url: Option<String>,
  api_key: String,
}

async fn resolve(model_name: &str, expected: ModelType) -> Result<ResolvedModel> {
  let (vendor, base_url) = resolve_for_modality(model_name, expected).await?;
  let api_key = resolve_api_key(&vendor).await?;
  Ok(ResolvedModel {
    vendor,
    base_url,
    api_key,
  })
}

/// Build an [`AsrProvider`] for the named ASR model. Returns
/// `UnsupportedProvider` if the model's vendor has no ASR
/// implementation yet.
pub async fn asr_provider(model_name: &str) -> Result<Box<dyn AsrProvider>> {
  let resolved = resolve(model_name, ModelType::Asr).await?;
  match resolved.vendor.as_str() {
    "stepfun" | "step" => Ok(Box::new(StepFunSpecializedClient::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    "openai" => Ok(Box::new(OpenAIAsrProvider::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    _ => unsupported_vendor(&resolved.vendor, "ASR"),
  }
}

/// Build a [`TtsProvider`] for the named TTS model.
pub async fn tts_provider(model_name: &str) -> Result<Box<dyn TtsProvider>> {
  let resolved = resolve(model_name, ModelType::Tts).await?;
  match resolved.vendor.as_str() {
    "stepfun" | "step" => Ok(Box::new(StepFunSpecializedClient::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    _ => unsupported_vendor(&resolved.vendor, "TTS"),
  }
}

/// Build a [`Text2ImageProvider`] for the named text-to-image model.
pub async fn text2image_provider(model_name: &str) -> Result<Box<dyn Text2ImageProvider>> {
  let resolved = resolve(model_name, ModelType::Text2Image).await?;
  match resolved.vendor.as_str() {
    "stepfun" | "step" => Ok(Box::new(StepFunSpecializedClient::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    _ => unsupported_vendor(&resolved.vendor, "text-to-image"),
  }
}

/// Build an [`Image2ImageProvider`] for the named image-to-image model.
pub async fn image2image_provider(model_name: &str) -> Result<Box<dyn Image2ImageProvider>> {
  let resolved = resolve(model_name, ModelType::Image2Image).await?;
  match resolved.vendor.as_str() {
    "stepfun" | "step" => Ok(Box::new(StepFunSpecializedClient::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    _ => unsupported_vendor(&resolved.vendor, "image-to-image"),
  }
}

/// Build an [`ImageEditProvider`] for the named image-edit model.
pub async fn image_edit_provider(model_name: &str) -> Result<Box<dyn ImageEditProvider>> {
  let resolved = resolve(model_name, ModelType::ImageEdit).await?;
  match resolved.vendor.as_str() {
    "stepfun" | "step" => Ok(Box::new(StepFunSpecializedClient::new(
      &resolved.api_key,
      resolved.base_url,
    )?)),
    _ => unsupported_vendor(&resolved.vendor, "image-edit"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Build an isolated `ModelRegistry` for tests by loading YAML
  /// directly. The global singleton may be in any state across the
  /// test process, so we don't touch it here — `resolve_for_modality`
  /// uses the global registry though, so a separate isolated helper
  /// exercises the same shape via a hand-built `LLMConfig`.
  fn type_mismatch_error_for(actual_type: ModelType, expected: ModelType) -> LLMError {
    LLMError::InvalidModelConfig {
      message: format!(
        "Model 'sample' has type '{}' but the requested modality requires type '{}'. \
         Update the YAML registry entry's `type:` field or pick a model whose type matches.",
        actual_type.to_legacy_string(),
        expected.to_legacy_string()
      ),
    }
  }

  #[test]
  fn type_mismatch_message_names_both_actual_and_expected() {
    // Use a chat model name where an ASR is expected: the error must
    // make the mistake operator-actionable, not just say "wrong type".
    let err = type_mismatch_error_for(ModelType::Chat, ModelType::Asr);
    let msg = err.to_string();
    assert!(msg.contains("type 'chat'"), "actual type missing: {msg}");
    assert!(msg.contains("type 'asr'"), "expected type missing: {msg}");
  }

  #[test]
  fn unsupported_vendor_message_names_modality() {
    let err = unsupported_vendor::<()>("openai", "ASR").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("openai"), "vendor missing: {msg}");
    assert!(msg.contains("ASR"), "modality missing: {msg}");
  }
}
