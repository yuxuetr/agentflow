//! Local stdio MCP latency benchmarks.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p agentflow-mcp --test mcp_latency_benchmarks --target-dir /tmp/agentflow-target -- --nocapture
//! ```

use agentflow_mcp::client::ClientBuilder;
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};

#[tokio::test]
async fn benchmark_local_stdio_mcp_latency() {
  println!("\nLocal stdio MCP latency benchmarks");
  println!("{}", "=".repeat(80));

  let mut first_client = build_client().await;
  let first_connect = measure_once(|| async {
    first_client
      .connect()
      .await
      .expect("connect should succeed");
  })
  .await;
  let first_list = measure_once(|| async {
    let tools = first_client
      .list_tools()
      .await
      .expect("tools/list should succeed");
    assert_eq!(tools.len(), 2);
  })
  .await;
  first_client
    .disconnect()
    .await
    .expect("disconnect should succeed");

  let mut reused_client = build_client().await;
  reused_client
    .connect()
    .await
    .expect("connect should succeed");
  let mut reused_list = Vec::with_capacity(30);
  for _ in 0..30 {
    reused_list.push(
      measure_once(|| async {
        let tools = reused_client
          .list_tools()
          .await
          .expect("tools/list should succeed");
        assert_eq!(tools.len(), 2);
      })
      .await,
    );
  }
  let mut reused_call = Vec::with_capacity(50);
  for _ in 0..50 {
    reused_call.push(
      measure_once(|| async {
        let result = reused_client
          .call_tool("echo", json!({"text": "latency"}))
          .await
          .expect("tools/call should succeed");
        assert_eq!(result.first_text(), Some("mcp-basic: latency"));
      })
      .await,
    );
  }
  reused_client
    .disconnect()
    .await
    .expect("disconnect should succeed");

  let reconnect = measure_series(10, || async {
    let mut client = build_client().await;
    client.connect().await.expect("connect should succeed");
    let tools = client
      .list_tools()
      .await
      .expect("tools/list should succeed");
    assert_eq!(tools.len(), 2);
    client
      .disconnect()
      .await
      .expect("disconnect should succeed");
  })
  .await;

  println!("  first connect: {:?}", first_connect);
  println!("  first tools/list: {:?}", first_list);
  print_stats("reused tools/list", &reused_list);
  print_stats("reused tools/call", &reused_call);
  print_stats("shutdown/reconnect/list", &reconnect);
}

async fn build_client() -> agentflow_mcp::client::MCPClient {
  ClientBuilder::new()
    .with_stdio(vec!["python3".to_string(), server_path()])
    .with_timeout(Duration::from_secs(5))
    .with_max_retries(0)
    .build()
    .await
    .expect("client build should succeed")
}

fn server_path() -> String {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("..")
    .join("agentflow-skills")
    .join("examples")
    .join("skills")
    .join("mcp-basic")
    .join("server.py")
    .to_string_lossy()
    .into_owned()
}

async fn measure_once<F, Fut>(f: F) -> Duration
where
  F: FnOnce() -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  let start = Instant::now();
  f().await;
  start.elapsed()
}

async fn measure_series<F, Fut>(iterations: usize, mut f: F) -> Vec<Duration>
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = ()>,
{
  let mut samples = Vec::with_capacity(iterations);
  for _ in 0..iterations {
    samples.push(measure_once(&mut f).await);
  }
  samples
}

fn print_stats(label: &str, samples: &[Duration]) {
  println!(
    "  {label}: p50 {:?}, p95 {:?}, avg {:?} ({} samples)",
    percentile(samples, 50),
    percentile(samples, 95),
    average(samples),
    samples.len()
  );
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
  let mut sorted = samples.to_vec();
  sorted.sort();
  let idx = ((sorted.len() - 1) * percentile) / 100;
  sorted[idx]
}

fn average(samples: &[Duration]) -> Duration {
  let total: Duration = samples.iter().copied().sum();
  total / samples.len() as u32
}
