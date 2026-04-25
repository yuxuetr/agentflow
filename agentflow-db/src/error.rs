use thiserror::Error;

/// Database related errors.
#[derive(Debug, Error)]
pub enum DbError {
  #[error("Database connection error: {0}")]
  ConnectionError(#[from] sqlx::Error),
  #[error("Database configuration error: {message}")]
  ConfigError { message: String },
  #[error("Entity not found: {entity_type} with id {id}")]
  NotFound {
    entity_type: &'static str,
    id: String,
  },
}
