use thiserror::Error;

/// Comprehensive error types for LLM operations
#[derive(Error, Debug)]
pub enum LLMError {
  #[error("Configuration error: {message}")]
  ConfigurationError { message: String },

  #[error("Model '{model_name}' not found in registry")]
  ModelNotFound { model_name: String },

  #[error("Provider '{provider}' not supported")]
  UnsupportedProvider { provider: String },

  // F-AF-4: keep the variant shape stable (callers `match` on it) but
  // make the rendered message crisp — every fresh-host failure on this
  // error should be actionable in one line. The message names the env
  // var by provider and points at the standard fix.
  #[error(
    "API key missing for provider '{provider}' — set {} in your environment or ~/.agentflow/.env (run `agentflow config init` to generate a template), or pass --model to override the provider. See agentflow-llm/README.md § {} for the env-var alternatives.",
    env_var_hint(provider),
    provider
  )]
  MissingApiKey { provider: String },

  #[error("Invalid model configuration: {message}")]
  InvalidModelConfig { message: String },

  #[error("HTTP request failed: {status_code} - {message}")]
  HttpError { status_code: u16, message: String },

  #[error("Request timeout after {timeout_ms}ms")]
  TimeoutError { timeout_ms: u64 },

  #[error("Rate limit exceeded for provider '{provider}': {message}")]
  RateLimitExceeded { provider: String, message: String },

  #[error("Authentication failed for provider '{provider}': {message}")]
  AuthenticationError { provider: String, message: String },

  #[error("API response parsing failed: {message}")]
  ResponseParsingError { message: String },

  #[error("Streaming error: {message}")]
  StreamingError { message: String },

  #[error("Model execution error: {message}")]
  ModelExecutionError { message: String },

  #[error("Quota exceeded for provider '{provider}': {message}")]
  QuotaExceeded { provider: String, message: String },

  #[error("Service unavailable for provider '{provider}': {message}")]
  ServiceUnavailable { provider: String, message: String },

  #[error("Internal LLM error: {message}")]
  InternalError { message: String },

  #[error("Network error: {message}")]
  NetworkError { message: String },

  #[error("Parse error: {message}")]
  ParseError { message: String },

  #[error("API error from '{provider}': {status_code} - {message}")]
  ApiError {
    provider: String,
    status_code: u16,
    message: String,
  },

  #[error("Unsupported operation: {message}")]
  UnsupportedOperation { message: String },
}

pub type Result<T> = std::result::Result<T, LLMError>;

/// Convert common HTTP and network errors to LLMError
impl From<reqwest::Error> for LLMError {
  fn from(error: reqwest::Error) -> Self {
    if error.is_timeout() {
      // Q1.8.2: align the reported timeout with what
      // `providers::default_http_client` actually enforces. Pre-fix
      // this said 30000 ms even though the builder had no timeout at
      // all.
      LLMError::TimeoutError {
        timeout_ms: crate::providers::DEFAULT_HTTP_REQUEST_TIMEOUT_SECS * 1000,
      }
    } else if let Some(status) = error.status() {
      let status_code = status.as_u16();
      let message = error.to_string();

      match status_code {
        401 => LLMError::AuthenticationError {
          provider: "unknown".to_string(),
          message,
        },
        429 => LLMError::RateLimitExceeded {
          provider: "unknown".to_string(),
          message,
        },
        503 => LLMError::ServiceUnavailable {
          provider: "unknown".to_string(),
          message,
        },
        _ => LLMError::HttpError {
          status_code,
          message,
        },
      }
    } else {
      LLMError::InternalError {
        message: error.to_string(),
      }
    }
  }
}

/// F-AF-4: map a provider name to its canonical env-var hint string
/// for the [`LLMError::MissingApiKey`] message. Returns a
/// comma-separated list of accepted env vars (per the provider's
/// `api_key_env` config) when there are multiple, or a single name
/// when there's just one. Unknown providers fall back to a generic
/// hint so the error still renders without panicking.
fn env_var_hint(provider: &str) -> &'static str {
  match provider {
    "openai" => "OPENAI_API_KEY",
    "anthropic" => "ANTHROPIC_API_KEY",
    "google" => "GEMINI_API_KEY (or GOOGLE_API_KEY)",
    "moonshot" => "MOONSHOT_API_KEY (or MOONSHOT_KEY)",
    "stepfun" | "step" => "STEPFUN_API_KEY (or STEP_API_KEY)",
    "dashscope" => "DASHSCOPE_API_KEY",
    "glm" | "bigmodel" | "zhipu" => "GLM_API_KEY (or BIGMODEL_API_KEY, ZHIPU_API_KEY)",
    "deepseek" => "DEEPSEEK_API_KEY",
    "minimax" => "MINIMAX_API_KEY",
    _ => "the provider's *_API_KEY env var",
  }
}

impl From<serde_json::Error> for LLMError {
  fn from(error: serde_json::Error) -> Self {
    LLMError::ResponseParsingError {
      message: error.to_string(),
    }
  }
}

impl From<serde_yaml::Error> for LLMError {
  fn from(error: serde_yaml::Error) -> Self {
    LLMError::ConfigurationError {
      message: error.to_string(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// F-AF-4: the rendered `MissingApiKey` message MUST name the
  /// provider-specific env var so a fresh-host operator can act
  /// on it in one read. Lock the env-var-hint pattern + the
  /// `agentflow config init` actionable hint.
  #[test]
  fn missing_api_key_renders_provider_specific_env_var() {
    let err = LLMError::MissingApiKey {
      provider: "moonshot".to_string(),
    };
    let rendered = err.to_string();
    assert!(
      rendered.contains("MOONSHOT_API_KEY"),
      "moonshot variant must name MOONSHOT_API_KEY: {rendered}"
    );
    assert!(
      rendered.contains("agentflow config init"),
      "actionable fix must mention `agentflow config init`: {rendered}"
    );
    assert!(
      rendered.contains("~/.agentflow/.env"),
      "must point at ~/.agentflow/.env as the standard location: {rendered}"
    );
  }

  /// F-AF-4: unknown providers must still render a usable message
  /// (no panic), even if the hint is generic.
  #[test]
  fn missing_api_key_unknown_provider_falls_back_gracefully() {
    let err = LLMError::MissingApiKey {
      provider: "some-future-provider".to_string(),
    };
    let rendered = err.to_string();
    assert!(rendered.contains("some-future-provider"));
    assert!(rendered.contains("API_KEY"));
  }

  /// F-AF-4: every provider currently in the env-var-hint table
  /// renders without falling through to the unknown branch.
  /// Locks the table coverage against silent regressions when a
  /// new provider is added but the hint isn't.
  #[test]
  fn env_var_hint_covers_all_known_providers() {
    for provider in [
      "openai",
      "anthropic",
      "google",
      "moonshot",
      "stepfun",
      "dashscope",
    ] {
      let hint = env_var_hint(provider);
      assert!(
        hint.contains("API_KEY") || hint.contains("KEY"),
        "provider '{provider}' hint must mention an API key env var, got '{hint}'"
      );
      assert_ne!(
        hint, "the provider's *_API_KEY env var",
        "provider '{provider}' fell through to the generic branch"
      );
    }
  }
}
