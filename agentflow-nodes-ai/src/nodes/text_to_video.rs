use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_llm::{
  AgentFlow, providers::modality::Text2VideoRequest as ModalityText2VideoRequest,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Default poll interval while waiting on a submitted video job.
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Default max wait, used when the node has no `timeout_ms` configured.
/// Veo's own docs cite up to ~6 minutes of peak-usage latency.
const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(360);

/// Text-to-Video generation node.
///
/// Unlike [`super::text_to_image::TextToImageNode`], the underlying
/// provider call is an async job API (submit + poll) that can take
/// minutes rather than seconds — see
/// [`agentflow_llm::Text2VideoProvider::generate_and_wait`].
#[derive(Debug, Clone)]
pub struct TextToVideoNode {
  pub name: String,
  pub model: String,
  pub prompt_template: String,
  pub input_keys: Vec<String>,
  pub output_key: String,

  // Video generation specific parameters
  pub aspect_ratio: Option<String>,
  pub duration_seconds: Option<u32>,
  pub resolution: Option<String>,
  pub seed: Option<i64>,

  // Workflow control
  pub dependencies: Vec<String>,
  pub condition: Option<String>,
  /// Doubles as the provider's `max_wait` — see
  /// `execute_real_video_generation`.
  pub timeout_ms: Option<u64>,
}

impl TextToVideoNode {
  pub fn new(name: &str, model: &str) -> Self {
    Self {
      name: name.to_string(),
      model: model.to_string(),
      prompt_template: String::new(),
      input_keys: Vec::new(),
      output_key: format!("{}_video", name),
      aspect_ratio: None,
      duration_seconds: None,
      resolution: None,
      seed: None,
      dependencies: Vec::new(),
      condition: None,
      timeout_ms: None,
    }
  }

  pub fn with_prompt(mut self, template: &str) -> Self {
    self.prompt_template = template.to_string();
    self
  }

  pub fn with_aspect_ratio(mut self, aspect_ratio: &str) -> Self {
    self.aspect_ratio = Some(aspect_ratio.to_string());
    self
  }

  pub fn with_duration_seconds(mut self, duration_seconds: u32) -> Self {
    self.duration_seconds = Some(duration_seconds);
    self
  }

  pub fn with_resolution(mut self, resolution: &str) -> Self {
    self.resolution = Some(resolution.to_string());
    self
  }

  pub fn with_seed(mut self, seed: i64) -> Self {
    self.seed = Some(seed);
    self
  }

  pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
    self.input_keys = keys;
    self
  }

  pub fn with_output_key(mut self, key: &str) -> Self {
    self.output_key = key.to_string();
    self
  }

  /// Max total wait for the submitted job to complete (submit + poll
  /// loop), not a single HTTP request timeout.
  pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
    self.timeout_ms = Some(timeout_ms);
    self
  }

  /// Resolve template variables in the prompt using inputs. Mirrors
  /// `TextToImageNode::resolve_prompt` exactly.
  fn resolve_prompt(&self, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
    let mut resolved = self.prompt_template.clone();
    for (key, value) in inputs {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder)
        && let FlowValue::Json(Value::String(s)) = value
      {
        resolved = resolved.replace(&placeholder, s);
      }
    }
    Ok(resolved)
  }

  /// Execute real video generation through the modality dispatcher,
  /// submitting the job and polling until it completes.
  async fn execute_real_video_generation(&self, prompt: &str) -> Result<String, AgentFlowError> {
    tracing::debug!(
      model = %self.model,
      prompt = %prompt,
      "executing text-to-video request via modality dispatcher"
    );

    let provider = AgentFlow::text2video_for(&self.model).await.map_err(|e| {
      AgentFlowError::ConfigurationError {
        message: format!(
          "Failed to resolve text-to-video provider for '{}': {}",
          self.model, e
        ),
      }
    })?;

    let request = ModalityText2VideoRequest {
      model: self.model.clone(),
      prompt: prompt.to_string(),
      aspect_ratio: self.aspect_ratio.clone(),
      duration_seconds: self.duration_seconds,
      resolution: self.resolution.clone(),
      seed: self.seed,
      extra: None,
    };

    let max_wait = self
      .timeout_ms
      .map(Duration::from_millis)
      .unwrap_or(DEFAULT_MAX_WAIT);

    let video_response = provider
      .generate_and_wait(request, DEFAULT_POLL_INTERVAL, max_wait)
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError {
        message: format!("Text-to-video generation failed: {}", e),
      })?;

    let first_video =
      video_response
        .videos
        .first()
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No videos returned from text-to-video provider".to_string(),
        })?;

    let result = first_video
      .url
      .clone()
      .ok_or_else(|| AgentFlowError::AsyncExecutionError {
        message: "No video URL returned from provider".to_string(),
      })?;

    tracing::debug!(
      provider = %provider.name(),
      "video generation complete"
    );
    Ok(result)
  }
}

#[async_trait]
impl AsyncNode for TextToVideoNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    if let Some(ref condition) = self.condition
      && let Some(FlowValue::Json(Value::String(cond))) = inputs.get(condition)
      && cond != "true"
    {
      tracing::debug!(
        name = %self.name,
        condition = %cond,
        "skipping TextToVideo node due to condition"
      );
      return Ok(HashMap::new());
    }

    let enriched_prompt = self.resolve_prompt(inputs)?;

    tracing::debug!(
      name = %self.name,
      model = %self.model,
      prompt = %enriched_prompt,
      "TextToVideo node prepared"
    );

    // Upstream failure surfaces as a real error — no mock/placeholder
    // fallback, matching `TextToImageNode`'s Q1.3.3 precedent.
    let response = self
      .execute_real_video_generation(&enriched_prompt)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!(
          "TextToVideo node '{}': video generation failed: {err}",
          self.name
        ),
      })?;

    let mut outputs = HashMap::new();
    outputs.insert(
      self.output_key.clone(),
      FlowValue::Json(Value::String(response)),
    );

    Ok(outputs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Without a configured API key, the real video generation path must
  /// surface a real error — no silent placeholder, mirroring
  /// `TextToImageNode`'s equivalent test.
  #[tokio::test]
  async fn execute_propagates_upstream_failure_instead_of_returning_mock() {
    let node = TextToVideoNode::new("test_gen", "definitely-not-a-real-model")
      .with_prompt("A cat playing piano")
      .with_timeout(500);

    let inputs = AsyncNodeInputs::new();
    let result = node.execute(&inputs).await;
    assert!(
      result.is_err(),
      "upstream failure must propagate; got Ok({:?})",
      result.ok()
    );
  }
}
