//! End-to-end retrieval evaluation harness.
//!
//! The eval module turns a [`Dataset`] (`corpus` + `queries` + `judgments`)
//! plus a [`Retriever`] into a quantitative [`EvalReport`] (Recall@K, MRR,
//! nDCG@K, latency).
//!
//! ## Design
//!
//! - **Retriever-agnostic**: the runner only needs `search(query, k) ->
//!   Vec<doc_id>`. BM25 is the default offline backend; vector / hybrid /
//!   external retrievers plug in by implementing [`Retriever`].
//! - **Dataset-first**: standard JSONL layout (`corpus.jsonl`,
//!   `queries.jsonl`, `qrels.jsonl`) plus optional `dataset.toml` manifest.
//! - **Baseline comparison**: [`compare`] takes two `EvalReport`s and emits
//!   per-metric deltas with a sign-test verdict.
//!
//! See [`docs/RAG_EVAL.md`](https://github.com/agentflow/agentflow/blob/main/docs/RAG_EVAL.md)
//! for the user-facing reference.

pub mod compare;
pub mod dataset;
pub mod metrics;
pub mod retrievers;
pub mod runner;

pub use compare::{
  ComparisonReport, MetricDelta, Verdict, compare, paired_sign_lower_tail_p_value,
};
pub use dataset::{CorpusDoc, Dataset, DatasetManifest, Judgment, Query, RelevanceScore};
pub use metrics::{LatencyAggregate, MetricKind, ndcg_at_k, recall_at_k, reciprocal_rank};
pub use retrievers::{Bm25Eval, DenseEval, HybridEval};
pub use runner::{EvalConfig, EvalReport, PerKMetrics, PerQueryRow, Retriever, evaluate};
