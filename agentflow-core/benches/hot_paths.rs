//! Hot-path benchmarks for the post-N8 FlowValue + checkpoint round-trip
//! (P10.1.1).
//!
//! The existing `agentflow-core/benches/scheduler.rs` covers DAG mechanics
//! (topological sort, dependency-ready dispatch, fan-out). This file
//! covers the two hot paths that run *between* node executions.
//!
//! `flowvalue_decode/*` benches `serde_json::from_value::<FlowValue>`
//! over the tagged-enum wire shape (P0.2). Every node output that lands
//! in a checkpoint pays this cost on the next workflow load, so even a
//! few-microsecond regression compounds across realistic 100-node state
//! pools.
//!
//! `checkpoint_roundtrip/*` benches the full
//! `CheckpointManager::save_checkpoint` + `load_latest_checkpoint`
//! cycle. Captures JSON ser/deser cost, `to_string_pretty` formatting,
//! atomic temp-file rename, and the `checkpoint_latest.json` copy step
//! in one bench.
//!
//! Run locally:
//!   ```sh
//!   cargo bench -p agentflow-core --bench hot_paths
//!   ```
//!
//! The bench is wired into the existing `bench-gate` baseline at
//! `agentflow-core/hot_paths`. The default `bench-gate` 1.25× threshold
//! catches realistic regressions; the TODO's 1.10× note is operator-side
//! intuition for "review this PR more carefully" rather than a hard gate
//! (the bench output is noisy enough on most hosts that 1.10× would trip
//! on day-to-day variance).

use agentflow_core::checkpoint::{Checkpoint, CheckpointConfig, CheckpointManager, WorkflowStatus};
use agentflow_core::value::FlowValue;
use chrono::Utc;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

// ── helpers ─────────────────────────────────────────────────────────────────

/// Build a "tiny" JSON-variant FlowValue payload — five scalar fields. The
/// realistic shape for a `Plan` / `Observe` / short-text node output.
fn tiny_json_value() -> Value {
  json!({
    "type": "json",
    "value": { "ok": true, "score": 0.92, "name": "demo", "n": 7, "tag": "release" }
  })
}

/// Build a "medium" JSON-variant FlowValue payload — a 50-element array of
/// 4-field objects (~3 KiB on the wire). Common shape for a batch node
/// emitting a per-item result list.
fn medium_json_value() -> Value {
  let arr: Vec<Value> = (0..50)
    .map(|i| {
      json!({
        "id": format!("id-{i}"),
        "label": format!("label-{i}"),
        "score": (i as f64) / 50.0,
        "enabled": i % 2 == 0,
      })
    })
    .collect();
  json!({ "type": "json", "value": arr })
}

/// Build a "large" JSON-variant FlowValue payload — a nested object with a
/// 500-element array (~30 KiB). Stand-in for an LLM tool-result emitting a
/// rich JSON document.
fn large_json_value() -> Value {
  let arr: Vec<Value> = (0..500)
    .map(|i| {
      json!({
        "id": format!("entry-{i}"),
        "text": format!("Long text content for entry {i} repeated padding padding padding."),
        "weight": (i as f64) * 0.1,
        "metadata": { "source": "bench", "index": i, "valid": i % 3 != 0 },
      })
    })
    .collect();
  json!({
    "type": "json",
    "value": { "total": arr.len(), "entries": arr, "version": "1.0" }
  })
}

/// `FlowValue::File` wire shape (tagged).
fn file_value() -> Value {
  json!({
    "type": "file",
    "path": "/tmp/bench-fixture/output.bin",
    "mime_type": "application/octet-stream",
  })
}

/// `FlowValue::Url` wire shape (tagged).
fn url_value() -> Value {
  json!({
    "type": "url",
    "url": "https://example.test/bench/resource.json",
    "mime_type": "application/json",
  })
}

/// Build a state pool with `n` node entries, each carrying one
/// medium-JSON FlowValue. Mirrors the shape `Checkpoint::state` holds
/// after each node completion in a realistic 10–100-node workflow.
fn make_state_pool(n: usize) -> HashMap<String, Value> {
  let mut state = HashMap::with_capacity(n);
  for i in 0..n {
    let outputs = json!({
      "result": medium_json_value(),
    });
    state.insert(format!("node_{i}"), outputs);
  }
  state
}

/// Build a CheckpointManager rooted at the per-bench tempdir. Returns the
/// owning TempDir so the caller can keep it alive for the duration of the
/// bench iteration.
fn fresh_manager() -> (CheckpointManager, TempDir) {
  let dir = TempDir::new().expect("tempdir");
  let cfg = CheckpointConfig::default()
    .with_checkpoint_dir(PathBuf::from(dir.path()))
    .with_auto_cleanup(false);
  let manager = CheckpointManager::new(cfg).expect("checkpoint manager");
  (manager, dir)
}

// ── benches ─────────────────────────────────────────────────────────────────

fn bench_flowvalue_decode(c: &mut Criterion) {
  let mut group = c.benchmark_group("flowvalue_decode");
  group.measurement_time(Duration::from_secs(6));

  // BenchmarkId pair (function_name, parameter) produces criterion path
  // `<group>/<function_name>/<parameter>` on disk — the format
  // `bench-gate` reads. Avoid slash characters inside either component
  // because criterion sanitises them to `_` in the directory name (so
  // `from_parameter("json/tiny")` would write to `flowvalue_decode/
  // json_tiny/`, which the gate's exact-match lookup misses).
  for (variant, size, value) in [
    ("json", "tiny", tiny_json_value()),
    ("json", "medium", medium_json_value()),
    ("json", "large", large_json_value()),
    ("file", "metadata_only", file_value()),
    ("url", "metadata_only", url_value()),
  ] {
    // We bench against a pre-built `serde_json::Value` rather than a raw
    // string. The realistic hot path is `serde_json::from_value::<FlowValue>
    // (json_value.clone())` because the outer Checkpoint deserialization
    // hands the per-node `state` map to `decode_checkpoint_flow_value` as
    // already-parsed `serde_json::Value`s. Benching from a raw &str would
    // double-count the outer JSON parse that has nothing to do with the
    // FlowValue tagged decoder.
    group.bench_with_input(BenchmarkId::new(variant, size), &value, |b, value| {
      b.iter(|| {
        let _decoded: FlowValue =
          serde_json::from_value(value.clone()).expect("flow value decode ok");
      });
    });
  }
  group.finish();
}

fn bench_checkpoint_roundtrip(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let mut group = c.benchmark_group("checkpoint_roundtrip");
  // Disk I/O dominates the long tail; bump measurement_time so the
  // p50 stays stable enough for the bench-gate 1.25× threshold to
  // mean something.
  group.measurement_time(Duration::from_secs(10));

  for &node_count in &[10_usize, 100] {
    let state = make_state_pool(node_count);
    group.throughput(Throughput::Elements(node_count as u64));

    // `save` and `load` are benched as a single round-trip because they
    // share the same on-disk state and the checkpoint manager is a thin
    // wrapper around fs::write + fs::read. Splitting them would force
    // each iteration to seed-the-fixture, which would dominate the load
    // bench's wall-clock with the save it's trying to isolate.
    group.bench_with_input(
      BenchmarkId::new("save_and_load", node_count),
      &node_count,
      |b, _| {
        b.to_async(&rt).iter_batched(
          // Per-iteration setup: a fresh manager + tempdir + a cloned
          // state pool so each iteration owns its own working copy.
          // `iter_batched` re-invokes setup per iter, so cloning here
          // doesn't cross into the measured body; the timed work is
          // strictly the save + load round-trip.
          || {
            let (manager, dir) = fresh_manager();
            (manager, dir, state.clone())
          },
          |(manager, _dir, state)| async move {
            let wid = format!("bench-{}", uuid::Uuid::new_v4());
            manager
              .save_checkpoint_with_status(
                &wid,
                "last_node",
                &state,
                WorkflowStatus::Running,
              )
              .await
              .expect("save ok");
            manager
              .load_latest_checkpoint(&wid)
              .await
              .expect("load ok")
              .expect("checkpoint present")
          },
          criterion::BatchSize::SmallInput,
        );
      },
    );
  }

  // Decode-only bench: serialise a pre-built Checkpoint to JSON once,
  // then bench just the parse. This isolates the deserialiser from the
  // fs cost — same isolation the `flowvalue_decode` group provides for
  // the inner per-value path, but at the Checkpoint level.
  for &node_count in &[10_usize, 100] {
    let checkpoint = Checkpoint {
      workflow_id: "bench-decode".to_string(),
      last_completed_node: "last_node".to_string(),
      state: make_state_pool(node_count),
      created_at: Utc::now(),
      status: WorkflowStatus::Running,
      metadata: HashMap::new(),
    };
    let serialized =
      serde_json::to_string_pretty(&checkpoint).expect("serialize checkpoint");
    group.throughput(Throughput::Elements(node_count as u64));
    group.bench_with_input(
      BenchmarkId::new("decode", node_count),
      &node_count,
      |b, _| {
        b.iter(|| {
          let _decoded: Checkpoint =
            serde_json::from_str(&serialized).expect("decode ok");
        });
      },
    );
  }

  group.finish();
}

criterion_group!(benches, bench_flowvalue_decode, bench_checkpoint_roundtrip);
criterion_main!(benches);
