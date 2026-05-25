//! Integration test: a known [`LlmTraceContext`] flows from the LLM client
//! through the provider into the outbound HTTP request as a `traceparent`
//! header. The test pins down the W3C wire format the rest of the OTel
//! pipeline depends on.
//!
//! We use a hand-rolled tokio TCP listener instead of an HTTP-mock crate
//! so the test stays independent of mockito version churn and lets us
//! inspect the raw request bytes directly.
//!
//! The reqwest client is built with `.no_proxy()` so the request reaches
//! our localhost listener instead of being routed through any system
//! proxy (a common dev-machine setup that would otherwise fail).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use agentflow_llm::providers::{LLMProvider, OpenAIProvider, ProviderRequest};
use agentflow_llm::trace_context::{LlmTraceContext, scope};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const TRACE_ID: &str = "0af7651916cd43dd8448eb211c80319c";
const SPAN_ID: &str = "b7ad6b7169203331";

const RESPONSE_BODY: &str = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;

/// Spawn a one-shot TCP listener that accepts a single HTTP/1.1 request,
/// captures its head + body, and replies with a canned 200 OK + JSON body.
///
/// Returns `(base_url, captured_request_handle)`.
async fn spawn_capturing_server() -> (String, Arc<Mutex<Option<String>>>) {
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
    // Drain headers + body — reqwest surfaces transport errors if the
    // server closes its socket before fully reading the request body.
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

    let response = format!(
      "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
      RESPONSE_BODY.len(),
      RESPONSE_BODY,
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
  });

  // Give the server task a chance to reach `accept()` before the client
  // tries to connect.
  tokio::time::sleep(Duration::from_millis(50)).await;

  (format!("http://{addr}"), captured)
}

/// Build a `reqwest::Client` that bypasses any system proxy so 127.0.0.1
/// requests actually reach the test listener.
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
    thinking: None,
  }
}

fn header_line(captured: &str, header_name: &str) -> Option<String> {
  let lower_target = header_name.to_ascii_lowercase();
  for line in captured.split("\r\n") {
    if let Some((name, value)) = line.split_once(':')
      && name.trim().to_ascii_lowercase() == lower_target
    {
      return Some(value.trim().to_string());
    }
  }
  None
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_emits_traceparent_when_context_active() {
  let (base_url, captured) = spawn_capturing_server().await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let request = provider_request("gpt-4o-mini");
  let ctx = LlmTraceContext::new(TRACE_ID, SPAN_ID).expect("valid context");

  let response = scope(ctx.clone(), async move { provider.execute(&request).await })
    .await
    .expect("provider response");
  assert!(response.content.to_string().contains("ok"));

  let captured = captured
    .lock()
    .await
    .clone()
    .expect("server captured request");
  let traceparent = header_line(&captured, "traceparent")
    .expect("traceparent header was missing from outbound request");
  assert_eq!(
    traceparent,
    ctx.to_traceparent(),
    "traceparent value did not match the active context",
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openai_omits_traceparent_when_no_context_active() {
  let (base_url, captured) = spawn_capturing_server().await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let request = provider_request("gpt-4o-mini");

  // No `scope(...)` wrapping → task-local is empty → no header injected.
  provider.execute(&request).await.expect("provider response");

  let captured = captured
    .lock()
    .await
    .clone()
    .expect("server captured request");
  assert!(
    header_line(&captured, "traceparent").is_none(),
    "traceparent header leaked through with no active context: {captured}",
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_scope_uses_inner_context_for_outbound_call() {
  let outer = LlmTraceContext::random();
  let inner = LlmTraceContext::new(TRACE_ID, SPAN_ID).expect("valid context");
  let expected_traceparent = inner.to_traceparent();

  let (base_url, captured) = spawn_capturing_server().await;
  let provider =
    OpenAIProvider::with_client(no_proxy_client(), "test-key", Some(base_url)).expect("provider");
  let request = provider_request("gpt-4o-mini");

  scope(outer, async {
    scope(inner, async {
      provider.execute(&request).await.expect("provider response");
    })
    .await;
  })
  .await;

  let captured = captured
    .lock()
    .await
    .clone()
    .expect("server captured request");
  assert_eq!(
    header_line(&captured, "traceparent").as_deref(),
    Some(expected_traceparent.as_str()),
    "expected the inner scope's traceparent to win over the outer scope",
  );
}
