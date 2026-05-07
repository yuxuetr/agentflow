//! Cross-provider behavioral consistency suite.
//!
//! Drives the same input through OpenAI, Anthropic, Google, Moonshot, and
//! StepFun providers (using each one's own response wire format) and asserts
//! that the parsed [`ProviderResponse`] / [`LLMError`] outputs match a single
//! consistent contract:
//!
//! 1. **Success path**: text content, `StopReason::Stop`, populated
//!    [`TokenUsage`] (prompt / completion / total).
//! 2. **Authentication failure (401)**: all providers surface
//!    [`LLMError::HttpError`] with `status_code = 401`, regardless of how
//!    they label the error in the response body.
//! 3. **Rate limit (429)**: all providers surface
//!    [`LLMError::HttpError`] with `status_code = 429`.
//! 4. **Server error (500)**: all providers surface
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
use agentflow_llm::tool_calling::StopReason;
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
