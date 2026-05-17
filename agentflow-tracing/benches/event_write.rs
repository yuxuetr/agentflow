//! Criterion micro-benchmarks for trace write throughput.
//!
//! The roadmap calls for "JSONL vs SQLite throughput" — the crate
//! currently ships a single backend (`FileTraceStorage`, one JSON
//! file per workflow) plus an optional postgres backend behind the
//! `postgres` feature. To keep the bench hermetic we measure:
//!
//! - **`serialize_only`**: in-memory `serde_json` cost (no IO).
//! - **`file_storage_save`**: full disk round-trip via the existing
//!   `FileTraceStorage::save_trace` path.
//! - **`jsonl_append`**: synthetic JSONL append loop, one line per
//!   node trace, so we can compare append-style throughput against
//!   the single-blob save above.
//!
//! When a real JSONL or SQLite backend lands, drop a sibling group
//! and the comparison stays meaningful.
//!
//! Run:
//!
//! ```sh
//! cargo bench -p agentflow-tracing --bench event_write
//! ```

use std::io::Write;
use std::time::Duration;

use agentflow_tracing::TraceStorage;
use agentflow_tracing::storage::file::FileTraceStorage;
use agentflow_tracing::types::{ExecutionTrace, NodeStatus, NodeTrace, TraceStatus};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::json;
use tempfile::TempDir;
use tokio::runtime::Runtime;

fn make_trace(node_count: usize) -> ExecutionTrace {
  let mut trace = ExecutionTrace::new(format!("wf_{node_count}"));
  trace.workflow_name = Some("bench".to_string());
  trace.status = TraceStatus::Completed;
  trace.completed_at = Some(chrono::Utc::now());
  for i in 0..node_count {
    let mut node = NodeTrace::new(format!("node_{i}"), "BenchNode".to_string());
    node.input = Some(json!({"index": i, "payload": "lorem ipsum dolor sit amet"}));
    node.output = Some(json!({"index": i, "result": "lorem ipsum dolor sit amet"}));
    node.status = NodeStatus::Completed;
    node.duration_ms = Some(1);
    trace.nodes.push(node);
  }
  trace
}

fn bench_serialize_only(c: &mut Criterion) {
  let mut group = c.benchmark_group("trace_serialize");
  group.measurement_time(Duration::from_secs(5));
  for &n in &[10_usize, 100, 1000] {
    let trace = make_trace(n);
    group.throughput(Throughput::Elements(n as u64));
    group.bench_with_input(BenchmarkId::new("nodes", n), &n, |b, _| {
      b.iter(|| serde_json::to_string(&trace).expect("serialize"));
    });
  }
  group.finish();
}

fn bench_file_storage(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let dir = TempDir::new().expect("tmpdir");
  let storage = FileTraceStorage::new(dir.path().to_path_buf()).expect("storage");
  let mut group = c.benchmark_group("file_storage_save");
  group.measurement_time(Duration::from_secs(6));
  for &n in &[10_usize, 100, 1000] {
    group.throughput(Throughput::Elements(n as u64));
    group.bench_with_input(BenchmarkId::new("nodes", n), &n, |b, &n| {
      let trace = make_trace(n);
      b.to_async(&rt).iter(|| async {
        storage.save_trace(&trace).await.expect("save");
      });
    });
  }
  group.finish();
}

fn bench_jsonl_append(c: &mut Criterion) {
  let mut group = c.benchmark_group("jsonl_append");
  group.measurement_time(Duration::from_secs(6));
  for &n in &[10_usize, 100, 1000] {
    let trace = make_trace(n);
    group.throughput(Throughput::Elements(n as u64));
    group.bench_with_input(BenchmarkId::new("nodes", n), &n, |b, _| {
      b.iter(|| {
        let dir = TempDir::new().expect("tmpdir");
        let path = dir.path().join("events.jsonl");
        let mut file = std::fs::File::create(&path).expect("create");
        for node in &trace.nodes {
          let line = serde_json::to_string(node).expect("serialize");
          file.write_all(line.as_bytes()).expect("write");
          file.write_all(b"\n").expect("write newline");
        }
        file.sync_data().expect("fsync");
      });
    });
  }
  group.finish();
}

criterion_group!(
  benches,
  bench_serialize_only,
  bench_file_storage,
  bench_jsonl_append
);
criterion_main!(benches);
