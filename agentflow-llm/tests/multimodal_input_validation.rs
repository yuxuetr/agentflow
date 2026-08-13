//! P-LLM2.4 follow-up: exercises the real end-to-end
//! `AgentFlow::model(...).multimodal_prompt(...).execute()` path — through
//! `ModelRegistry::global()` and the mock provider, not just the extracted
//! `validate_request` unit tests in `model_types.rs` — to confirm document
//! (PDF) and video input are validated against a model's registry
//! `accepts:` at request-validation time: allowed when declared, rejected
//! with a clear `LLMError::InvalidModelConfig` (not a mangled request or a
//! silently dropped content block) when not.
//!
//! Single test function mutating the process-global registry singleton —
//! see `llm_retry.rs`'s module doc for why that's safe here (each
//! `tests/*.rs` file is its own process).

use agentflow_llm::multimodal::MultimodalMessage;
use agentflow_llm::{AgentFlow, LLMError, ModelRegistry};

#[tokio::test]
async fn document_and_video_input_validated_against_model_accepts() {
  let yaml = r#"
models:
  mock-with-document:
    vendor: mock
    type: chat
    accepts: [text, document]
  mock-with-video:
    vendor: mock
    type: chat
    accepts: [text, video]
  mock-text-only:
    vendor: mock
    type: chat
    accepts: [text]
"#;
  ModelRegistry::global()
    .load_config_from_yaml(yaml)
    .await
    .expect("load hermetic mock config");

  // Document input succeeds against a model whose `accepts:` declares it.
  let document_message = MultimodalMessage::user()
    .add_text("what are the key findings")
    .add_document_data("aGVsbG8=", "application/pdf")
    .build();
  let result = AgentFlow::model("mock-with-document")
    .multimodal_prompt(document_message.clone())
    .execute()
    .await;
  assert!(
    result.is_ok(),
    "expected document input to be accepted by a model declaring accepts: document, got {result:?}"
  );

  // The same message against a text-only model fails at validation time
  // with a clear, typed error — not a garbled request sent to the provider.
  let result = AgentFlow::model("mock-text-only")
    .multimodal_prompt(document_message)
    .execute()
    .await;
  match result {
    Err(LLMError::InvalidModelConfig { message }) => {
      assert!(
        message.to_lowercase().contains("document"),
        "expected the error to name the rejected modality, got: {message}"
      );
    }
    other => panic!("expected InvalidModelConfig, got {other:?}"),
  }

  // Video input succeeds against a model whose `accepts:` declares it.
  let video_message = MultimodalMessage::user()
    .add_text("summarize this clip")
    .add_video_url("https://example.com/clip.mp4")
    .build();
  let result = AgentFlow::model("mock-with-video")
    .multimodal_prompt(video_message.clone())
    .execute()
    .await;
  assert!(
    result.is_ok(),
    "expected video input to be accepted by a model declaring accepts: video, got {result:?}"
  );

  // The same message against a text-only model fails at validation time.
  let result = AgentFlow::model("mock-text-only")
    .multimodal_prompt(video_message)
    .execute()
    .await;
  match result {
    Err(LLMError::InvalidModelConfig { message }) => {
      assert!(
        message.to_lowercase().contains("video"),
        "expected the error to name the rejected modality, got: {message}"
      );
    }
    other => panic!("expected InvalidModelConfig, got {other:?}"),
  }

  // Cross-modality rejection: document input against the video-only model,
  // and vice versa, must each fail — declaring one non-text modality does
  // not implicitly grant another.
  let document_message = MultimodalMessage::user()
    .add_document_data("aGVsbG8=", "application/pdf")
    .build();
  let result = AgentFlow::model("mock-with-video")
    .multimodal_prompt(document_message)
    .execute()
    .await;
  assert!(
    matches!(result, Err(LLMError::InvalidModelConfig { .. })),
    "expected document input to be rejected by a model that only declares accepts: video, got {result:?}"
  );

  let video_message = MultimodalMessage::user()
    .add_video_url("https://example.com/clip.mp4")
    .build();
  let result = AgentFlow::model("mock-with-document")
    .multimodal_prompt(video_message)
    .execute()
    .await;
  assert!(
    matches!(result, Err(LLMError::InvalidModelConfig { .. })),
    "expected video input to be rejected by a model that only declares accepts: document, got {result:?}"
  );
}
