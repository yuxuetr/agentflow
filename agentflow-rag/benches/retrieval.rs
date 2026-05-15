//! Criterion micro-benchmarks for the BM25 retriever.
//!
//! These benches are purely keyword-based: no embedding network calls,
//! no qdrant. The shape mirrors what `agentflow rag eval ci_offline`
//! exercises so a regression here usually shows up in the eval gate too.
//!
//! Run:
//!
//! ```sh
//! cargo bench -p agentflow-rag --bench retrieval
//! ```

use std::time::Duration;

use agentflow_rag::retrieval::bm25::BM25Retriever;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

/// Build a deterministic corpus of `n` synthetic docs.
///
/// We mix a fixed vocabulary across docs so realistic IDF distributions
/// emerge — every term appears in some but not all docs. The seed is
/// the doc index, which keeps results reproducible across runs.
fn build_corpus(n: usize) -> BM25Retriever {
  const VOCAB: &[&str] = &[
    "agent",
    "workflow",
    "retrieval",
    "context",
    "scheduler",
    "language",
    "model",
    "vector",
    "embedding",
    "query",
    "ranking",
    "pipeline",
    "skill",
    "trace",
    "metric",
    "session",
  ];
  let mut retriever = BM25Retriever::new();
  for i in 0..n {
    let mut tokens = Vec::with_capacity(64);
    // 64-token doc; rotate the vocab plus the index so docs differ.
    for j in 0..64 {
      let v = VOCAB[(i + j) % VOCAB.len()];
      tokens.push(v.to_string());
    }
    let content = tokens.join(" ");
    retriever.add_document(format!("doc_{i}"), content);
  }
  retriever
}

fn bench_bm25_search(c: &mut Criterion) {
  let mut group = c.benchmark_group("bm25_search");
  group.measurement_time(Duration::from_secs(8));
  for &size in &[1_000_usize, 10_000] {
    let retriever = build_corpus(size);
    group.throughput(Throughput::Elements(1));
    group.bench_with_input(BenchmarkId::new("top_10", size), &size, |b, _| {
      b.iter(|| retriever.search("agent workflow retrieval", 10));
    });
    group.bench_with_input(BenchmarkId::new("top_100", size), &size, |b, _| {
      b.iter(|| retriever.search("agent workflow retrieval", 100));
    });
  }
  group.finish();
}

fn bench_bm25_index(c: &mut Criterion) {
  let mut group = c.benchmark_group("bm25_index");
  group.measurement_time(Duration::from_secs(8));
  group.sample_size(20);
  for &size in &[1_000_usize, 10_000] {
    group.throughput(Throughput::Elements(size as u64));
    group.bench_with_input(BenchmarkId::new("build_corpus", size), &size, |b, &n| {
      b.iter(|| build_corpus(n));
    });
  }
  group.finish();
}

criterion_group!(benches, bench_bm25_search, bench_bm25_index);
criterion_main!(benches);
