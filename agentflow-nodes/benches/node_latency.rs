//! Per-node latency benchmarks (P10.2.1).
//!
//! The existing `agentflow-core/benches/scheduler.rs` covers the DAG
//! scheduler's hot path but leaves individual node implementations
//! un-benched. This file fills that gap for the built-in nodes whose
//! `execute()` is **pure compute** — no LLM API call, no HTTP, no MCP
//! server spawn. The other node types depend on real services and
//! belong in a separate (probably nightly) benchmark surface that's
//! deliberately not part of the per-PR bench gate.
//!
//! Coverage:
//!   * `template/render/{small,medium,large}` — Tera rendering. The
//!     TODO explicitly calls out template-render regressions; this
//!     is the headline benchmark.
//!   * `conditional/{exists,equals,greater_than}` — pattern-match
//!     dispatch + HashMap lookup. Cheap, but catches an O(N) sneak
//!     into the eval path.
//!   * `file/{read,write}/{1k,64k}` — tokio fs round-trip through a
//!     `tempfile::TempDir`. Local-FS-dependent, but the relative
//!     ratio across runs is what bench-gate watches.
//!
//! Bench-gate baseline schema:
//!   `agentflow-nodes/node_latency`:
//!     `template/render/small`, `template/render/medium`, …
//!
//! Run locally:
//!   ```sh
//!   cargo bench -p agentflow-nodes --bench node_latency
//!   ```
//! CI uses the same flags as the other benches (`--warm-up-time 1
//! --measurement-time 1 --sample-size 10`) for fast feedback; a
//! refresh-baseline pass should use the defaults (3s measurement,
//! sample-size 20) for tighter intervals.

use agentflow_core::async_node::AsyncNode;
use agentflow_core::value::FlowValue;
use agentflow_nodes::nodes::conditional::{ConditionType, ConditionalNode};
use agentflow_nodes::nodes::file::FileNode;
use agentflow_nodes::nodes::template::TemplateNode;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

// ── helpers ─────────────────────────────────────────────────────────────────

/// Build a `HashMap<String, FlowValue>` from a fixed list of
/// `(name, value)` pairs without the per-call boilerplate at every
/// bench site.
fn inputs(pairs: &[(&str, Value)]) -> HashMap<String, FlowValue> {
  pairs
    .iter()
    .map(|(k, v)| (k.to_string(), FlowValue::Json(v.clone())))
    .collect()
}

/// Synthesise a template body of roughly `target_chars` characters
/// that exercises a non-trivial slice of the Tera renderer:
/// for-loop over a Vec context, if-condition, a filter chain, and
/// a final string concatenation. Smaller / larger sizes scale the
/// loop count linearly — the same template shape, just more
/// iterations.
fn synthetic_template(loop_iters: usize) -> String {
  let mut t = String::with_capacity(loop_iters * 32);
  t.push_str("Greetings, {{ name }}.\n");
  t.push_str("{% for item in items %}");
  t.push_str("  - {{ item.id }}: {{ item.label | upper }}");
  t.push_str("{% if item.enabled %} (active){% endif %}\n");
  t.push_str("{% endfor %}");
  t.push_str("Total active: {{ items | length }}.\n");
  // Pad with literal text to hit the target size — exercises the
  // raw-text branch of the parser too.
  let pad = loop_iters * 16;
  for _ in 0..pad {
    t.push('.');
  }
  t
}

/// Build the matching context for `synthetic_template`. `n` controls
/// how big the iterated `items` array is.
fn template_context(n: usize) -> HashMap<String, FlowValue> {
  let items: Vec<Value> = (0..n)
    .map(|i| {
      json!({
        "id": format!("id-{i}"),
        "label": format!("label-{i}"),
        "enabled": i % 2 == 0,
      })
    })
    .collect();
  inputs(&[("name", json!("benchmark")), ("items", Value::Array(items))])
}

// ── benches ─────────────────────────────────────────────────────────────────

fn bench_template_render(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  // Group is `template`; benches inside it are `render/<size>`. Final
  // criterion id is `template/render/<size>`, matching the bench-gate
  // baseline schema's `<group>/<variant>/<param>` convention.
  let mut group = c.benchmark_group("template");
  group.measurement_time(Duration::from_secs(8));

  // Three sizes mirror the scheduler bench's 10/100/1000 cadence
  // so the bench-gate report stays scannable. `small` exercises the
  // small-template fast path (parser dominates); `medium` is the
  // realistic workflow shape; `large` exercises the loop body.
  for (label, loop_iters) in [("small", 4_usize), ("medium", 32), ("large", 256)] {
    let template_src = synthetic_template(loop_iters);
    let node = TemplateNode::new("bench", &template_src);
    let ctx = template_context(loop_iters);
    group.throughput(Throughput::Elements(loop_iters as u64));
    group.bench_with_input(BenchmarkId::new("render", label), &loop_iters, |b, _| {
      b.to_async(&rt)
        .iter(|| async { node.execute(&ctx).await.expect("template render ok") });
    });
  }
  group.finish();
}

fn bench_conditional_evaluate(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  // Group is `conditional`; benches inside it are `evaluate/<variant>`.
  // Final criterion id is `conditional/evaluate/<variant>`.
  let mut group = c.benchmark_group("conditional");
  group.measurement_time(Duration::from_secs(8));

  // Three condition variants cover the three distinct branches of
  // `evaluate_condition`. Each one runs against a small input map
  // so the cost is pure dispatch + comparison, not HashMap growth.
  let ctx = inputs(&[
    ("user_present", json!(true)),
    ("name", json!("alice")),
    ("score", json!(42.0)),
  ]);

  let exists_node = ConditionalNode::new("ex", "user_present");
  group.bench_function(BenchmarkId::new("evaluate", "exists"), |b| {
    b.to_async(&rt)
      .iter(|| async { exists_node.execute(&ctx).await.expect("conditional ok") });
  });

  let equals_node = ConditionalNode::new("eq", "name")
    .with_condition_type(ConditionType::Equals("alice".to_string()));
  group.bench_function(BenchmarkId::new("evaluate", "equals"), |b| {
    b.to_async(&rt)
      .iter(|| async { equals_node.execute(&ctx).await.expect("conditional ok") });
  });

  let gt_node =
    ConditionalNode::new("gt", "score").with_condition_type(ConditionType::GreaterThan(10.0));
  group.bench_function(BenchmarkId::new("evaluate", "greater_than"), |b| {
    b.to_async(&rt)
      .iter(|| async { gt_node.execute(&ctx).await.expect("conditional ok") });
  });

  group.finish();
}

fn bench_file_read_write(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let mut group = c.benchmark_group("file");
  group.measurement_time(Duration::from_secs(8));

  // Two payload sizes: 1 KiB and 64 KiB. Smaller catches per-call
  // overhead; larger catches per-byte copy regressions. We do NOT
  // bench MiB-scale payloads — that's a perf-of-`tokio::fs`
  // measurement, not an AgentFlow node measurement.
  let dir = TempDir::new().expect("tempdir");
  let node = FileNode::default();

  for (label, byte_count) in [("1k", 1024_usize), ("64k", 64 * 1024)] {
    let payload: String = "x".repeat(byte_count);
    let write_path = dir
      .path()
      .join(format!("write_{label}.txt"))
      .to_string_lossy()
      .into_owned();
    let write_inputs = inputs(&[
      ("operation", json!("write")),
      ("path", json!(write_path.clone())),
      ("content", json!(payload.clone())),
    ]);

    group.throughput(Throughput::Bytes(byte_count as u64));
    group.bench_with_input(BenchmarkId::new("write", label), &byte_count, |b, _| {
      b.to_async(&rt)
        .iter(|| async { node.execute(&write_inputs).await.expect("file write ok") });
    });

    // Seed the read target once outside the loop. The bench then
    // measures the read-only path repeatedly; the file's contents
    // and on-disk metadata stay stable across iterations.
    let read_path = dir
      .path()
      .join(format!("read_{label}.txt"))
      .to_string_lossy()
      .into_owned();
    std::fs::write(&read_path, &payload).expect("seed read fixture");
    let read_inputs = inputs(&[("operation", json!("read")), ("path", json!(read_path))]);
    group.bench_with_input(BenchmarkId::new("read", label), &byte_count, |b, _| {
      b.to_async(&rt)
        .iter(|| async { node.execute(&read_inputs).await.expect("file read ok") });
    });
  }
  group.finish();
}

criterion_group!(
  benches,
  bench_template_render,
  bench_conditional_evaluate,
  bench_file_read_write
);
criterion_main!(benches);
