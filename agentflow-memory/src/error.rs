use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },
}

impl From<sqlx::Error> for MemoryError {
    fn from(e: sqlx::Error) -> Self {
        MemoryError::StorageError(e.to_string())
    }
}
