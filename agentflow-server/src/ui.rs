//! Embedded Web UI mount for the single-process debugger.
//!
//! The Vite project lives in `agentflow-ui/`; `dist/` is checked in for now
//! so the server can embed a working UI without requiring Node.js at runtime.

use axum::{
  Router,
  body::Body,
  http::{
    StatusCode,
    header::{CACHE_CONTROL, CONTENT_TYPE},
  },
  response::{Html, IntoResponse, Response},
  routing::get,
};

use crate::AppState;

const INDEX_HTML: &str = include_str!("../../agentflow-ui/dist/index.html");
const APP_JS: &str = include_str!("../../agentflow-ui/dist/assets/app.js");
const STYLES_CSS: &str = include_str!("../../agentflow-ui/dist/assets/styles.css");

/// Static UI routes mounted by [`crate::create_router`].
pub fn ui_router() -> Router<AppState> {
  Router::new()
    .route("/ui", get(index_html))
    .route("/ui/", get(index_html))
    .route("/ui/assets/app.js", get(app_js))
    .route("/ui/assets/styles.css", get(styles_css))
}

/// Serve the SPA shell.
pub async fn index_html() -> Html<&'static str> {
  Html(INDEX_HTML)
}

async fn app_js() -> Response {
  asset_response(APP_JS, "application/javascript; charset=utf-8")
}

async fn styles_css() -> Response {
  asset_response(STYLES_CSS, "text/css; charset=utf-8")
}

/// Build a cacheable static asset response.
pub fn asset_response(body: &'static str, content_type: &'static str) -> Response {
  (
    StatusCode::OK,
    [
      (CONTENT_TYPE, content_type),
      (CACHE_CONTROL, "public, max-age=3600"),
    ],
    Body::from(body),
  )
    .into_response()
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::header;

  #[tokio::test]
  async fn index_contains_app_mount() {
    let Html(html) = index_html().await;
    assert!(html.contains("agentflow-debugger"));
    assert!(html.contains("/ui/assets/app.js"));
  }

  #[test]
  fn asset_response_sets_content_type() {
    let response = asset_response("body", "text/css; charset=utf-8");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
      response.headers().get(header::CONTENT_TYPE),
      Some(
        &"text/css; charset=utf-8"
          .parse()
          .expect("valid header value")
      )
    );
  }
}
