//! P3.8 cross-hop W3C traceparent end-to-end acceptance.
//!
//! Each of the four AgentFlow process / protocol hops already has
//! per-hop unit + integration coverage:
//!
//! - LLM HTTP header — `agentflow-llm/src/trace_context.rs::tests` +
//!   the per-provider consistency tests
//! - Plugin subprocess env — `agentflow-cli/tests/plugin_traceparent_tests.rs`
//! - MCP transport — `agentflow-mcp/tests/traceparent_propagation.rs`
//! - Worker gRPC — `agentflow-server/src/scheduler/grpc.rs::traceparent_tests`
//!
//! This file is the **single** acceptance test that proves the four
//! carriers, when fired inside one shared
//! `agentflow_tracing::context::scope`, agree on the wire value. It's
//! deliberately hermetic — no live HTTP, no live MCP server, no real
//! plugin binary, no tonic Channel. The injection helpers are
//! exercised against in-memory `HeaderMap` / `JsonRpcRequest` /
//! tonic `Request<T>` / `tokio::process::Command` instances, and the
//! captured carrier values are compared byte-for-byte.
//!
//! The "no network" property is what lets this test live in the
//! default CI run instead of being gated on `AGENTFLOW_LIVE_*` env
//! vars. Live traces also flow end-to-end during the existing
//! provider-consistency / harness E2E suites; this file is the
//! "always-on" guard against any future per-hop divergence.

use agentflow_llm::{LlmTraceContext, trace_context as llm_trace};
use agentflow_mcp::protocol::{
  JsonRpcRequest, RequestId, extract_traceparent_from_request, inject_traceparent_into_request,
};
use agentflow_server::scheduler::grpc::{
  extract_traceparent_from_grpc_request, inject_traceparent_into_grpc_request,
};
use reqwest::header::HeaderMap;
use serde_json::json;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command as TokioCommand;
use tonic::Request;

/// Pinned traceparent for the test. Format matches the W3C grammar:
/// `<version>-<trace-id (32 hex)>-<span-id (16 hex)>-<flags (2 hex)>`.
const SHARED_TRACEPARENT: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

/// Plugin-side carrier mirrors the `pub(crate)` helper in
/// `agentflow-cli/src/executor/plugin.rs`. The production helper is
/// `pub(crate)` and not reachable from integration tests; copy the
/// 4-line wrapper here so the test stays honest. If the production
/// helper ever gains additional env injection, update this mirror
/// (per the same convention the existing `plugin_traceparent_tests.rs`
/// follows).
fn plugin_inject(cmd: &mut TokioCommand) {
  if let Some(value) = agentflow_tracing::context::current_traceparent() {
    cmd.env(agentflow_tracing::context::TRACEPARENT_ENV, value);
  }
}

/// Spawn `sh -c 'echo tp=${TRACEPARENT-}'` so we can read the env var
/// the production preparer would have set on the subprocess.
async fn capture_plugin_subprocess_traceparent(cmd: &mut TokioCommand) -> String {
  cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
  let mut child = cmd.spawn().expect("spawn sh");
  let mut stdout = child.stdout.take().unwrap();
  let mut buf = String::new();
  stdout.read_to_string(&mut buf).await.unwrap();
  let _ = child.wait().await.unwrap();
  buf.trim().to_string()
}

/// LLM-side carrier: bridge `agentflow_tracing::context` (the
/// agentflow-wide active traceparent) into the LLM-specific
/// `LlmTraceContext` task-local + write the HTTP header. Production
/// LLM clients install both contexts; this helper mirrors that bridge
/// so the test can exercise the LLM HTTP carrier from a single
/// agentflow scope.
async fn llm_inject_into_headers(headers: &mut HeaderMap) {
  let Some(active) = agentflow_tracing::context::current_traceparent() else {
    return;
  };
  let Some(ctx) = LlmTraceContext::from_traceparent(&active) else {
    return;
  };
  // Use `inject_context_into_headers` (the explicit-context variant)
  // so the test doesn't have to install the LLM scope — the cross-
  // hop contract is about the agentflow-wide value, not about the
  // LLM's local task-local.
  llm_trace::inject_context_into_headers(&ctx, headers);
}

#[tokio::test]
async fn one_agentflow_scope_propagates_into_all_four_carriers() {
  // Fire all 4 carriers inside a single agentflow_tracing scope.
  // After the scope returns, each captured carrier MUST carry the
  // same traceparent value verbatim — that's the cross-hop
  // continuity contract.
  let (mcp_observed, grpc_observed, plugin_observed, llm_observed) =
    agentflow_tracing::context::scope(SHARED_TRACEPARENT.to_string(), async {
      // ── MCP hop ────────────────────────────────────────────────
      let mut mcp_req = JsonRpcRequest::new(
        RequestId::Number(1),
        "tools/call",
        Some(json!({ "name": "search", "arguments": { "q": "foo" } })),
      );
      let injected = inject_traceparent_into_request(&mut mcp_req);
      assert!(injected, "MCP inject must succeed inside an active scope");
      let mcp_observed = extract_traceparent_from_request(&mcp_req);

      // ── Worker gRPC hop ────────────────────────────────────────
      // inject is generic over `T` so we use `()` here — the body
      // type is irrelevant for metadata-only injection. Production
      // call sites use `pb::ClaimTaskRequest` etc., covered by the
      // per-hop unit tests.
      let mut grpc_req: Request<()> = Request::new(());
      inject_traceparent_into_grpc_request(&mut grpc_req);
      let grpc_observed = extract_traceparent_from_grpc_request(&grpc_req);

      // ── Plugin subprocess hop ──────────────────────────────────
      // `sh -c 'echo tp=${TRACEPARENT-}'` prints whatever the env
      // var was set to (empty if unset). The plugin preparer mirror
      // above writes the env var from the active scope.
      let mut plugin_cmd = TokioCommand::new("sh");
      plugin_cmd.args(["-c", "echo tp=${TRACEPARENT-}"]);
      plugin_inject(&mut plugin_cmd);
      let plugin_observed = capture_plugin_subprocess_traceparent(&mut plugin_cmd).await;

      // ── LLM HTTP header hop ────────────────────────────────────
      let mut llm_headers = HeaderMap::new();
      llm_inject_into_headers(&mut llm_headers).await;
      let llm_observed = llm_headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

      (mcp_observed, grpc_observed, plugin_observed, llm_observed)
    })
    .await;

  // ── Single-value cross-hop assertion ─────────────────────────
  // Every carrier surfaces the same agentflow-wide traceparent
  // byte-for-byte. The plugin path prefixes with "tp=" because the
  // shell echo wraps the value; strip it before comparing.
  let expected = SHARED_TRACEPARENT.to_string();
  assert_eq!(
    mcp_observed.as_deref(),
    Some(expected.as_str()),
    "MCP carrier diverged"
  );
  assert_eq!(
    grpc_observed.as_deref(),
    Some(expected.as_str()),
    "Worker gRPC carrier diverged"
  );
  assert_eq!(
    plugin_observed,
    format!("tp={expected}"),
    "plugin subprocess env var diverged"
  );
  assert_eq!(
    llm_observed.as_deref(),
    Some(expected.as_str()),
    "LLM HTTP header diverged"
  );
}

#[tokio::test]
async fn outside_any_scope_no_carrier_emits_traceparent() {
  // Inverse of the contract: when there is no agentflow-wide scope
  // active, every carrier must omit its traceparent field. Receivers
  // rely on field absence to distinguish "no upstream trace" from
  // "upstream trace exists but is empty / malformed".

  // ── MCP ─────────────────────────────────────────────────────
  let mut mcp_req = JsonRpcRequest::new(
    RequestId::Number(1),
    "tools/call",
    Some(json!({ "name": "noop" })),
  );
  let injected = inject_traceparent_into_request(&mut mcp_req);
  assert!(
    !injected,
    "MCP injector must be a no-op outside any agentflow scope"
  );
  assert!(extract_traceparent_from_request(&mcp_req).is_none());

  // ── Worker gRPC ─────────────────────────────────────────────
  let mut grpc_req: Request<()> = Request::new(());
  inject_traceparent_into_grpc_request(&mut grpc_req);
  assert!(
    extract_traceparent_from_grpc_request(&grpc_req).is_none(),
    "gRPC metadata must omit traceparent outside scope"
  );

  // ── Plugin subprocess ──────────────────────────────────────
  let mut plugin_cmd = TokioCommand::new("sh");
  plugin_cmd.args(["-c", "echo tp=${TRACEPARENT-}"]);
  plugin_inject(&mut plugin_cmd);
  let plugin_observed = capture_plugin_subprocess_traceparent(&mut plugin_cmd).await;
  // No scope ⇒ env var is unset ⇒ shell expansion produces an
  // empty value, leaving just the "tp=" literal.
  assert_eq!(plugin_observed, "tp=");

  // ── LLM HTTP ───────────────────────────────────────────────
  let mut headers = HeaderMap::new();
  llm_inject_into_headers(&mut headers).await;
  assert!(
    headers.get("traceparent").is_none(),
    "LLM header must omit traceparent outside scope"
  );
}

#[tokio::test]
async fn nested_scope_shadows_outer_value_across_all_four_carriers() {
  // Same as the single-scope test but with a nested inner scope.
  // The inner value MUST be the one every carrier picks up, not
  // the outer. Each per-hop test already locks this down in
  // isolation; this assertion is the cross-hop equivalent — proves
  // they all agree on which scope is "active" at injection time.
  let outer = "00-0af7651916cd43dd8448eb211c80319c-aaaaaaaaaaaaaaaa-01".to_string();
  let inner = "00-0af7651916cd43dd8448eb211c80319c-bbbbbbbbbbbbbbbb-01".to_string();

  let (mcp_observed, grpc_observed, plugin_observed, llm_observed) =
    agentflow_tracing::context::scope(outer.clone(), async {
      agentflow_tracing::context::scope(inner.clone(), async {
        let mut mcp_req = JsonRpcRequest::new(RequestId::Number(1), "ping", None);
        inject_traceparent_into_request(&mut mcp_req);
        let mcp_observed = extract_traceparent_from_request(&mcp_req);

        let mut grpc_req: Request<()> = Request::new(());
        inject_traceparent_into_grpc_request(&mut grpc_req);
        let grpc_observed = extract_traceparent_from_grpc_request(&grpc_req);

        let mut plugin_cmd = TokioCommand::new("sh");
        plugin_cmd.args(["-c", "echo tp=${TRACEPARENT-}"]);
        plugin_inject(&mut plugin_cmd);
        let plugin_observed = capture_plugin_subprocess_traceparent(&mut plugin_cmd).await;

        let mut headers = HeaderMap::new();
        llm_inject_into_headers(&mut headers).await;
        let llm_observed = headers
          .get("traceparent")
          .and_then(|v| v.to_str().ok())
          .map(str::to_owned);

        (mcp_observed, grpc_observed, plugin_observed, llm_observed)
      })
      .await
    })
    .await;

  assert_eq!(mcp_observed.as_deref(), Some(inner.as_str()));
  assert_eq!(grpc_observed.as_deref(), Some(inner.as_str()));
  assert_eq!(plugin_observed, format!("tp={inner}"));
  assert_eq!(llm_observed.as_deref(), Some(inner.as_str()));
}
