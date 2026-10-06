//! Error types for the RAG system

/// Result type alias for RAG operations
pub type Result<T> = std::result::Result<T, RAGError>;

/// Main error type for RAG operations
#[derive(Debug, thiserror::Error)]
pub enum RAGError {
  /// Vector store related errors
  #[error("Vector store error: {message}")]
  VectorStoreError { message: String },

  /// Embedding generation errors
  #[error("Embedding error: {message}")]
  EmbeddingError { message: String },

  /// Document processing errors
  #[error("Document processing error: {message}")]
  DocumentError { message: String },

  /// Chunking strategy errors
  #[error("Chunking error: {message}")]
  ChunkingError { message: String },

  /// Indexing pipeline errors
  #[error("Indexing error: {message}")]
  IndexingError { message: String },

  /// Retrieval errors
  #[error("Retrieval error: {message}")]
  RetrievalError { message: String },

  /// Configuration errors
  #[error("Configuration error: {message}")]
  ConfigurationError { message: String },

  /// Connection errors
  #[error("Connection error: {message}")]
  ConnectionError { message: String },

  /// API errors (for external services)
  #[error("API error: {status}: {message}")]
  ApiError { status: u16, message: String },

  /// Timeout errors
  #[error("Operation timed out: {operation}")]
  TimeoutError { operation: String },

  /// Not found errors
  #[error("Not found: {resource}")]
  NotFoundError { resource: String },

  /// Already exists errors
  #[error("Already exists: {resource}")]
  AlreadyExistsError { resource: String },

  /// Invalid input errors
  #[error("Invalid input: {message}")]
  InvalidInputError { message: String },

  /// Serialization/deserialization errors
  #[error("Serialization error: {0}")]
  SerializationError(#[from] serde_json::Error),

  /// I/O errors
  #[error("I/O error: {0}")]
  IoError(#[from] std::io::Error),

  /// HTTP request errors
  #[error("HTTP error: {0}")]
  HttpError(#[from] reqwest::Error),

  /// CSV parsing errors
  #[error("CSV error: {0}")]
  CsvError(#[from] csv::Error),

  /// Generic error for catch-all cases
  #[error("RAG error: {0}")]
  GenericError(String),
}

impl RAGError {
  /// Create a vector store error
  pub fn vector_store<S: Into<String>>(message: S) -> Self {
    RAGError::VectorStoreError {
      message: message.into(),
    }
  }

  /// Create an embedding error
  pub fn embedding<S: Into<String>>(message: S) -> Self {
    RAGError::EmbeddingError {
      message: message.into(),
    }
  }

  /// Create a document processing error
  pub fn document<S: Into<String>>(message: S) -> Self {
    RAGError::DocumentError {
      message: message.into(),
    }
  }

  /// Create a chunking error
  pub fn chunking<S: Into<String>>(message: S) -> Self {
    RAGError::ChunkingError {
      message: message.into(),
    }
  }

  /// Create an indexing error
  pub fn indexing<S: Into<String>>(message: S) -> Self {
    RAGError::IndexingError {
      message: message.into(),
    }
  }

  /// Create a retrieval error
  pub fn retrieval<S: Into<String>>(message: S) -> Self {
    RAGError::RetrievalError {
      message: message.into(),
    }
  }

  /// Create a configuration error
  pub fn configuration<S: Into<String>>(message: S) -> Self {
    RAGError::ConfigurationError {
      message: message.into(),
    }
  }

  /// Create a connection error
  pub fn connection<S: Into<String>>(message: S) -> Self {
    RAGError::ConnectionError {
      message: message.into(),
    }
  }

  /// Create an API error
  pub fn api<S: Into<String>>(status: u16, message: S) -> Self {
    RAGError::ApiError {
      status,
      message: message.into(),
    }
  }

  /// Create a timeout error
  pub fn timeout<S: Into<String>>(operation: S) -> Self {
    RAGError::TimeoutError {
      operation: operation.into(),
    }
  }

  /// Create a not found error
  pub fn not_found<S: Into<String>>(resource: S) -> Self {
    RAGError::NotFoundError {
      resource: resource.into(),
    }
  }

  /// Create an already exists error
  pub fn already_exists<S: Into<String>>(resource: S) -> Self {
    RAGError::AlreadyExistsError {
      resource: resource.into(),
    }
  }

  /// Create an invalid input error
  pub fn invalid_input<S: Into<String>>(message: S) -> Self {
    RAGError::InvalidInputError {
      message: message.into(),
    }
  }

  /// Check if this is a transient error that might succeed on retry
  pub fn is_transient(&self) -> bool {
    match self {
      RAGError::ConnectionError { .. } => true,
      RAGError::TimeoutError { .. } => true,
      RAGError::ApiError { status, .. } => *status >= 500,
      _ => false,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_construction() {
    let err = RAGError::vector_store("Test error");
    assert!(matches!(err, RAGError::VectorStoreError { .. }));
  }

  #[test]
  fn test_transient_errors() {
    assert!(RAGError::connection("timeout").is_transient());
    assert!(RAGError::timeout("search").is_transient());
    assert!(RAGError::api(503, "service unavailable").is_transient());
    assert!(!RAGError::api(404, "not found").is_transient());
    assert!(!RAGError::invalid_input("bad param").is_transient());
  }
}
