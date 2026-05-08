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
//! These are mocked via a hand-rolled tokio TCP listener — same pattern as
//! `trace_context_propagation.rs`, see that file for rationale.
//!
//! Live LLM tests (real API calls) are gated by
//! `AGENTFLOW_LIVE_LLM_TESTS=1` and live in a separate file when added.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentflow_llm::providers::{
  AnthropicProvider, ContentType, GoogleProvider, LLMProvider, MoonshotProvider, OpenAIProvider,
  ProviderRequest, StepFunProvider,
};
use agentflow_llm::tool_calling::{StopReason, ToolCallRequest, ToolChoice, ToolSpec};
use agentflow_llm::LLMError;
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
  let provider =
    AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
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
  let provider = MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
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
  let provider = StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
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
  let provider = MoonshotProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
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
  let provider = StepFunProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
    .expect("provider");
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
  Fut: std::future::Future<Output = agentflow_llm::Result<agentflow_llm::providers::ProviderResponse>>,
{
  let (base_url, _captured) =
    spawn_mock_server(status, GENERIC_ERROR_BODY.to_string()).await;
  let err = run(base_url).await.expect_err("expected HttpError");
  match err {
    LLMError::HttpError { status_code, .. } => assert_eq!(status_code, status),
    other => panic!("expected HttpError({status}), got {other:?}"),
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_maps_401_to_http_error() {
  assert_status_maps_to_http_error(401, |base_url| async move {
    let provider = OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    provider.execute(&provider_request("gpt-4o-mini")).await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anthropic_maps_429_to_http_error() {
  assert_status_maps_to_http_error(429, |base_url| async move {
    let provider = AnthropicProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
    provider.execute(&provider_request("claude-3-5-sonnet")).await
  })
  .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn google_maps_500_to_http_error() {
  assert_status_maps_to_http_error(500, |base_url| async move {
    let provider = GoogleProvider::with_client(no_proxy_client(), "test-key", Some(base_url))
      .expect("provider");
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
  assert_eq!(calls.len(), 1, "expected exactly one tool call, got {calls:?}");
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
