//! Criterion micro-benchmarks for provider-level call overhead.
//!
//! We use the mock provider so the wall-clock time is dominated by
//! serialization, allocation, and trait-object dispatch — i.e., the
//! pieces of the LLM hop that ship with every provider. Network and
//! token-budgeted prompts are intentionally out of scope here.
//!
//! Run:
//!
//! ```sh
//! cargo bench -p agentflow-llm --bench provider_hop
//! ```

use std::collections::HashMap;
use std::time::Duration;

use agentflow_llm::providers::mock::MockProvider;
use agentflow_llm::providers::{LLMProvider, ProviderRequest};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::json;
use tokio::runtime::Runtime;

fn make_request(message_count: usize) -> ProviderRequest {
  let messages: Vec<_> = (0..message_count)
    .map(|i| json!({"role": if i % 2 == 0 { "user" } else { "assistant" }, "content": format!("turn-{i} payload with a bit of filler to look realistic")}))
    .collect();
  ProviderRequest {
    model: "mock-model".to_string(),
    messages,
    stream: false,
    parameters: HashMap::new(),
    tools: None,
    tool_choice: None,
    thinking: None,
  }
}

fn bench_single_hop(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let provider = MockProvider::new("", None)
    .expect("mock provider")
    .with_response("ok");

  let mut group = c.benchmark_group("provider_hop");
  group.measurement_time(Duration::from_secs(6));
  for &turns in &[1_usize, 8, 32] {
    let request = make_request(turns);
    group.throughput(Throughput::Elements(1));
    group.bench_with_input(BenchmarkId::new("execute", turns), &turns, |b, _| {
      b.to_async(&rt)
        .iter(|| async { provider.execute(&request).await.expect("mock ok") });
    });
  }
  group.finish();
}

fn bench_streaming_hop(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let provider = MockProvider::new("", None)
    .expect("mock provider")
    .with_response("streamed token batch");

  let mut group = c.benchmark_group("provider_hop_streaming");
  group.measurement_time(Duration::from_secs(6));
  let request = make_request(4);
  group.throughput(Throughput::Elements(1));
  group.bench_function("execute_streaming_full_drain", |b| {
    b.to_async(&rt).iter(|| async {
      let mut stream = provider
        .execute_streaming(&request)
        .await
        .expect("mock stream");
      while let Some(_chunk) = stream.next_chunk().await.expect("chunk") {}
    });
  });
  group.finish();
}

criterion_group!(benches, bench_single_hop, bench_streaming_hop);
criterion_main!(benches);
