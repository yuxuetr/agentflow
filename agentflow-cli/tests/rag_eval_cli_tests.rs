//! `agentflow rag eval` schema regression test (P4.1).
//!
//! Drives the bundled `agentflow-rag/eval_datasets/ci_offline/` fixture
//! through the CLI and asserts the JSON envelope keeps every field
//! downstream consumers (CI dashboards, baseline-comparison tooling)
//! rely on. The test is hermetic — BM25 over a 20-doc synthetic
//! corpus, no embedding model, no network, no DB.
//!
//! Only built when the `rag` feature is enabled (the rag CLI command
//! is gated behind it).

#![cfg(feature = "rag")]

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::TempDir;

fn fixture_path() -> String {
  format!(
    "{}/../agentflow-rag/eval_datasets/ci_offline",
    env!("CARGO_MANIFEST_DIR")
  )
}

#[test]
fn cli_rag_eval_ci_offline_json_envelope_carries_every_expected_field() {
  let work = TempDir::new().unwrap();
  let report_path = work.path().join("rag-report.json");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "rag",
      "eval",
      "--dataset",
      &fixture_path(),
      "--output",
      report_path.to_str().unwrap(),
    ])
    .assert()
    .success();

  let raw = fs::read_to_string(&report_path).unwrap();
  let report: Value = serde_json::from_str(&raw).unwrap();

  // ── Top-level envelope ─────────────────────────────────────────────
  let dataset = report
    .get("dataset")
    .expect("envelope must carry 'dataset' block");
  assert!(
    dataset.get("path").is_some(),
    "dataset.path must be present"
  );
  assert!(
    dataset.get("manifest").is_some(),
    "dataset.manifest must be present"
  );
  assert_eq!(
    dataset["corpus_size"].as_u64(),
    Some(20),
    "ci_offline corpus must contain 20 docs"
  );
  assert_eq!(
    dataset["queries"].as_u64(),
    Some(10),
    "ci_offline queries must contain 10 entries"
  );
  assert_eq!(
    dataset["judgments"].as_u64(),
    Some(10),
    "ci_offline qrels must contain 10 judgment rows"
  );

  // ── Baseline block: every metric field downstream consumers need ──
  let baseline = report
    .get("baseline")
    .expect("envelope must carry 'baseline' block");
  for key in [
    "retriever",
    "label",
    "mrr",
    "latency",
    "per_k",
    "num_queries",
  ] {
    assert!(
      baseline.get(key).is_some(),
      "baseline.{key} missing from envelope"
    );
  }
  assert_eq!(baseline["retriever"].as_str(), Some("bm25"));
  assert!(baseline["mrr"].as_f64().is_some(), "mrr must be a number");
  assert!(
    baseline["mrr"].as_f64().unwrap() > 0.0,
    "BM25 over a hand-tuned fixture should produce mrr > 0"
  );

  // ── Latency aggregates ────────────────────────────────────────────
  let latency = &baseline["latency"];
  for key in ["mean_ms", "p50_ms", "p95_ms"] {
    let v = latency
      .get(key)
      .unwrap_or_else(|| panic!("latency.{key} missing from envelope"))
      .as_f64();
    assert!(v.is_some(), "latency.{key} must be a number");
    assert!(
      v.unwrap() >= 0.0,
      "latency.{key} should be non-negative, got {:?}",
      v
    );
  }

  // ── per_k rows: Recall@K and nDCG@K for each K the CLI defaults to
  let per_k = baseline["per_k"]
    .as_array()
    .expect("per_k must be an array");
  let mut ks_seen: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
  for row in per_k {
    let k = row["k"]
      .as_u64()
      .expect("per_k row must carry an integer k");
    let recall = row["recall"]
      .as_f64()
      .expect("per_k row must carry a recall f64");
    let ndcg = row["ndcg"]
      .as_f64()
      .expect("per_k row must carry an ndcg f64");
    assert!(
      (0.0..=1.0).contains(&recall),
      "recall@{k} should be in [0, 1]; got {recall}"
    );
    assert!(
      (0.0..=1.0).contains(&ndcg),
      "ndcg@{k} should be in [0, 1]; got {ndcg}"
    );
    ks_seen.insert(k);
  }
  // The CLI's default k_values are [1, 3, 5, 10].
  let expected: std::collections::BTreeSet<u64> = [1u64, 3, 5, 10].into_iter().collect();
  assert_eq!(
    ks_seen, expected,
    "per_k must contain default K values; got {ks_seen:?}",
  );

  // ── Quality smoke gate (not a regression bound, a sanity gate) ────
  //
  // The fixture is deliberately easy for BM25: every query has at
  // least one highly-relevant doc whose vocabulary overlaps the
  // query. Recall@5 of 1.0 is the steady-state expectation. If it
  // ever drops below 0.8, either the dataset corrupted or the
  // retriever regressed — either way, CI should fail loudly.
  let recall_at_5 = per_k
    .iter()
    .find(|row| row["k"].as_u64() == Some(5))
    .and_then(|row| row["recall"].as_f64())
    .expect("recall@5 must be present");
  assert!(
    recall_at_5 >= 0.8,
    "recall@5 dropped below 0.8 sanity gate: {recall_at_5}",
  );
}

#[test]
fn cli_rag_eval_ci_offline_loads_without_output_flag() {
  // Smoke: the CLI's default text rendering also succeeds. Catches
  // breakage in render_table that --output would otherwise hide.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["rag", "eval", "--dataset", &fixture_path()])
    .assert()
    .success();
}

// ── P4.2 baseline-snapshot regression gate ─────────────────────────────

fn baseline_snapshot_path() -> String {
  format!(
    "{}/../agentflow-rag/eval_baselines/ci_offline/bm25.json",
    env!("CARGO_MANIFEST_DIR")
  )
}

#[test]
fn cli_rag_eval_compare_baseline_passes_against_checked_in_snapshot() {
  // The committed snapshot was produced by the same BM25 + dataset
  // combo, so a fresh run must match it. Exit 0 + regression gate PASS.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "rag",
      "eval",
      "--dataset",
      &fixture_path(),
      "--compare-baseline",
      &baseline_snapshot_path(),
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("Regression gate (PASS)"));
}

#[test]
fn cli_rag_eval_compare_baseline_json_carries_regression_block() {
  let work = TempDir::new().unwrap();
  let report_path = work.path().join("report.json");
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "rag",
      "eval",
      "--dataset",
      &fixture_path(),
      "--compare-baseline",
      &baseline_snapshot_path(),
      "--output",
      report_path.to_str().unwrap(),
    ])
    .assert()
    .success();
  let raw = fs::read_to_string(&report_path).unwrap();
  let report: Value = serde_json::from_str(&raw).unwrap();
  let regression = &report["regression"];
  assert_eq!(regression["regression_detected"], false);
  assert!(regression["threshold_recall_drop"].as_f64().is_some());
  assert!(regression["threshold_p_value"].as_f64().is_some());
  // Comparison block should now also carry paired_sign_p_value when
  // present (None on a tied-baseline-vs-self compare).
  let comparison = &report["comparison"];
  assert!(comparison.is_object(), "comparison block must be present");
}

// Note: the regression-detected end-to-end path (gate FAIL → exit 1)
// is covered by unit tests inside agentflow-cli/src/commands/rag/eval.rs
// rather than another integration test. Constructing a BM25-fooling
// dataset is fragile and couples the test to retriever internals; the
// unit tests pass a hand-built ComparisonReport into the same
// evaluate_regression function the CLI uses, so the gate logic itself
// is exercised without the retriever stamp.

#[test]
fn cli_rag_eval_rejects_mutually_exclusive_flags() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "rag",
      "eval",
      "--dataset",
      &fixture_path(),
      "--compare-to",
      "k1=1.5,b=0.6",
      "--compare-baseline",
      &baseline_snapshot_path(),
    ])
    .assert()
    .failure()
    .stderr(predicates::str::contains("mutually exclusive"));
}
