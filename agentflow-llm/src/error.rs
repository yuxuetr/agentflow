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

  #[error("API key missing for provider '{provider}'")]
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
  ApiError { provider: String, status_code: u16, message: String },

  #[error("Unsupported operation: {message}")]
  UnsupportedOperation { message: String },
}

pub type Result<T> = std::result::Result<T, LLMError>;

/// Convert common HTTP and network errors to LLMError
impl From<reqwest::Error> for LLMError {
  fn from(error: reqwest::Error) -> Self {
    if error.is_timeout() {
      LLMError::TimeoutError {
        timeout_ms: 30000, // Default timeout
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