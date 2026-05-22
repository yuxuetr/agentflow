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
  extract::{FromRequest, Request, rejection::JsonRejection},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Serialize;
use serde::de::DeserializeOwned;
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
/// `code` string in `ApiError::error_code`.
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

  /// Request body exceeded the configured size limit. Surfaced through
  /// the unified envelope (HTTP 413) instead of being collapsed into
  /// `BadRequest` so clients can branch on the size-limit cause.
  #[error("Payload too large: {0}")]
  PayloadTooLarge(String),
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
      ApiError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
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
      ApiError::PayloadTooLarge(_) => "payload_too_large",
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

/// JSON body extractor that maps deserialization failures into the
/// unified [`ApiError`] envelope instead of axum's default plain-text
/// rejection. Drop-in replacement for `Json<T>` in route handler
/// signatures. Without this wrapper, malformed bodies / missing fields
/// / unknown enum variants return raw `Failed to deserialize the JSON
/// body…` text bodies, violating the `code` / `message` envelope
/// callers branch on (see `docs/DEPLOYMENT.md` "Unified error envelope").
pub struct JsonReq<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for JsonReq<T>
where
  S: Send + Sync,
  T: DeserializeOwned,
{
  type Rejection = ApiError;

  async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
    match Json::<T>::from_request(req, state).await {
      Ok(Json(value)) => Ok(JsonReq(value)),
      Err(rejection) => Err(json_rejection_to_api_error(rejection)),
    }
  }
}

fn json_rejection_to_api_error(rejection: JsonRejection) -> ApiError {
  // `body_text()` carries the actionable detail (which field is missing,
  // which enum variant is unknown, byte offset of a syntax error). Keep
  // it as `message` so clients can still surface the underlying cause.
  let message = rejection.body_text();
  // Preserve the 413 status that `DefaultBodyLimit` produces. Without
  // this branch, an oversized body would be collapsed into `BadRequest`
  // (HTTP 400) and clients couldn't branch on the size-limit cause —
  // and the existing `body_limit_rejects_oversized_json_before_handler`
  // contract tests would fail.
  if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
    return ApiError::PayloadTooLarge(message);
  }
  ApiError::BadRequest(message)
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

  #[tokio::test]
  async fn json_req_malformed_body_yields_unified_envelope() {
    use axum::{Router, extract::State, routing::post};
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct Body {
      #[allow(dead_code)]
      field: String,
    }

    async fn handler(_state: State<()>, JsonReq(_body): JsonReq<Body>) -> StatusCode {
      StatusCode::OK
    }

    let app = Router::new().route("/", post(handler)).with_state(());

    let req = axum::http::Request::builder()
      .method("POST")
      .uri("/")
      .header("content-type", "application/json")
      .body(axum::body::Body::from(r#"{"field": 123}"#))
      .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "bad_request");
    let msg = body["error"]["message"].as_str().unwrap();
    // Underlying serde detail (type-mismatch on `field`) must survive
    // into the envelope so clients can still see the cause.
    assert!(msg.contains("field"), "message lacked field detail: {msg}");
  }
}
