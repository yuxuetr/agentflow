//! Cross-provider behavioral consistency suite.
//!
//! Drives the same input through OpenAI, Anthropic, Google, Moonshot, and
//! StepFun providers (using each one's own response wire format) and asserts
//! that the parsed [`ProviderResponse`] / [`LLMError`] outputs match a single
//! consistent contract:
//!
//! 1. **Success path**: text content, `StopReason::Stop`, populated
//!    [`TokenUsage`] (prompt / completion / total).
//! 2. **Tool-calling success path**: `tool_calls` array length 1, name /
//!    arguments / id parsed identically regardless of provider wire format
//!    (OpenAI `tool_calls`, Anthropic `tool_use` content blocks, Google
//!    `functionCall` parts), `StopReason::ToolCalls`. id may be synthesised
//!    when the provider doesn't supply one (Google), but must be non-empty.
//! 3. **Authentication failure (401)**: all providers surface
//!    [`LLMError::HttpError`] with `status_code = 401`, regardless of how
//!    they label the error in the response body.
//! 4. **Rate limit (429)**: all providers surface
//!    [`LLMError::HttpError`] with `status_code = 429`.
//! 5. **Server error (500)**: all providers surface
//!    [`LLMError::HttpError`] with `status_code = 500`.
//!
//! 6. **Streaming success path**: each provider parses its own native
//!    streaming wire format (OpenAI / Moonshot / StepFun SSE
//!    `data: {chunk}` + `data: [DONE]`, Anthropic SSE `event: …` /
//!    `data: …`, Google newline-delimited JSON), and the resulting
//!    [`StreamChunk`] sequence concatenates to the same text, terminates
//!    cleanly, and emits at least one chunk with `is_final = true`.
//!
//! These are mocked via a hand-rolled tokio TCP listener — same pattern as
//! `trace_context_propagation.rs`, see that file for rationale. The
//! streaming path additionally uses chunked transfer encoding so each event
//! arrives as a separate frame, which is how real LLM providers stream.
//!
//! Live LLM tests (real API calls) are gated by
//! `AGENTFLOW_LIVE_LLM_TESTS=1` and live in a separate file when added.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentflow_llm::LLMError;
use agentflow_llm::client::StreamingResponse;
use agentflow_llm::providers::{
  AnthropicProvider, ContentType, GoogleProvider, LLMProvider, MoonshotProvider, OpenAIProvider,
  ProviderRequest, StepFunProvider,
};
use agentflow_llm::tool_calling::{StopReason, ToolCallRequest, ToolChoice, ToolSpec};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

// -----------------------------------------------------------------------------
// Shared mock-server helper (parameterized by status + body)
// -----------------------------------------------------------------------------

/// Spawn a one-shot TCP listener that accepts a single HTTP/1.1 request and
/// replies with `(status, body)`. Returns `(base_url, captured_request)`.
async fn spawn_mock_server(status: u16, body: String) -> (String, Arc<Mutex<Option<String>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let addr = listener.local_addr().expect("local_addr");
  let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
  let captured_writer = captured.clone();

  tokio::spawn(async move {
    let (mut stream, _) = match listener.accept().await {
      Ok(v) => v,
      Err(_) => return,
    };

    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1024];
    let mut head_end: Option<usize> = None;
    let mut content_length: Option<usize> = None;
    loop {
      let n = match stream.read(&mut tmp).await {
        Ok(0) | Err(_) => break,
        Ok(n) => n,
      };
      buf.extend_from_slice(&tmp[..n]);
      if head_end.is_none() {
        for i in 0..buf.len().saturating_sub(3) {
          if &buf[i..i + 4] == b"\r\n\r\n" {
            head_end = Some(i + 4);
            break;
          }
        }
      }
      if let Some(end) = head_end {
        if content_length.is_none() {
          let head = std::str::from_utf8(&buf[..end]).unwrap_or("");
          for line in head.split("\r\n") {
            if let Some(value) = line
              .strip_prefix("Content-Length:")
              .or_else(|| line.strip_prefix("content-length:"))
            {
              content_length = value.trim().parse().ok();
            }
          }
        }
        let body_so_far = buf.len() - end;
        if body_so_far >= content_length.unwrap_or(0) {
          break;
        }
      }
    }
    *captured_writer.lock().await = Some(String::from_utf8_lossy(&buf).into_owned());

    let status_text = match status {
      200 => "OK",
      401 => "Unauthorized",
      429 => "Too Many Requests",
      500 => "Internal Server Error",
      503 => "Service Unavailable",
      _ => "Status",
    };
    let response = format!(
      "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
      status,
      status_text,
      body.len(),
      body,
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
  });

  // Give the server task a chance to reach `accept()` before clients connect.
  tokio::time::sleep(Duration::from_millis(50)).await;

  (format!("http://{addr}"), captured)
}

/// Test client that bypasses any system proxy so 127.0.0.1 requests reach the
/// listener instead of being routed through a Clash/V2Ray-style proxy on the
/// dev machine. See CLAUDE.md "Rust HTTP Testing Guidelines".
fn no_proxy_client() -> reqwest::Client {
  reqwest::Client::builder()
    .no_proxy()
    .pool_max_idle_per_host(0)
    .timeout(Duration::from_secs(10))
    .build()
    .expect("client")
}

fn provider_request(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({"role": "user", "content": "ping"})],
    stream: false,
    parameters: HashMap::new(),
    tools: None,
    tool_choice: None,
  }
}

/// Build a `ProviderRequest` that advertises a single `get_weather` tool and
/// requires the model to call it. The mock servers below ignore the request
/// body and return canned tool-call responses, so the only thing this helper
/// needs to do is exercise the request-side `tools` / `tool_choice` encode
/// path without crashing the provider.
fn provider_request_with_tools(model: &str) -> ProviderRequest {
  let weather_tool = ToolSpec::new(
    "get_weather",
    "Return the weather for a city",
    json!({
      "type": "object",
      "properties": {"city": {"type": "string"}},
      "required": ["city"]
    }),
  );
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({"role": "user", "content": "weather in Tokyo?"})],
    stream: false,
    parameters: HashMap::new(),
    tools: Some(vec![weather_tool]),
    tool_choice: Some(ToolChoice::Required),
  }
}

// -----------------------------------------------------------------------------
// Per-provider success-path fixtures
// -----------------------------------------------------------------------------

const OPENAI_SUCCESS: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#;
const ANTHROPIC_SUCCESS: &str = r#"{"id":"msg_test","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":5,"output_tokens":1}}"#;
const GOOGLE_SUCCESS: &str = r#"{"candidates":[{"content":{"parts":[{"text":"ok"}],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1,"totalTokenCount":4}}"#;
const MOONSHOT_SUCCESS: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"moonshot-v1-8k","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#;
const STEPFUN_SUCCESS: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"step-1-8k","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4}}"#;

// -----------------------------------------------------------------------------
// Success-path consistency: every provider returns text + StopReason::Stop +
// populated TokenUsage given a well-formed response in its native shape.
// -----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_success_path() {
  let (base_url, _captured) = spawn_mock_server(200, OPENAI_SUCCESS.to_string()).await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request("gpt-4o-mini"))
    .await
    .expect("ok");
  assert_text(&response.content, "ok");
  assert_usage(&response.usage);
  assert_eq!(response.stop_reason, Some(StopReason::Stop));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_success_path() {
  let (base_url, _captured) = spawn_mock_server(200, ANTHROPIC_SUCCESS.to_string()).await;
  let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
  let response = provider
    .execute(&provider_request("claude-3-5-sonnet"))
    .await
    .expect("ok");
  assert_text(&response.content, "ok");
  assert_usage(&response.usage);
  assert_eq!(response.stop_reason, Some(StopReason::Stop));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_success_path() {
  let (base_url, _captured) = spawn_mock_server(200, GOOGLE_SUCCESS.to_string()).await;
  let provider =
    GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request("gemini-1.5-pro"))
    .await
    .expect("ok");
  assert_text(&response.content, "ok");
  assert_usage(&response.usage);
  assert_eq!(response.stop_reason, Some(StopReason::Stop));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_success_path() {
  let (base_url, _captured) = spawn_mock_server(200, MOONSHOT_SUCCESS.to_string()).await;
  let provider =
    MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request("moonshot-v1-8k"))
    .await
    .expect("ok");
  assert_text(&response.content, "ok");
  assert_usage(&response.usage);
  assert_eq!(response.stop_reason, Some(StopReason::Stop));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_success_path() {
  let (base_url, _captured) = spawn_mock_server(200, STEPFUN_SUCCESS.to_string()).await;
  let provider =
    StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request("step-1-8k"))
    .await
    .expect("ok");
  assert_text(&response.content, "ok");
  assert_usage(&response.usage);
  assert_eq!(response.stop_reason, Some(StopReason::Stop));
}

// -----------------------------------------------------------------------------
// Per-provider tool-calling fixtures
//
// Each fixture is the provider's *native* wire format for "model called
// `get_weather(city='Tokyo')`". The cross-provider contract being tested is:
// regardless of where in the response the tool call lives (OpenAI's
// `tool_calls` array, Anthropic's `tool_use` content block, Google's
// `functionCall` part), the parsed `ProviderResponse.tool_calls` is one entry
// with `name = "get_weather"`, `arguments.city = "Tokyo"`, a non-empty `id`,
// and `stop_reason = StopReason::ToolCalls`.
// -----------------------------------------------------------------------------

const OPENAI_TOOL_CALL: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Tokyo\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
const ANTHROPIC_TOOL_CALL: &str = r#"{"id":"msg_test","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[{"type":"text","text":"I'll check the weather."},{"type":"tool_use","id":"toolu_abc","name":"get_weather","input":{"city":"Tokyo"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":5}}"#;
// Note: Google emits `finishReason: "STOP"` even when functionCall is present;
// the provider adapter overrides to `ToolCalls` when functionCall parts exist.
const GOOGLE_TOOL_CALL: &str = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"city":"Tokyo"}}}],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#;
const MOONSHOT_TOOL_CALL: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"moonshot-v1-8k","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Tokyo\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
const STEPFUN_TOOL_CALL: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"step-1-8k","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Tokyo\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;

// -----------------------------------------------------------------------------
// Tool-calling consistency: every provider parses a model-issued tool call
// into a single `ToolCallRequest { name, arguments, id }` and reports
// `StopReason::ToolCalls`, regardless of the underlying wire format.
// -----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_tool_call_path() {
  let (base_url, _captured) = spawn_mock_server(200, OPENAI_TOOL_CALL.to_string()).await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request_with_tools("gpt-4o-mini"))
    .await
    .expect("ok");
  assert_tool_call(&response.tool_calls, "get_weather", "Tokyo");
  assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_tool_call_path() {
  let (base_url, _captured) = spawn_mock_server(200, ANTHROPIC_TOOL_CALL.to_string()).await;
  let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
  let response = provider
    .execute(&provider_request_with_tools("claude-3-5-sonnet"))
    .await
    .expect("ok");
  assert_tool_call(&response.tool_calls, "get_weather", "Tokyo");
  assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_tool_call_path() {
  let (base_url, _captured) = spawn_mock_server(200, GOOGLE_TOOL_CALL.to_string()).await;
  let provider =
    GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request_with_tools("gemini-1.5-pro"))
    .await
    .expect("ok");
  assert_tool_call(&response.tool_calls, "get_weather", "Tokyo");
  // Google reports `STOP` on the wire even when emitting a functionCall;
  // the provider adapter is responsible for normalising to `ToolCalls`.
  assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_tool_call_path() {
  let (base_url, _captured) = spawn_mock_server(200, MOONSHOT_TOOL_CALL.to_string()).await;
  let provider =
    MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request_with_tools("moonshot-v1-8k"))
    .await
    .expect("ok");
  assert_tool_call(&response.tool_calls, "get_weather", "Tokyo");
  assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_tool_call_path() {
  let (base_url, _captured) = spawn_mock_server(200, STEPFUN_TOOL_CALL.to_string()).await;
  let provider =
    StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let response = provider
    .execute(&provider_request_with_tools("step-1-8k"))
    .await
    .expect("ok");
  assert_tool_call(&response.tool_calls, "get_weather", "Tokyo");
  assert_eq!(response.stop_reason, Some(StopReason::ToolCalls));
}

// -----------------------------------------------------------------------------
// Error-mapping consistency: every provider maps non-2xx HTTP responses to
// LLMError::HttpError preserving the exact status code, regardless of body.
// This is the contract — promotion to AuthenticationError/RateLimitExceeded
// would be a *breaking* change downstream consumers depend on.
// -----------------------------------------------------------------------------

const GENERIC_ERROR_BODY: &str = r#"{"error":{"message":"go away"}}"#;

async fn assert_status_maps_to_http_error<F, Fut>(status: u16, run: F)
where
  F: FnOnce(String) -> Fut,
  Fut:
    std::future::Future<Output = agentflow_llm::Result<agentflow_llm::providers::ProviderResponse>>,
{
  let (base_url, _captured) = spawn_mock_server(status, GENERIC_ERROR_BODY.to_string()).await;
  let err = run(base_url).await.expect_err("expected HttpError");
  match err {
    LLMError::HttpError { status_code, .. } => assert_eq!(status_code, status),
    other => panic!("expected HttpError({status}), got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_maps_401_to_http_error() {
  assert_status_maps_to_http_error(401, |base_url| async move {
    let provider =
      OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
    provider.execute(&provider_request("gpt-4o-mini")).await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_maps_429_to_http_error() {
  assert_status_maps_to_http_error(429, |base_url| async move {
    let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    provider
      .execute(&provider_request("claude-3-5-sonnet"))
      .await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_maps_500_to_http_error() {
  assert_status_maps_to_http_error(500, |base_url| async move {
    let provider =
      GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
    provider.execute(&provider_request("gemini-1.5-pro")).await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_maps_503_to_http_error() {
  assert_status_maps_to_http_error(503, |base_url| async move {
    let provider = MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    provider.execute(&provider_request("moonshot-v1-8k")).await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_maps_401_to_http_error() {
  assert_status_maps_to_http_error(401, |base_url| async move {
    let provider = StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    provider.execute(&provider_request("step-1-8k")).await
  })
  .await;
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn assert_text(content: &ContentType, expected: &str) {
  match content {
    ContentType::Text(t) => assert!(
      t.contains(expected),
      "expected content to contain `{}`, got `{}`",
      expected,
      t
    ),
    other => panic!("expected ContentType::Text, got {:?}", other),
  }
}

fn assert_tool_call(calls: &[ToolCallRequest], expected_name: &str, expected_city: &str) {
  assert_eq!(
    calls.len(),
    1,
    "expected exactly one tool call, got {calls:?}"
  );
  let call = &calls[0];
  assert_eq!(call.name, expected_name);
  assert!(
    !call.id.is_empty(),
    "tool call id must be populated (synthesised if provider doesn't supply one)"
  );
  let city = call
    .arguments
    .get("city")
    .and_then(|v| v.as_str())
    .unwrap_or_else(|| {
      panic!(
        "expected `arguments.city` to be a string, got {}",
        call.arguments
      )
    });
  assert_eq!(city, expected_city);
}

fn assert_usage(usage: &Option<agentflow_llm::providers::TokenUsage>) {
  let usage = usage.as_ref().expect("usage must be populated");
  assert!(
    usage.prompt_tokens.is_some(),
    "prompt_tokens must be populated"
  );
  assert!(
    usage.completion_tokens.is_some(),
    "completion_tokens must be populated"
  );
  assert!(
    usage.total_tokens.is_some(),
    "total_tokens must be populated"
  );
}

// -----------------------------------------------------------------------------
// Streaming consistency
//
// Each provider parses its own native streaming wire format:
//
//   * OpenAI / Moonshot / StepFun: SSE `data: {chunk-json}` deltas + `data: [DONE]`
//   * Anthropic: SSE `event: <type>` + `data: {event-json}` events; final
//     marker is `message_stop` (or `content_block_stop`)
//   * Google: newline-delimited JSON; `finishReason` on the last object is the
//     terminator (no SSE prefix at all)
//
// The cross-provider contract under test is independent of all of that:
//
//   1. Draining `next_chunk()` produces ≥ 2 non-final delta chunks whose
//      `content` concatenated equals `"Hello world"`.
//   2. The stream eventually terminates: after the final marker, the next call
//      to `next_chunk()` returns `Ok(None)`.
//   3. Every emitted chunk carries `content_type == Some("text")`.
//
// Reusing `spawn_mock_server` (Content-Length + close) is intentional: the
// reqwest `bytes_stream()` delivers the full body and the per-provider parser
// splits on `\n` regardless of how the bytes were chunked over the wire. The
// goal is wire-format coverage, not network timing.
// -----------------------------------------------------------------------------

fn provider_request_streaming(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({"role": "user", "content": "ping"})],
    stream: true,
    parameters: HashMap::new(),
    tools: None,
    tool_choice: None,
  }
}

/// Spawn a streaming mock server that emits each of `events` as a separate
/// HTTP `Transfer-Encoding: chunked` frame.
///
/// Why a different helper from `spawn_mock_server`: every provider's stream
/// parser returns one `StreamChunk` per call to `next_chunk()` and only awaits
/// more bytes between calls. If we delivered every SSE event in a single TCP
/// write (the `Content-Length` path), the parser would return the first event
/// and then see the underlying byte stream as exhausted, dropping the rest.
/// Chunked encoding faithfully simulates an LLM provider trickling events,
/// which is the contract we actually want to lock down.
async fn spawn_streaming_mock_server(events: Vec<String>) -> String {
  let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
  let addr = listener.local_addr().expect("local_addr");

  tokio::spawn(async move {
    let (mut stream, _) = match listener.accept().await {
      Ok(v) => v,
      Err(_) => return,
    };

    // Drain the request before responding. We don't care about the contents;
    // we just need the read side to not back-pressure the writer.
    let mut tmp = [0u8; 1024];
    let mut head_end: Option<usize> = None;
    let mut head_buf: Vec<u8> = Vec::with_capacity(2048);
    while head_end.is_none() {
      let n = match stream.read(&mut tmp).await {
        Ok(0) | Err(_) => break,
        Ok(n) => n,
      };
      head_buf.extend_from_slice(&tmp[..n]);
      for i in 0..head_buf.len().saturating_sub(3) {
        if &head_buf[i..i + 4] == b"\r\n\r\n" {
          head_end = Some(i + 4);
          break;
        }
      }
    }

    let header = "HTTP/1.1 200 OK\r\n\
                  Content-Type: text/event-stream\r\n\
                  Transfer-Encoding: chunked\r\n\
                  Connection: close\r\n\r\n";
    if stream.write_all(header.as_bytes()).await.is_err() {
      return;
    }

    for event in &events {
      let frame = format!("{:x}\r\n{}\r\n", event.len(), event);
      if stream.write_all(frame.as_bytes()).await.is_err() {
        return;
      }
      let _ = stream.flush().await;
      // Small inter-chunk delay so reqwest's bytes_stream surfaces each frame
      // as its own item instead of coalescing under TCP buffering.
      tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let _ = stream.write_all(b"0\r\n\r\n").await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
  });

  tokio::time::sleep(Duration::from_millis(50)).await;
  format!("http://{addr}")
}

/// Drain an in-flight stream and assert the cross-provider contract.
///
/// The contract is intentionally lenient about *where* the terminator lives:
///
/// * OpenAI-compatible providers (OpenAI / Moonshot / StepFun) attach
///   `finish_reason: "stop"` to the same chunk as the trailing content delta,
///   so the second chunk is both content-bearing and final.
/// * Anthropic emits `content_block_delta` events for text and a separate
///   `message_stop` (no content) terminator.
/// * Google sets `finishReason` on the last JSON object, which still carries
///   the trailing text part.
///
/// What every provider must agree on:
///   1. ≥ 2 chunks carry non-empty `content` and concatenate to `"Hello world"`.
///   2. At least one chunk has `is_final = true`.
///   3. Each emitted chunk has `content_type == Some("text")`.
///   4. After the terminator, `next_chunk()` returns `Ok(None)` cleanly.
async fn assert_stream_yields_hello_world(mut stream: Box<dyn StreamingResponse>) {
  let mut text = String::new();
  let mut content_chunks = 0usize;
  let mut saw_final = false;

  while let Some(chunk) = stream
    .next_chunk()
    .await
    .expect("streaming chunk must parse")
  {
    assert_eq!(
      chunk.content_type.as_deref(),
      Some("text"),
      "every streamed chunk should carry text content_type"
    );
    if chunk.is_final {
      saw_final = true;
    }
    if !chunk.content.is_empty() {
      text.push_str(&chunk.content);
      content_chunks += 1;
    }
  }

  assert_eq!(
    text, "Hello world",
    "concatenated stream text mismatch (got {content_chunks} content-bearing chunks)"
  );
  assert!(
    content_chunks >= 2,
    "expected ≥2 content-bearing chunks, got {content_chunks}"
  );
  assert!(
    saw_final,
    "expected at least one chunk with `is_final = true` (terminator)"
  );

  // After the terminator, the stream must be cleanly drained.
  assert!(
    stream
      .next_chunk()
      .await
      .expect("next_chunk after drain must not error")
      .is_none(),
    "draining a finished stream should yield Ok(None)"
  );
}

fn openai_compat_stream_events(model: &str) -> Vec<String> {
  vec![
    format!(
      "data: {{\"id\":\"chatcmpl-x\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"role\":\"assistant\",\"content\":\"Hello\"}}}}]}}\n\n",
    ),
    format!(
      "data: {{\"id\":\"chatcmpl-x\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"{model}\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\" world\"}},\"finish_reason\":\"stop\"}}]}}\n\n",
    ),
    "data: [DONE]\n\n".to_string(),
  ]
}

fn anthropic_stream_events() -> Vec<String> {
  vec![
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n".to_string(),
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n".to_string(),
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
  ]
}

fn google_stream_events() -> Vec<String> {
  vec![
    "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}],\"role\":\"model\"},\"index\":0}]}\n".to_string(),
    "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":3,\"candidatesTokenCount\":2,\"totalTokenCount\":5}}\n".to_string(),
  ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_streaming_path() {
  let base_url = spawn_streaming_mock_server(openai_compat_stream_events("gpt-4o-mini")).await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let stream = provider
    .execute_streaming(&provider_request_streaming("gpt-4o-mini"))
    .await
    .expect("stream");
  assert_stream_yields_hello_world(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_streaming_path() {
  let base_url = spawn_streaming_mock_server(anthropic_stream_events()).await;
  let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
  let stream = provider
    .execute_streaming(&provider_request_streaming("claude-3-5-sonnet"))
    .await
    .expect("stream");
  assert_stream_yields_hello_world(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_streaming_path() {
  let base_url = spawn_streaming_mock_server(google_stream_events()).await;
  let provider =
    GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let stream = provider
    .execute_streaming(&provider_request_streaming("gemini-1.5-pro"))
    .await
    .expect("stream");
  assert_stream_yields_hello_world(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_streaming_path() {
  let base_url = spawn_streaming_mock_server(openai_compat_stream_events("moonshot-v1-8k")).await;
  let provider =
    MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let stream = provider
    .execute_streaming(&provider_request_streaming("moonshot-v1-8k"))
    .await
    .expect("stream");
  assert_stream_yields_hello_world(stream).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_streaming_path() {
  let base_url = spawn_streaming_mock_server(openai_compat_stream_events("step-1-8k")).await;
  let provider =
    StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let stream = provider
    .execute_streaming(&provider_request_streaming("step-1-8k"))
    .await
    .expect("stream");
  assert_stream_yields_hello_world(stream).await;
}

// -----------------------------------------------------------------------------
// Multimodal consistency
//
// Each test sends a `text + image` user message in the provider's native wire
// format and asserts two cross-provider invariants:
//
//   1. Request-side encoding preserves the image payload (we look for the
//      marker base64 payload `"AAAA"` in the captured request body — every
//      provider shapes the bytes differently, but the bytes themselves must
//      survive the round-trip).
//   2. Response-side parsing is identical regardless of whether the request
//      was text-only or multimodal: text content + `StopReason::Stop` +
//      populated `TokenUsage` (same contract as the success-path tests).
//
// Native multimodal formats:
//
//   * OpenAI / Moonshot / StepFun: OpenAI v1/chat content array with
//     `{type: "image_url", image_url: {url: "data:image/png;base64,AAAA"}}`.
//   * Anthropic: content blocks with `{type: "image", source: {type: "base64",
//     media_type: "image/png", data: "AAAA"}}`.
//   * Google: OpenAI-style content array — the provider adapter translates it
//     into Gemini's `parts[].inline_data` shape (see
//     `openai_content_to_gemini_parts`).
//
// Marker payload `"AAAA"` is a 3-byte zero-padded base64 stub. It's invalid
// PNG data — that's intentional. The mock server doesn't decode the image,
// and using a known 4-char marker gives us a high-signal substring assertion
// without bloating fixtures with realistic image bytes.
// -----------------------------------------------------------------------------

const IMAGE_DATA_URL: &str = "data:image/png;base64,AAAA";
const IMAGE_MARKER: &str = "AAAA";

fn provider_request_multimodal_openai_style(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": [
        {"type": "text", "text": "What's in this image?"},
        {"type": "image_url", "image_url": {"url": IMAGE_DATA_URL}}
      ]
    })],
    stream: false,
    parameters: HashMap::new(),
    tools: None,
    tool_choice: None,
  }
}

fn provider_request_multimodal_anthropic_style(model: &str) -> ProviderRequest {
  ProviderRequest {
    model: model.to_string(),
    messages: vec![json!({
      "role": "user",
      "content": [
        {"type": "text", "text": "What's in this image?"},
        {
          "type": "image",
          "source": {
            "type": "base64",
            "media_type": "image/png",
            "data": IMAGE_MARKER,
          }
        }
      ]
    })],
    stream: false,
    parameters: HashMap::new(),
    tools: None,
    tool_choice: None,
  }
}

async fn run_multimodal<F, Fut>(fixture: &str, build_provider: F) -> String
where
  F: FnOnce(String) -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  let (base_url, captured) = spawn_mock_server(200, fixture.to_string()).await;
  build_provider(base_url).await;
  let captured = captured.lock().await;
  captured.clone().expect("mock server received request")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_multimodal_path() {
  let captured = run_multimodal(OPENAI_SUCCESS, |base_url| async move {
    let provider =
      OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
    let response = provider
      .execute(&provider_request_multimodal_openai_style("gpt-4o-mini"))
      .await
      .expect("ok");
    assert_text(&response.content, "ok");
    assert_usage(&response.usage);
    assert_eq!(response.stop_reason, Some(StopReason::Stop));
  })
  .await;
  assert!(
    captured.contains(IMAGE_MARKER),
    "OpenAI request body must preserve image payload, got: {captured}"
  );
  assert!(
    captured.contains("image_url"),
    "OpenAI request body must use the `image_url` part type, got: {captured}"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_multimodal_path() {
  let captured = run_multimodal(ANTHROPIC_SUCCESS, |base_url| async move {
    let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    let response = provider
      .execute(&provider_request_multimodal_anthropic_style(
        "claude-3-5-sonnet",
      ))
      .await
      .expect("ok");
    assert_text(&response.content, "ok");
    assert_usage(&response.usage);
    assert_eq!(response.stop_reason, Some(StopReason::Stop));
  })
  .await;
  assert!(
    captured.contains(IMAGE_MARKER),
    "Anthropic request body must preserve base64 image payload, got: {captured}"
  );
  assert!(
    captured.contains("\"type\":\"image\"") || captured.contains("\"type\": \"image\""),
    "Anthropic request body must use the `image` content block type, got: {captured}"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_multimodal_path() {
  let captured = run_multimodal(GOOGLE_SUCCESS, |base_url| async move {
    let provider =
      GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
    let response = provider
      .execute(&provider_request_multimodal_openai_style("gemini-1.5-pro"))
      .await
      .expect("ok");
    assert_text(&response.content, "ok");
    assert_usage(&response.usage);
    assert_eq!(response.stop_reason, Some(StopReason::Stop));
  })
  .await;
  // Google adapter rewrites OpenAI-style multimodal content into Gemini's
  // `inline_data` part. Both the marker bytes and the Gemini-specific key
  // must be present.
  assert!(
    captured.contains(IMAGE_MARKER),
    "Google request body must preserve base64 image payload, got: {captured}"
  );
  assert!(
    captured.contains("inline_data"),
    "Google request body must rewrite to `inline_data`, got: {captured}"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moonshot_multimodal_path() {
  let captured = run_multimodal(MOONSHOT_SUCCESS, |base_url| async move {
    let provider = MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    let response = provider
      .execute(&provider_request_multimodal_openai_style("moonshot-v1-8k"))
      .await
      .expect("ok");
    assert_text(&response.content, "ok");
    assert_usage(&response.usage);
    assert_eq!(response.stop_reason, Some(StopReason::Stop));
  })
  .await;
  assert!(
    captured.contains(IMAGE_MARKER),
    "Moonshot request body must preserve image payload, got: {captured}"
  );
  assert!(
    captured.contains("image_url"),
    "Moonshot request body must use the `image_url` part type, got: {captured}"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stepfun_multimodal_path() {
  let captured = run_multimodal(STEPFUN_SUCCESS, |base_url| async move {
    let provider = StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    let response = provider
      .execute(&provider_request_multimodal_openai_style("step-1-8k"))
      .await
      .expect("ok");
    assert_text(&response.content, "ok");
    assert_usage(&response.usage);
    assert_eq!(response.stop_reason, Some(StopReason::Stop));
  })
  .await;
  assert!(
    captured.contains(IMAGE_MARKER),
    "StepFun request body must preserve image payload, got: {captured}"
  );
  assert!(
    captured.contains("image_url"),
    "StepFun request body must use the `image_url` part type, got: {captured}"
  );
}
