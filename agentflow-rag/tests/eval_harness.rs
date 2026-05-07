//! End-to-end integration test for the RAG eval harness.
//!
//! Loads the bundled `agentflow_mini` dataset, runs the BM25 retriever, and
//! asserts the smoke-level expectations: dataset validates, BM25 retrieves
//! something useful, baseline comparison stays self-consistent.

use agentflow_rag::eval::{Bm25Eval, Dataset, EvalConfig, compare, evaluate};
use std::path::PathBuf;

fn dataset_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("examples")
    .join("datasets")
    .join("agentflow_mini")
}

#[test]
fn loads_and_validates_demo_dataset() {
  let dataset = Dataset::load_from_dir(dataset_dir()).expect("dataset must load");
  assert_eq!(dataset.manifest.name.as_deref(), Some("agentflow_mini"));
  assert!(dataset.corpus.len() >= 12);
  assert!(dataset.queries.len() >= 8);
  assert!(dataset.judgments.len() >= 8);
  // Validation succeeded inside load_from_dir; calling it again is a no-op
  // but documents the contract.
  dataset.validate().expect("dataset validates");
}

#[test]
fn bm25_eval_on_demo_dataset_retrieves_relevant_docs() {
  let dataset = Dataset::load_from_dir(dataset_dir()).expect("dataset must load");
  let retriever = Bm25Eval::from_dataset(&dataset);
  let config = EvalConfig {
    k_values: vec![1, 3, 5],
    label: "bm25-default".into(),
  };
  let report = evaluate(&retriever, &dataset, &config).expect("eval runs");

  // Sanity bounds: BM25 over a tiny on-topic corpus should clear modest thresholds.
  let recall_5 = report
    .per_k
    .iter()
    .find(|r| r.k == 5)
    .expect("Recall@5 row");
  assert!(
    recall_5.recall >= 0.7,
    "Recall@5 = {:.3} fell below 0.7",
    recall_5.recall
  );
  assert!(report.mrr >= 0.5, "MRR = {:.3} fell below 0.5", report.mrr);
  assert_eq!(report.queries_with_relevant, dataset.queries.len());
}

#[test]
fn baseline_comparison_self_compare_is_inconclusive() {
  let dataset = Dataset::load_from_dir(dataset_dir()).expect("dataset must load");
  let retriever = Bm25Eval::from_dataset(&dataset);
  let config = EvalConfig {
    k_values: vec![5],
    label: "self".into(),
  };
  let report = evaluate(&retriever, &dataset, &config).expect("eval runs");

  let cmp = compare(&report, &report);
  // Comparing a report against itself: every per-query RR is identical,
  // so paired sign test must report 0 wins / 0 losses → inconclusive.
  assert_eq!(cmp.paired_wins, 0);
  assert_eq!(cmp.paired_losses, 0);
  assert!(cmp.paired_ties > 0);
  match cmp.verdict {
    agentflow_rag::eval::Verdict::Inconclusive => {}
    other => panic!("expected Inconclusive, got {:?}", other),
  }
}

#[test]
fn tuned_bm25_compares_against_default() {
  let dataset = Dataset::load_from_dir(dataset_dir()).expect("dataset must load");
  let baseline = Bm25Eval::from_dataset(&dataset);
  let candidate = Bm25Eval::with_params(&dataset, 1.8, 0.6);
  let config = EvalConfig {
    k_values: vec![3, 5],
    label: "default".into(),
  };
  let baseline_report = evaluate(&baseline, &dataset, &config).expect("baseline");
  let candidate_config = EvalConfig {
    k_values: vec![3, 5],
    label: "tuned".into(),
  };
  let candidate_report = evaluate(&candidate, &dataset, &candidate_config).expect("candidate");

  let cmp = compare(&baseline_report, &candidate_report);
  // Tuned BM25 may shift a couple of queries either way on a 12-query corpus.
  // We only assert the comparison structure is well-formed and labels carry through.
  assert!(cmp.deltas.iter().any(|d| d.metric == "MRR"));
  assert_eq!(cmp.baseline_label, "bm25 [default]");
  assert_eq!(cmp.candidate_label, "bm25 [tuned]");
}
