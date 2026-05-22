//! Chunk-then-evaluate helpers (P10.6.3).
//!
//! The base eval pipeline takes corpus documents as-is and feeds them
//! into a retriever one-doc-one-id. This module adds a thin
//! preprocessing layer: split each corpus doc into fixed-size chunks
//! with synthetic ids (`{orig_id}::chunk{idx}`), index those, then
//! map retrieved chunk ids back to source doc ids before qrels
//! scoring. The result is the same Recall@K / MRR / nDCG signal an
//! un-chunked run would produce, but with a meaningful per-chunk-size
//! latency dimension on the report (because the underlying retriever
//! index now sees N× more documents, where N varies with chunk_size).
//!
//! Use case is operator-facing: capture a baseline at each chunk
//! strategy you ship, then compare baselines to catch chunking-side
//! regressions that the un-chunked eval can't surface.

use super::dataset::{CorpusDoc, Dataset, Query};
use crate::chunking::FixedSizeChunker;
use crate::error::Result;
use crate::types::TextChunk;
use std::collections::HashMap;

/// A corpus that has been re-chunked for evaluation. Carries the
/// chunked corpus + the chunk-id → source-doc-id mapping needed to
/// remap retriever output back to the qrels keys.
#[derive(Debug, Clone)]
pub struct ChunkedDataset {
  /// Re-chunked corpus, each chunk a synthetic [`CorpusDoc`] with id
  /// `{orig_id}::chunk{idx}`.
  pub corpus: Vec<CorpusDoc>,
  /// Queries from the source dataset, copied through unchanged.
  pub queries: Vec<Query>,
  /// Judgments from the source dataset, copied through unchanged.
  /// Qrels still reference source doc ids — the runner remaps before
  /// scoring.
  pub judgments: Vec<super::dataset::Judgment>,
  /// `chunk_id -> source_doc_id`. Populated for every chunk in
  /// `corpus`. Lookups during retrieval evaluation use this to fold
  /// multiple chunks of the same doc into a single hit.
  pub chunk_to_doc: HashMap<String, String>,
  /// Fixed-size chunker config used to produce this dataset. Pinned
  /// for diagnostics + future fidelity.
  pub chunk_size: usize,
  pub overlap: usize,
}

impl ChunkedDataset {
  /// Re-export the chunked corpus as a `Dataset` so existing
  /// retrievers (`Bm25Eval::from_dataset`, `DenseEval::new`) can
  /// consume it without a new constructor. Judgments are passed
  /// through verbatim — the remap-then-score happens at retrieval
  /// time, not at dataset-build time.
  pub fn as_dataset(&self) -> Dataset {
    Dataset::new(
      self.corpus.clone(),
      self.queries.clone(),
      self.judgments.clone(),
    )
  }
}

/// Chunk every corpus doc in `dataset` with a fixed-size chunker.
///
/// `chunk_size` is the max chunk character count; `overlap` controls
/// how much each chunk overlaps with the next. Pass `overlap = 0`
/// for non-overlapping chunks, the canonical fast baseline.
///
/// The synthetic chunk ids follow the pattern `{orig_id}::chunk{idx}`
/// (zero-based). Operators reading per-query rows in a report can
/// recover the source doc by splitting on `::chunk` (or by reading
/// the `chunk_to_doc` map directly).
///
/// A doc that produces zero chunks (empty body + empty title) is
/// preserved as a single empty-body chunk so qrels still resolve;
/// this matches the un-chunked baseline's behaviour for
/// pathological inputs.
pub fn chunk_dataset(
  dataset: &Dataset,
  chunk_size: usize,
  overlap: usize,
) -> Result<ChunkedDataset> {
  if chunk_size == 0 {
    return Err(crate::error::RAGError::invalid_input(
      "chunk_size must be greater than 0",
    ));
  }
  if overlap >= chunk_size {
    return Err(crate::error::RAGError::invalid_input(format!(
      "overlap ({overlap}) must be strictly less than chunk_size ({chunk_size})"
    )));
  }
  let chunker = FixedSizeChunker::new(chunk_size, overlap);
  let mut corpus: Vec<CorpusDoc> = Vec::with_capacity(dataset.corpus.len());
  let mut chunk_to_doc: HashMap<String, String> = HashMap::with_capacity(dataset.corpus.len());

  for doc in &dataset.corpus {
    // BEIR convention: title + body when title is non-empty, same as
    // `Bm25Eval::from_dataset`. Chunking on the post-concat string
    // keeps the chunked-eval comparable to the un-chunked eval.
    let body = match &doc.title {
      Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
      _ => doc.text.clone(),
    };
    let chunks: Vec<TextChunk> = if body.trim().is_empty() {
      // Empty body → single empty chunk so the doc still indexes and
      // the chunk_to_doc map covers it. Otherwise the retriever
      // would never surface this doc and recall computations would
      // silently drop it.
      vec![TextChunk {
        content: String::new(),
        start_idx: 0,
        end_idx: 0,
        metadata: Default::default(),
        chunk_index: 0,
        total_chunks: 1,
      }]
    } else {
      // Errors from the chunker are not expected for the fixed-size
      // backend (it doesn't fail), but propagate them just in case.
      chunker_chunk(&chunker, &body)?
    };
    for chunk in chunks {
      let chunk_id = format!("{}::chunk{}", doc.id, chunk.chunk_index);
      chunk_to_doc.insert(chunk_id.clone(), doc.id.clone());
      corpus.push(CorpusDoc {
        id: chunk_id,
        text: chunk.content,
        // Title is already folded into the chunk body above; don't
        // re-prepend it in the synthetic doc or downstream
        // retrievers would double-count.
        title: None,
      });
    }
  }

  Ok(ChunkedDataset {
    corpus,
    queries: dataset.queries.clone(),
    judgments: dataset.judgments.clone(),
    chunk_to_doc,
    chunk_size,
    overlap,
  })
}

/// Remap a retriever's `Vec<chunk_id>` output to a `Vec<source_doc_id>`,
/// preserving rank order and deduplicating repeated source docs.
///
/// When the retriever returns multiple chunks of the same source doc
/// inside its top-K window, only the highest-ranked one contributes
/// to qrels scoring. Without this dedupe a 5-chunk doc could claim
/// every slot in `Recall@5` even though it's a single relevant
/// document — that would inflate recall and break the comparison
/// with the un-chunked baseline.
///
/// Chunks whose id isn't in `chunk_to_doc` (defensive: a stale
/// retriever index, a malformed chunk id) pass through unchanged
/// rather than being dropped. The runner then evaluates them
/// against qrels as a normal doc id — if qrels don't know the id,
/// they contribute 0 to recall, which is the desired safe default.
pub fn remap_chunks_to_doc_ids(
  retrieved: &[String],
  chunk_to_doc: &HashMap<String, String>,
) -> Vec<String> {
  let mut out: Vec<String> = Vec::with_capacity(retrieved.len());
  let mut seen: std::collections::HashSet<String> =
    std::collections::HashSet::with_capacity(retrieved.len());
  for raw in retrieved {
    let mapped = chunk_to_doc
      .get(raw)
      .cloned()
      .unwrap_or_else(|| raw.clone());
    if seen.insert(mapped.clone()) {
      out.push(mapped);
    }
  }
  out
}

/// Thin wrapper so the call sites stay readable. The chunker's own
/// `chunk()` API returns `crate::error::Result<Vec<TextChunk>>`; we
/// just propagate it.
fn chunker_chunk(chunker: &FixedSizeChunker, text: &str) -> Result<Vec<TextChunk>> {
  use crate::chunking::ChunkingStrategy;
  chunker.chunk(text)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use std::collections::HashMap as Map;

  fn make_dataset(docs: &[(&str, &str)]) -> Dataset {
    let corpus: Vec<CorpusDoc> = docs
      .iter()
      .map(|(id, text)| CorpusDoc {
        id: id.to_string(),
        text: text.to_string(),
        title: None,
      })
      .collect();
    Dataset::new(corpus, vec![], vec![])
  }

  #[test]
  fn chunk_dataset_rejects_zero_chunk_size() {
    let ds = make_dataset(&[("d1", "hello world")]);
    let err = chunk_dataset(&ds, 0, 0).unwrap_err();
    assert!(err.to_string().contains("chunk_size"));
  }

  #[test]
  fn chunk_dataset_rejects_overlap_ge_chunk_size() {
    let ds = make_dataset(&[("d1", "hello world")]);
    let err = chunk_dataset(&ds, 4, 4).unwrap_err();
    assert!(err.to_string().contains("overlap"));
    let err2 = chunk_dataset(&ds, 4, 5).unwrap_err();
    assert!(err2.to_string().contains("overlap"));
  }

  #[test]
  fn chunk_dataset_produces_synthetic_ids_and_mapping() {
    // A single doc with ~30 chars chunked at chunk_size=10 produces
    // 3 chunks with predictable ids. We don't assert the exact
    // number of chunks (chunker boundary heuristics may shift) but
    // the mapping invariants must hold.
    let ds = make_dataset(&[("d1", "abcdefghijklmnopqrstuvwxyz0123")]);
    let chunked = chunk_dataset(&ds, 10, 0).expect("chunk ok");
    assert!(chunked.corpus.len() >= 2, "expected >=2 chunks");
    for doc in &chunked.corpus {
      assert!(
        doc.id.starts_with("d1::chunk"),
        "synthetic id wrong: {}",
        doc.id
      );
      assert_eq!(
        chunked.chunk_to_doc.get(&doc.id).map(String::as_str),
        Some("d1")
      );
    }
  }

  #[test]
  fn chunk_dataset_preserves_queries_and_judgments_verbatim() {
    let corpus = vec![CorpusDoc {
      id: "d1".into(),
      text: "abcdefghij".into(),
      title: None,
    }];
    let queries = vec![Query {
      id: "q1".into(),
      text: "abc".into(),
      notes: None,
    }];
    let mut rel = Map::new();
    rel.insert("d1".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    let ds = Dataset::new(corpus, queries.clone(), judgments.clone());
    let chunked = chunk_dataset(&ds, 4, 0).expect("chunk ok");
    assert_eq!(chunked.queries.len(), queries.len());
    assert_eq!(chunked.queries[0].id, queries[0].id);
    assert_eq!(chunked.queries[0].text, queries[0].text);
    assert_eq!(chunked.judgments.len(), judgments.len());
    assert_eq!(chunked.chunk_size, 4);
    assert_eq!(chunked.overlap, 0);
  }

  #[test]
  fn chunk_dataset_concatenates_title_into_body_for_chunking() {
    // BEIR convention: title + "\n" + body. The chunker sees the
    // concatenated string. The synthetic doc carries an empty
    // title (the title text is already inside the chunk body), so
    // downstream retrievers don't double-prepend.
    let corpus = vec![CorpusDoc {
      id: "d1".into(),
      text: "body content".into(),
      title: Some("TITLE".into()),
    }];
    let ds = Dataset::new(corpus, vec![], vec![]);
    let chunked = chunk_dataset(&ds, 100, 0).expect("chunk ok");
    // With chunk_size=100, the title+body fits in one chunk.
    assert_eq!(chunked.corpus.len(), 1);
    assert_eq!(chunked.corpus[0].title, None);
    assert!(chunked.corpus[0].text.contains("TITLE"));
    assert!(chunked.corpus[0].text.contains("body content"));
  }

  #[test]
  fn chunk_dataset_preserves_empty_doc_as_single_empty_chunk() {
    // Pathological input: a corpus doc with no text + no title. We
    // index it as one empty chunk so qrels still resolve.
    let corpus = vec![CorpusDoc {
      id: "d-empty".into(),
      text: String::new(),
      title: None,
    }];
    let ds = Dataset::new(corpus, vec![], vec![]);
    let chunked = chunk_dataset(&ds, 64, 0).expect("chunk ok");
    assert_eq!(chunked.corpus.len(), 1);
    assert_eq!(chunked.corpus[0].id, "d-empty::chunk0");
    assert_eq!(chunked.corpus[0].text, "");
    assert_eq!(
      chunked
        .chunk_to_doc
        .get("d-empty::chunk0")
        .map(String::as_str),
      Some("d-empty")
    );
  }

  #[test]
  fn remap_dedupes_repeated_source_doc_preserving_rank() {
    // Retriever returned [d1::chunk0, d2::chunk1, d1::chunk2, d3::chunk0].
    // After remap + dedupe: [d1, d2, d3] — d1 only counted once,
    // ranked at its first occurrence.
    let mut map = HashMap::new();
    map.insert("d1::chunk0".to_string(), "d1".to_string());
    map.insert("d1::chunk2".to_string(), "d1".to_string());
    map.insert("d2::chunk1".to_string(), "d2".to_string());
    map.insert("d3::chunk0".to_string(), "d3".to_string());
    let raw = vec![
      "d1::chunk0".to_string(),
      "d2::chunk1".to_string(),
      "d1::chunk2".to_string(),
      "d3::chunk0".to_string(),
    ];
    let out = remap_chunks_to_doc_ids(&raw, &map);
    assert_eq!(
      out,
      vec!["d1".to_string(), "d2".to_string(), "d3".to_string()]
    );
  }

  #[test]
  fn chunked_bm25_eval_recovers_recall_via_remap_dedupe() {
    // End-to-end: build a corpus that chunks into multiple pieces,
    // run BM25 over the chunked corpus, verify that the qrels-based
    // recall at K=1 still credits the source doc (because the
    // remap step folds chunk hits back to source ids before
    // scoring). Without the remap step, the K-window would be
    // saturated with chunks of the same source doc and the source
    // doc id wouldn't appear at all — recall would silently drop.
    use crate::eval::retrievers::Bm25Eval;
    use crate::eval::runner::{EvalConfig, evaluate_with_remapping};

    let corpus = vec![
      CorpusDoc {
        id: "d1".into(),
        // Long enough to produce multiple ~30-char chunks.
        text: "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu".into(),
        title: None,
      },
      CorpusDoc {
        id: "d2".into(),
        text: "machine learning model training data pipeline".into(),
        title: None,
      },
    ];
    let queries = vec![Query {
      id: "q1".into(),
      text: "gamma delta".into(),
      notes: None,
    }];
    let mut rel = Map::new();
    rel.insert("d1".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    let ds = Dataset::new(corpus, queries, judgments);

    let chunked = chunk_dataset(&ds, 30, 0).expect("chunk ok");
    // Sanity: chunking must produce more docs than the source.
    assert!(chunked.corpus.len() > ds.corpus.len());

    let retriever = Bm25Eval::from_dataset(&chunked.as_dataset());
    let config = EvalConfig {
      k_values: vec![1, 3, 5],
      label: "chunk-30".into(),
    };
    let report = evaluate_with_remapping(
      &retriever,
      &chunked.as_dataset(),
      &Some(chunked.chunk_to_doc.clone()),
      &config,
    )
    .expect("evaluate ok");

    // At K=1, the remap+dedupe step should have surfaced "d1" as
    // the top result. Without remap, the report would carry a
    // chunk id like "d1::chunk1" which qrels can't match → recall=0.
    let recall_at_1 = report
      .per_k
      .iter()
      .find(|row| row.k == 1)
      .map(|row| row.recall)
      .unwrap();
    assert!(
      recall_at_1 > 0.0,
      "remap+dedupe failed: recall@1={recall_at_1}; per_query={:#?}",
      report.per_query
    );
    // The per-query row also carries the remapped result text — no
    // chunk-id leakage into the scored ids.
    let per_query_row = &report.per_query[0];
    assert_eq!(per_query_row.query_id, "q1");
  }

  #[test]
  fn remap_passes_through_unknown_chunk_ids_unchanged() {
    // Defensive: a retriever id missing from the map (stale index,
    // mis-built chunk id) flows through verbatim. The qrels-eval
    // step then scores it normally — if qrels don't recognise the
    // id, it contributes 0 to recall, which is the safe default.
    let map = HashMap::from([("d1::chunk0".to_string(), "d1".to_string())]);
    let raw = vec!["d1::chunk0".to_string(), "rogue_id".to_string()];
    let out = remap_chunks_to_doc_ids(&raw, &map);
    assert_eq!(out, vec!["d1".to_string(), "rogue_id".to_string()]);
  }
}
