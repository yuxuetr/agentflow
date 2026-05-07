//! Built-in retriever adapters for the eval harness.
//!
//! These are deliberately offline / dependency-free so the harness can run in
//! CI without Qdrant or external embedding APIs. Production runs may plug a
//! custom [`Retriever`] in directly.

use super::dataset::Dataset;
use super::runner::Retriever;
use crate::error::Result;
use crate::retrieval::bm25::BM25Retriever;

/// BM25 retriever over the dataset corpus. Default for the CLI smoke test
/// because it has no external dependencies and is deterministic.
pub struct Bm25Eval {
  retriever: BM25Retriever,
}

impl Bm25Eval {
  pub fn from_dataset(dataset: &Dataset) -> Self {
    let mut retriever = BM25Retriever::new();
    for doc in &dataset.corpus {
      // Concatenate title + body when title is present — common BEIR
      // convention. Avoids losing keyword signal that lives in titles.
      let body = match &doc.title {
        Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
        _ => doc.text.clone(),
      };
      retriever.add_document(doc.id.clone(), body);
    }
    Self { retriever }
  }

  pub fn with_params(dataset: &Dataset, k1: f32, b: f32) -> Self {
    let mut retriever = BM25Retriever::with_params(k1, b);
    for doc in &dataset.corpus {
      let body = match &doc.title {
        Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
        _ => doc.text.clone(),
      };
      retriever.add_document(doc.id.clone(), body);
    }
    Self { retriever }
  }
}

impl Retriever for Bm25Eval {
  fn name(&self) -> &str {
    "bm25"
  }

  fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
    let results = self.retriever.search(query, k);
    Ok(results.into_iter().map(|r| r.id).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use crate::eval::runner::{EvalConfig, evaluate};
  use std::collections::HashMap;

  fn dataset_for_bm25() -> Dataset {
    let corpus = vec![
      CorpusDoc {
        id: "d1".into(),
        text: "machine learning improves predictions".into(),
        title: None,
      },
      CorpusDoc {
        id: "d2".into(),
        text: "the quick brown fox jumps over the lazy dog".into(),
        title: None,
      },
      CorpusDoc {
        id: "d3".into(),
        text: "deep learning neural networks".into(),
        title: None,
      },
    ];
    let queries = vec![Query {
      id: "q1".into(),
      text: "deep learning".into(),
      notes: None,
    }];
    let mut rel = HashMap::new();
    rel.insert("d3".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    Dataset::new(corpus, queries, judgments)
  }

  #[test]
  fn bm25_eval_finds_obvious_match() {
    let dataset = dataset_for_bm25();
    let retriever = Bm25Eval::from_dataset(&dataset);
    let config = EvalConfig {
      k_values: vec![1, 3],
      label: "bm25".into(),
    };
    let report = evaluate(&retriever, &dataset, &config).unwrap();
    let recall_1 = report.per_k.iter().find(|r| r.k == 1).unwrap();
    assert!((recall_1.recall - 1.0).abs() < 1e-9);
    assert_eq!(report.retriever, "bm25");
  }

  #[test]
  fn bm25_eval_with_custom_params() {
    let dataset = dataset_for_bm25();
    let retriever = Bm25Eval::with_params(&dataset, 1.5, 0.5);
    let result = retriever.search("deep learning", 1).unwrap();
    assert_eq!(result.first().map(|s| s.as_str()), Some("d3"));
  }

  #[test]
  fn bm25_eval_uses_title_when_present() {
    let dataset = Dataset::new(
      vec![CorpusDoc {
        id: "d1".into(),
        text: "body".into(),
        title: Some("rare-keyword".into()),
      }],
      vec![Query {
        id: "q1".into(),
        text: "rare-keyword".into(),
        notes: None,
      }],
      vec![],
    );
    let retriever = Bm25Eval::from_dataset(&dataset);
    let result = retriever.search("rare-keyword", 1).unwrap();
    assert_eq!(result.first().map(|s| s.as_str()), Some("d1"));
  }
}
