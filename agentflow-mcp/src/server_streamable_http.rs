//! Streamable HTTP server endpoint — Modern-era (`2026-07-28`) dual-era
//! server support.
//!
//! ## Stability: Experimental
//!
//! Unlike [`crate::server::MCPServer`]'s Beta-pinned stdio surface, this
//! endpoint is new and has zero external validation — see
//! `docs/STABILITY.md`. Header-validation error shapes may still change.
//!
//! W5.8-7 (`docs/RFC_MCP_PROTOCOL_MODERNIZATION.md` Phase 3). This module
//! adds a second, independent entry point to the same
//! [`crate::server::MCPServer::handle_request`] the stdio loop already
//! drives — per the RFC's own model ("A dual-era server MAY serve both
//! eras concurrently on the same endpoint or process"), the existing
//! stdio path (Legacy, `2024-11-05`) stays byte-for-byte unchanged; this
//! endpoint speaks Modern (`2026-07-28`) exclusively, mirroring the
//! client-side split from W5.8-3/4
//! ([`crate::transport::StreamableHttpTransport`] is Modern-only;
//! [`crate::transport::StdioTransport`] is Legacy-only).
//!
//! ## Header validation
//!
//! Per the RFC's "On Streamable HTTP specifically" section, every POST
//! must carry `MCP-Protocol-Version` / `Mcp-Method` / (for `tools/call`
//! only — the sole named method this server implements out of the RFC's
//! `tools/call`/`resources/read`/`prompts/get` trio) `Mcp-Name` headers
//! mirroring the JSON-RPC body:
//!
//! - Missing/wrong `MCP-Protocol-Version` → `400` +
//!   `UnsupportedProtocolVersionError` (`-32022`, listing
//!   `["2026-07-28"]` as supported) — chosen over `HeaderMismatch`
//!   because that's exactly the error shape the client-side
//!   retry-with-supported-version mechanism (W5.8-4) already knows how
//!   to parse and react to.
//! - Header/body protocol-version disagreement (when the body carries a
//!   `params._meta.io.modelcontextprotocol/protocolVersion`), missing/
//!   wrong `Mcp-Method`, or missing/wrong `Mcp-Name` on `tools/call` →
//!   `400` + `HeaderMismatch` (`-32020`).
//!
//! ## Response shape
//!
//! Always a single JSON object (`200` + body, or `202` + empty body for
//! a notification) — **never** an SSE stream. [`MCPServerHandler`]'s
//! `call_tool` is synchronous with no mechanism to emit mid-call
//! `notifications/progress` events, so there is nothing to stream. This
//! is an honest scope limit, not an oversight; revisit if a future
//! handler needs progress events.
//!
//! [`MCPServerHandler`]: crate::server::MCPServerHandler

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult};
use crate::protocol::modern::{
  HEADER_MISMATCH_ERROR_CODE, MCP_PROTOCOL_VERSION_2026_07_28, MODERN_PROTOCOL_VERSION_META_FIELD,
  UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE,
};
use crate::server::MCPServer;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Router, serve};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;

/// HTTP header carrying the protocol version, mirroring the JSON-RPC
/// body's `_meta` field. Same name as the client-side constant in
/// `transport::streamable_http` — duplicated rather than shared because
/// this module lives on the server side of the same wire contract and
/// importing across that boundary would blur which side owns the
/// constant. Kept in sync manually; a shared `protocol::modern` constant
/// would be a reasonable follow-up if a third consumer appears.
const MCP_PROTOCOL_VERSION_HEADER: &str = "MCP-Protocol-Version";
const MCP_METHOD_HEADER: &str = "Mcp-Method";
const MCP_NAME_HEADER: &str = "Mcp-Name";

/// Build a router serving the Streamable HTTP endpoint at `POST /`,
/// matching [`crate::transport::StreamableHttpTransport`], which POSTs
/// directly to its configured `base_url` with no path suffix.
pub fn streamable_http_router(server: Arc<MCPServer>) -> Router {
  Router::new()
    .route("/", post(handle_streamable_http))
    .with_state(server)
}

/// Bind `addr` and serve the Streamable HTTP endpoint until the process
/// is killed. Convenience standalone runner mirroring
/// [`MCPServer::run_stdio`]'s role for HTTP; embedders that want
/// graceful shutdown or to mount this alongside other routes should use
/// [`streamable_http_router`] directly instead.
pub async fn run_streamable_http(server: Arc<MCPServer>, addr: SocketAddr) -> MCPResult<()> {
  let app = streamable_http_router(server);
  let listener = tokio::net::TcpListener::bind(addr)
    .await
    .map_err(|e| MCPError::connection(format!("failed to bind {addr}: {e}")))?;
  serve(listener, app)
    .await
    .map_err(|e| MCPError::transport(format!("streamable HTTP server exited with error: {e}")))?;
  Ok(())
}

fn error_response(
  status: StatusCode,
  id: Option<Value>,
  code: i32,
  message: String,
  data: Option<Value>,
) -> Response {
  let mut error = json!({ "code": code, "message": message });
  if let Some(data) = data {
    error["data"] = data;
  }
  let body = json!({
    "jsonrpc": "2.0",
    "id": id,
    "error": error,
  });
  (status, Json(body)).into_response()
}

async fn handle_streamable_http(
  State(server): State<Arc<MCPServer>>,
  headers: HeaderMap,
  body: Bytes,
) -> Response {
  let body_value: Value = match serde_json::from_slice(&body) {
    Ok(v) => v,
    Err(e) => {
      return error_response(
        StatusCode::BAD_REQUEST,
        None,
        JsonRpcErrorCode::ParseError.code(),
        format!("invalid JSON body: {e}"),
        None,
      );
    }
  };
  let id = body_value.get("id").cloned();

  let header_version = headers
    .get(MCP_PROTOCOL_VERSION_HEADER)
    .and_then(|v| v.to_str().ok());
  if header_version != Some(MCP_PROTOCOL_VERSION_2026_07_28) {
    return error_response(
      StatusCode::BAD_REQUEST,
      id,
      UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE,
      "missing or unsupported MCP-Protocol-Version header".to_string(),
      Some(json!({ "supported": [MCP_PROTOCOL_VERSION_2026_07_28] })),
    );
  }

  if let Some(body_version) = body_value
    .get("params")
    .and_then(|p| p.get("_meta"))
    .and_then(|m| m.get(MODERN_PROTOCOL_VERSION_META_FIELD))
    .and_then(Value::as_str)
    && body_version != MCP_PROTOCOL_VERSION_2026_07_28
  {
    return error_response(
      StatusCode::BAD_REQUEST,
      id,
      HEADER_MISMATCH_ERROR_CODE,
      format!(
        "MCP-Protocol-Version header ({MCP_PROTOCOL_VERSION_2026_07_28}) disagrees with \
         body params._meta protocolVersion ({body_version})"
      ),
      None,
    );
  }

  let body_method = body_value
    .get("method")
    .and_then(Value::as_str)
    .unwrap_or_default();
  let header_method = headers.get(MCP_METHOD_HEADER).and_then(|v| v.to_str().ok());
  if header_method != Some(body_method) {
    return error_response(
      StatusCode::BAD_REQUEST,
      id,
      HEADER_MISMATCH_ERROR_CODE,
      format!("Mcp-Method header ({header_method:?}) disagrees with body method ({body_method:?})"),
      None,
    );
  }

  if body_method == "tools/call" {
    let body_name = body_value
      .get("params")
      .and_then(|p| p.get("name"))
      .and_then(Value::as_str);
    let header_name = headers.get(MCP_NAME_HEADER).and_then(|v| v.to_str().ok());
    if header_name.is_none() || header_name != body_name {
      return error_response(
        StatusCode::BAD_REQUEST,
        id,
        HEADER_MISMATCH_ERROR_CODE,
        format!(
          "Mcp-Name header ({header_name:?}) disagrees with body params.name ({body_name:?})"
        ),
        None,
      );
    }
  }

  match server.handle_request(body_value).await {
    Ok(Some(response)) => (StatusCode::OK, Json(response)).into_response(),
    Ok(None) => StatusCode::ACCEPTED.into_response(),
    Err(e) => error_response(
      StatusCode::BAD_REQUEST,
      id,
      e.json_rpc_code()
        .unwrap_or(JsonRpcErrorCode::InternalError.code()),
      e.to_string(),
      None,
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::server::AgentFlowServerHandler;

  fn test_server() -> Arc<MCPServer> {
    Arc::new(MCPServer::new(Box::new(AgentFlowServerHandler::new())))
  }

  #[test]
  fn router_builds_without_panicking() {
    let _router = streamable_http_router(test_server());
  }
}
