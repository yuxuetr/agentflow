//! Unified error envelope for all gateway routes.
//!
//! Every error response — auth failure, validation error, database error,
//! upstream LLM/tool failure — produces this shape:
//!
//! ```json
//! {
//!   "error": {
//!     "code": "not_found",
//!     "message": "run 9f3f… not found",
//!     "details": null
//!   }
//! }
//! ```
//!
//! `code` is a stable, snake-cased identifier callers can branch on.
//! `details` is an optional structured payload (validation errors, etc.).

use axum::{
  Json,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
  error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
  code: &'static str,
  message: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  details: Option<Value>,
}

/// API errors returned to the client. New variants must map to a stable
/// `code` string in [`ApiError::error_code`].
#[derive(Debug, Error)]
pub enum ApiError {
  #[error("Database error: {0}")]
  Database(#[from] agentflow_db::error::DbError),

  #[error("Internal server error: {0}")]
  Internal(String),

  #[error("Not found: {0}")]
  NotFound(String),

  #[error("Bad request: {0}")]
  BadRequest(String),

  /// Authentication header missing or malformed.
  #[error("Missing or malformed Authorization header")]
  Unauthorized,

  /// Authentication header present but token doesn't match.
  #[error("Forbidden: invalid token")]
  Forbidden,

  /// Server has no auth configured but a request reached an authenticated
  /// route. Distinct from `Unauthorized` so operators can see a config gap.
  #[error("Server misconfiguration: {0}")]
  Misconfigured(String),
}

impl ApiError {
  fn status_code(&self) -> StatusCode {
    match self {
      ApiError::Database(_) | ApiError::Internal(_) | ApiError::Misconfigured(_) => {
        StatusCode::INTERNAL_SERVER_ERROR
      }
      ApiError::NotFound(_) => StatusCode::NOT_FOUND,
      ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
      ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
      ApiError::Forbidden => StatusCode::FORBIDDEN,
    }
  }

  fn error_code(&self) -> &'static str {
    match self {
      ApiError::Database(_) => "database_error",
      ApiError::Internal(_) => "internal_error",
      ApiError::NotFound(_) => "not_found",
      ApiError::BadRequest(_) => "bad_request",
      ApiError::Unauthorized => "unauthorized",
      ApiError::Forbidden => "forbidden",
      ApiError::Misconfigured(_) => "server_misconfigured",
    }
  }
}

impl IntoResponse for ApiError {
  fn into_response(self) -> Response {
    let status = self.status_code();
    let body = ErrorEnvelope {
      error: ErrorBody {
        code: self.error_code(),
        message: self.to_string(),
        details: None,
      },
    };
    (status, Json(body)).into_response()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::StatusCode;

  fn extract_body(resp: Response) -> (StatusCode, serde_json::Value) {
    let status = resp.status();
    // Synchronous body extraction is enough for these unit tests because
    // `IntoResponse` here always emits an in-memory JSON payload.
    let body = futures::executor::block_on(async {
      axum::body::to_bytes(resp.into_body(), 4096).await.unwrap()
    });
    (status, serde_json::from_slice(&body).unwrap())
  }

  #[test]
  fn not_found_serialises_with_code_and_message() {
    let (status, body) = extract_body(ApiError::NotFound("run xyz".into()).into_response());
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "Not found: run xyz");
  }

  #[test]
  fn unauthorized_uses_401_and_stable_code() {
    let (status, body) = extract_body(ApiError::Unauthorized.into_response());
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
  }

  #[test]
  fn bad_request_passes_message_through() {
    let (status, body) =
      extract_body(ApiError::BadRequest("missing field 'workflow'".into()).into_response());
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
    assert!(
      body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("missing field")
    );
  }
}
