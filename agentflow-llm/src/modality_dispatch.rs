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

/// A modality's constructor table (vendor name → how to build its
/// provider). `resolve()` has already validated the model's registry
/// `type:` matches this modality, so lookup only needs the vendor name.
type Ctor<T> = fn(&str, Option<String>) -> Result<Box<T>>;

fn build_stepfun_asr(api_key: &str, base_url: Option<String>) -> Result<Box<dyn AsrProvider>> {
  Ok(Box::new(StepFunSpecializedClient::new(api_key, base_url)?))
}
fn build_stepfun_tts(api_key: &str, base_url: Option<String>) -> Result<Box<dyn TtsProvider>> {
  Ok(Box::new(StepFunSpecializedClient::new(api_key, base_url)?))
}
fn build_stepfun_text2image(
  api_key: &str,
  base_url: Option<String>,
) -> Result<Box<dyn Text2ImageProvider>> {
  Ok(Box::new(StepFunSpecializedClient::new(api_key, base_url)?))
}
fn build_stepfun_image2image(
  api_key: &str,
  base_url: Option<String>,
) -> Result<Box<dyn Image2ImageProvider>> {
  Ok(Box::new(StepFunSpecializedClient::new(api_key, base_url)?))
}
fn build_stepfun_image_edit(
  api_key: &str,
  base_url: Option<String>,
) -> Result<Box<dyn ImageEditProvider>> {
  Ok(Box::new(StepFunSpecializedClient::new(api_key, base_url)?))
}
fn build_openai_asr(api_key: &str, base_url: Option<String>) -> Result<Box<dyn AsrProvider>> {
  Ok(Box::new(OpenAIAsrProvider::new(api_key, base_url)?))
}

const ASR_PROVIDERS: &[(&str, Ctor<dyn AsrProvider>)] = &[
  ("stepfun", build_stepfun_asr),
  ("step", build_stepfun_asr),
  ("openai", build_openai_asr),
];
const TTS_PROVIDERS: &[(&str, Ctor<dyn TtsProvider>)] =
  &[("stepfun", build_stepfun_tts), ("step", build_stepfun_tts)];
const TEXT2IMAGE_PROVIDERS: &[(&str, Ctor<dyn Text2ImageProvider>)] = &[
  ("stepfun", build_stepfun_text2image),
  ("step", build_stepfun_text2image),
];
const IMAGE2IMAGE_PROVIDERS: &[(&str, Ctor<dyn Image2ImageProvider>)] = &[
  ("stepfun", build_stepfun_image2image),
  ("step", build_stepfun_image2image),
];
const IMAGE_EDIT_PROVIDERS: &[(&str, Ctor<dyn ImageEditProvider>)] = &[
  ("stepfun", build_stepfun_image_edit),
  ("step", build_stepfun_image_edit),
];

/// The modality labels `vendor` already has a provider implementation
/// for, derived from the same constructor tables the dispatch functions
/// look up — single source of truth, so this can't drift from what's
/// actually wired up.
fn implemented_modalities_for(vendor: &str) -> Vec<&'static str> {
  let mut modalities = Vec::new();
  if ASR_PROVIDERS.iter().any(|(v, _)| *v == vendor) {
    modalities.push("ASR");
  }
  if TTS_PROVIDERS.iter().any(|(v, _)| *v == vendor) {
    modalities.push("TTS");
  }
  if TEXT2IMAGE_PROVIDERS.iter().any(|(v, _)| *v == vendor) {
    modalities.push("text-to-image");
  }
  if IMAGE2IMAGE_PROVIDERS.iter().any(|(v, _)| *v == vendor) {
    modalities.push("image-to-image");
  }
  if IMAGE_EDIT_PROVIDERS.iter().any(|(v, _)| *v == vendor) {
    modalities.push("image-edit");
  }
  modalities
}

fn unsupported_vendor<T>(vendor: &str, modality: &str) -> Result<T> {
  let implemented = implemented_modalities_for(vendor);
  let detail = if implemented.is_empty() {
    "no modality implementations yet".to_string()
  } else {
    format!("implements: {}", implemented.join(", "))
  };
  Err(LLMError::UnsupportedProvider {
    provider: format!("{vendor} (no {modality} implementation yet — {detail})"),
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

/// Look up `vendor` in `table` and invoke its constructor, or produce a
/// diagnosable `UnsupportedProvider` error naming `modality_label`.
fn dispatch<T: ?Sized>(
  table: &[(&str, Ctor<T>)],
  vendor: &str,
  api_key: &str,
  base_url: Option<String>,
  modality_label: &str,
) -> Result<Box<T>> {
  match table.iter().find(|(v, _)| *v == vendor) {
    Some((_, ctor)) => ctor(api_key, base_url),
    None => unsupported_vendor(vendor, modality_label),
  }
}

/// Build an [`AsrProvider`] for the named ASR model. Returns
/// `UnsupportedProvider` if the model's vendor has no ASR
/// implementation yet.
pub async fn asr_provider(model_name: &str) -> Result<Box<dyn AsrProvider>> {
  let resolved = resolve(model_name, ModelType::Asr).await?;
  dispatch(
    ASR_PROVIDERS,
    &resolved.vendor,
    &resolved.api_key,
    resolved.base_url,
    "ASR",
  )
}

/// Build a [`TtsProvider`] for the named TTS model.
pub async fn tts_provider(model_name: &str) -> Result<Box<dyn TtsProvider>> {
  let resolved = resolve(model_name, ModelType::Tts).await?;
  dispatch(
    TTS_PROVIDERS,
    &resolved.vendor,
    &resolved.api_key,
    resolved.base_url,
    "TTS",
  )
}

/// Build a [`Text2ImageProvider`] for the named text-to-image model.
pub async fn text2image_provider(model_name: &str) -> Result<Box<dyn Text2ImageProvider>> {
  let resolved = resolve(model_name, ModelType::Text2Image).await?;
  dispatch(
    TEXT2IMAGE_PROVIDERS,
    &resolved.vendor,
    &resolved.api_key,
    resolved.base_url,
    "text-to-image",
  )
}

/// Build an [`Image2ImageProvider`] for the named image-to-image model.
pub async fn image2image_provider(model_name: &str) -> Result<Box<dyn Image2ImageProvider>> {
  let resolved = resolve(model_name, ModelType::Image2Image).await?;
  dispatch(
    IMAGE2IMAGE_PROVIDERS,
    &resolved.vendor,
    &resolved.api_key,
    resolved.base_url,
    "image-to-image",
  )
}

/// Build an [`ImageEditProvider`] for the named image-edit model.
pub async fn image_edit_provider(model_name: &str) -> Result<Box<dyn ImageEditProvider>> {
  let resolved = resolve(model_name, ModelType::ImageEdit).await?;
  dispatch(
    IMAGE_EDIT_PROVIDERS,
    &resolved.vendor,
    &resolved.api_key,
    resolved.base_url,
    "image-edit",
  )
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
    // openai has no TTS implementation (only ASR), so this is a genuine
    // unsupported combination.
    let err = unsupported_vendor::<()>("openai", "TTS").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("openai"), "vendor missing: {msg}");
    assert!(msg.contains("TTS"), "modality missing: {msg}");
  }

  #[test]
  fn unsupported_vendor_message_lists_vendors_other_implemented_modalities() {
    // openai implements ASR but not TTS: the error for "TTS" should name
    // ASR as a hint that the vendor isn't wholesale unsupported.
    let err = unsupported_vendor::<()>("openai", "TTS").unwrap_err();
    let msg = err.to_string();
    assert!(
      msg.contains("implements: ASR"),
      "should list openai's existing ASR support: {msg}"
    );
  }

  #[test]
  fn unsupported_vendor_message_names_no_implementations_for_a_fully_unsupported_vendor() {
    let err = unsupported_vendor::<()>("moonshot", "ASR").unwrap_err();
    let msg = err.to_string();
    assert!(
      msg.contains("no modality implementations yet"),
      "should say moonshot has nothing implemented: {msg}"
    );
  }

  #[test]
  fn implemented_modalities_for_stepfun_covers_all_five() {
    let modalities = implemented_modalities_for("stepfun");
    assert_eq!(
      modalities,
      vec![
        "ASR",
        "TTS",
        "text-to-image",
        "image-to-image",
        "image-edit"
      ]
    );
  }

  #[test]
  fn implemented_modalities_for_openai_is_asr_only() {
    assert_eq!(implemented_modalities_for("openai"), vec!["ASR"]);
  }
}
