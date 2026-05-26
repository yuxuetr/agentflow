//! Document indexing pipeline

use crate::{
  chunking::ChunkingStrategy,
  embeddings::EmbeddingProvider,
  error::Result,
  types::{Document, IndexingStats},
  vectorstore::VectorStore,
};
use futures::stream::{self, StreamExt};
use std::time::Instant;

/// Default per-pipeline concurrency for `index_documents`. Picked
/// conservatively because both phases of `index_document` make
/// blocking network calls:
///
/// - Embedding: cloud providers (OpenAI / similar) impose per-key
///   request-rate + token-rate limits; running 32 chunk-batches in
///   parallel typically just trades wall-clock for 429s.
/// - Vector store writes: shared-pool Qdrant deployments throttle
///   when concurrent upsert clients exceed `shard_count *
///   max_in_flight`.
///
/// 4 lets a single-doc pipeline beat its sequential baseline by ~3x
/// on a 64-doc corpus while staying well clear of the smallest free
/// embedding tiers. Operators with dedicated infra can bump it via
/// `IndexingPipeline::with_max_concurrency` or the explicit
/// `index_documents_with_concurrency` entry point.
pub const DEFAULT_INDEX_CONCURRENCY: usize = 4;

/// Indexing pipeline for processing and storing documents
pub struct IndexingPipeline {
  chunker: Box<dyn ChunkingStrategy>,
  embedder: Box<dyn EmbeddingProvider>,
  store: Box<dyn VectorStore>,
  /// Q3.9.4: governs how many `index_document` invocations may be
  /// in flight at once when `index_documents` fans out across the
  /// input corpus. Always `>= 1` because constructors clamp via
  /// `.max(1)`; `0` would otherwise make `buffer_unordered` stall.
  max_concurrency: usize,
}

impl IndexingPipeline {
  /// Create a new indexing pipeline
  pub fn new(
    chunker: Box<dyn ChunkingStrategy>,
    embedder: Box<dyn EmbeddingProvider>,
    store: Box<dyn VectorStore>,
  ) -> Self {
    Self {
      chunker,
      embedder,
      store,
      max_concurrency: DEFAULT_INDEX_CONCURRENCY,
    }
  }

  /// Override the per-pipeline `index_documents` concurrency cap.
  ///
  /// Q3.9.4: zero is clamped to `1` so callers can't accidentally
  /// stall the pipeline by passing an unchecked `usize` (e.g. from a
  /// YAML config field that defaulted to `0`). Cloud providers with
  /// generous quotas may want 8–16; CPU-bound local-embedding setups
  /// usually want 1 because the underlying ONNX session is
  /// single-threaded.
  pub fn with_max_concurrency(mut self, concurrency: usize) -> Self {
    self.max_concurrency = concurrency.max(1);
    self
  }

  /// Read the active concurrency cap.
  pub fn max_concurrency(&self) -> usize {
    self.max_concurrency
  }

  /// Index a single document
  pub async fn index_document(&self, collection: &str, doc: Document) -> Result<IndexingStats> {
    let start = Instant::now();
    let mut stats = IndexingStats::default();

    // 1. Chunk document
    let chunks = self.chunker.chunk(&doc.content)?;
    stats.chunks_created = chunks.len();

    // 2. Generate embeddings for each chunk
    let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
    let embeddings = self.embedder.embed_batch(texts).await?;
    stats.embeddings_generated = embeddings.len();

    // 3. Create documents from chunks
    let chunk_docs: Vec<Document> = chunks
      .into_iter()
      .zip(embeddings)
      .enumerate()
      .map(|(idx, (chunk, embedding))| {
        let mut metadata = doc.metadata.clone();
        metadata.insert("chunk_index".to_string(), (idx as i64).into());
        metadata.insert("original_doc_id".to_string(), doc.id.clone().into());

        Document {
          id: format!("{}_{}", doc.id, idx),
          content: chunk.content,
          metadata,
          embedding: Some(embedding),
        }
      })
      .collect();

    // 4. Store in vector database
    self.store.add_documents(collection, chunk_docs).await?;
    stats.documents_processed = 1;

    stats.processing_time_ms = start.elapsed().as_millis() as u64;
    Ok(stats)
  }

  /// Index multiple documents using the pipeline's configured
  /// `max_concurrency` (default [`DEFAULT_INDEX_CONCURRENCY`]).
  ///
  /// Q3.9.4: pre-fix this was a plain `for doc in docs { ... await }`
  /// loop, so a 64-document corpus paid N × per-document latency
  /// even though both the embedding API and the vector store
  /// happily accept concurrent calls. The fan-out is now governed
  /// by `max_concurrency`, so the same corpus finishes in roughly
  /// `N / max_concurrency × per_doc_time` wall clock — modulo
  /// rate-limit interactions which is why the default is
  /// conservative.
  pub async fn index_documents(
    &self,
    collection: &str,
    docs: Vec<Document>,
  ) -> Result<IndexingStats> {
    self
      .index_documents_with_concurrency(collection, docs, self.max_concurrency)
      .await
  }

  /// One-shot override of the pipeline's concurrency cap. Useful
  /// for migrations where the operator knows the destination store
  /// can absorb a much larger burst than the steady-state default.
  ///
  /// `concurrency.max(1)` matches `with_max_concurrency` so a `0`
  /// from upstream config never stalls the stream.
  pub async fn index_documents_with_concurrency(
    &self,
    collection: &str,
    docs: Vec<Document>,
    concurrency: usize,
  ) -> Result<IndexingStats> {
    let start = Instant::now();
    let mut total_stats = IndexingStats::default();
    let concurrency = concurrency.max(1);

    // `buffer_unordered` lets in-flight futures complete out of
    // order; aggregation only sums, so order doesn't matter. Each
    // closure borrows `&self`, which is fine because every future
    // resolves before the outer `await` here returns — `self` is
    // alive for the whole stream.
    let results: Vec<Result<IndexingStats>> = stream::iter(docs)
      .map(|doc| self.index_document(collection, doc))
      .buffer_unordered(concurrency)
      .collect()
      .await;

    for result in results {
      match result {
        Ok(stats) => {
          total_stats.documents_processed += stats.documents_processed;
          total_stats.chunks_created += stats.chunks_created;
          total_stats.embeddings_generated += stats.embeddings_generated;
        }
        Err(e) => {
          tracing::error!("Failed to index document: {}", e);
          total_stats.errors += 1;
        }
      }
    }

    total_stats.processing_time_ms = start.elapsed().as_millis() as u64;
    Ok(total_stats)
  }
}

#[cfg(test)]
mod tests {
  //! Q3.9.4 regression coverage. Uses in-memory stub
  //! `EmbeddingProvider` + `VectorStore` impls so the tests exercise
  //! actual concurrency rather than network behavior.
  use super::*;
  use crate::{
    chunking::FixedSizeChunker,
    embeddings::EmbeddingProvider,
    error::RAGError,
    types::{CollectionConfig, Filter, SearchResult},
    vectorstore::{CollectionStats, VectorStore},
  };
  use async_trait::async_trait;
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::time::Duration;

  /// Stub embedder whose `embed_batch` sleeps to simulate I/O. The
  /// peak-in-flight counter is what the Q3.9.4 invariant asserts on:
  /// "≥ 2 docs must have their embedder calls overlapping at some
  /// point during `index_documents`."
  struct DelayEmbedder {
    delay: Duration,
    in_flight: AtomicUsize,
    peak: Arc<AtomicUsize>,
  }

  impl DelayEmbedder {
    fn new(delay: Duration) -> (Self, Arc<AtomicUsize>) {
      let peak = Arc::new(AtomicUsize::new(0));
      (
        Self {
          delay,
          in_flight: AtomicUsize::new(0),
          peak: peak.clone(),
        },
        peak,
      )
    }

    fn note_in_flight(&self) {
      let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
      // Walk `peak` up via CAS until we either set the new max or
      // observe one already past us. Avoids the `fetch_max` MSRV
      // gotcha on older toolchains.
      let mut current_peak = self.peak.load(Ordering::SeqCst);
      while now > current_peak {
        match self
          .peak
          .compare_exchange(current_peak, now, Ordering::SeqCst, Ordering::SeqCst)
        {
          Ok(_) => break,
          Err(observed) => current_peak = observed,
        }
      }
    }
  }

  #[async_trait]
  impl EmbeddingProvider for DelayEmbedder {
    async fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
      self.note_in_flight();
      tokio::time::sleep(self.delay).await;
      self.in_flight.fetch_sub(1, Ordering::SeqCst);
      Ok(vec![0.1, 0.2, 0.3])
    }

    async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
      self.note_in_flight();
      tokio::time::sleep(self.delay).await;
      self.in_flight.fetch_sub(1, Ordering::SeqCst);
      Ok(vec![vec![0.1, 0.2, 0.3]; texts.len()])
    }

    fn dimension(&self) -> usize {
      3
    }

    fn model_name(&self) -> &str {
      "delay-stub"
    }
  }

  /// In-memory vector store stub. Only `add_documents` is meaningful
  /// for the indexing tests; the rest of the trait surface returns
  /// `Default`-like values so the test compiles.
  struct CountingStore {
    landed: AtomicUsize,
  }

  impl CountingStore {
    fn new() -> Self {
      Self {
        landed: AtomicUsize::new(0),
      }
    }
  }

  #[async_trait]
  impl VectorStore for CountingStore {
    async fn create_collection(&self, _name: &str, _cfg: CollectionConfig) -> Result<()> {
      Ok(())
    }

    async fn delete_collection(&self, _name: &str) -> Result<()> {
      Ok(())
    }

    async fn collection_exists(&self, _name: &str) -> Result<bool> {
      Ok(true)
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
      Ok(Vec::new())
    }

    async fn add_documents(&self, _collection: &str, docs: Vec<Document>) -> Result<Vec<String>> {
      let ids: Vec<String> = docs.iter().map(|d| d.id.clone()).collect();
      self.landed.fetch_add(docs.len(), Ordering::SeqCst);
      Ok(ids)
    }

    async fn delete_documents(&self, _collection: &str, _ids: Vec<String>) -> Result<()> {
      Ok(())
    }

    async fn similarity_search(
      &self,
      _collection: &str,
      _query: &str,
      _top_k: usize,
      _filter: Option<Filter>,
    ) -> Result<Vec<SearchResult>> {
      Ok(Vec::new())
    }

    async fn similarity_search_by_vector(
      &self,
      _collection: &str,
      _vector: Vec<f32>,
      _top_k: usize,
      _filter: Option<Filter>,
    ) -> Result<Vec<SearchResult>> {
      Ok(Vec::new())
    }

    async fn get_collection_stats(&self, name: &str) -> Result<CollectionStats> {
      Ok(CollectionStats {
        name: name.to_string(),
        document_count: self.landed.load(Ordering::SeqCst),
        dimension: 3,
        index_size_bytes: 0,
      })
    }
  }

  fn make_docs(n: usize) -> Vec<Document> {
    (0..n)
      .map(|i| Document::with_id(format!("doc-{i}"), format!("document number {i} content")))
      .collect()
  }

  #[tokio::test]
  async fn index_documents_runs_at_least_two_documents_concurrently() {
    // Q3.9.4 regression: pre-fix the loop was sequential so peak
    // in-flight stayed at 1. Post-fix `buffer_unordered(4)` must
    // drive at least 2 concurrent embedder calls.
    let (embedder, peak) = DelayEmbedder::new(Duration::from_millis(50));
    let pipeline = IndexingPipeline::new(
      Box::new(FixedSizeChunker::new(64, 0)),
      Box::new(embedder),
      Box::new(CountingStore::new()),
    );

    let stats = pipeline
      .index_documents("c", make_docs(8))
      .await
      .expect("index_documents ok");
    assert_eq!(stats.documents_processed, 8);
    let peak_observed = peak.load(Ordering::SeqCst);
    assert!(
      peak_observed >= 2,
      "Q3.9.4: peak in-flight embedder calls must exceed 1 \
       (got {peak_observed}); the pipeline regressed to sequential",
    );
  }

  #[tokio::test]
  async fn index_documents_completes_faster_than_serial_bound() {
    // Concurrent dispatch must finish well under the sequential
    // bound (N × per-doc latency). Allow generous headroom for
    // tokio scheduling: assert "less than half the serial wall
    // clock" rather than fighting CI flakiness with tighter bounds.
    let per_doc = Duration::from_millis(40);
    let n_docs = 8;
    let serial_bound = per_doc * n_docs as u32;

    let (embedder, _peak) = DelayEmbedder::new(per_doc);
    let pipeline = IndexingPipeline::new(
      Box::new(FixedSizeChunker::new(64, 0)),
      Box::new(embedder),
      Box::new(CountingStore::new()),
    )
    .with_max_concurrency(4);

    let start = Instant::now();
    let _ = pipeline
      .index_documents("c", make_docs(n_docs))
      .await
      .expect("index ok");
    let elapsed = start.elapsed();

    assert!(
      elapsed * 2 < serial_bound,
      "Q3.9.4: 8 docs × 40ms with concurrency=4 must finish under \
       half the serial bound; got {elapsed:?} vs serial {serial_bound:?}",
    );
  }

  #[tokio::test]
  async fn with_max_concurrency_clamps_zero_to_one() {
    // Defensive: `concurrency = 0` would make `buffer_unordered`
    // stall forever, which is a footgun for YAML-driven config
    // that defaulted to 0.
    let (embedder, _peak) = DelayEmbedder::new(Duration::from_millis(1));
    let pipeline = IndexingPipeline::new(
      Box::new(FixedSizeChunker::new(64, 0)),
      Box::new(embedder),
      Box::new(CountingStore::new()),
    )
    .with_max_concurrency(0);
    assert_eq!(pipeline.max_concurrency(), 1);
    let stats = pipeline
      .index_documents("c", make_docs(3))
      .await
      .expect("index ok");
    assert_eq!(stats.documents_processed, 3);
  }

  #[tokio::test]
  async fn index_documents_with_concurrency_overrides_pipeline_default() {
    // A migration job might want a burst far above the steady-state
    // default. The one-shot override entry point exists so callers
    // don't have to mutate the pipeline. Pin `max_concurrency = 1`
    // so the override is what's driving the observed fan-out.
    let (embedder, peak) = DelayEmbedder::new(Duration::from_millis(30));
    let pipeline = IndexingPipeline::new(
      Box::new(FixedSizeChunker::new(64, 0)),
      Box::new(embedder),
      Box::new(CountingStore::new()),
    )
    .with_max_concurrency(1);
    let _ = pipeline
      .index_documents_with_concurrency("c", make_docs(6), 6)
      .await
      .expect("index ok");
    let peak_observed = peak.load(Ordering::SeqCst);
    assert!(
      peak_observed >= 2,
      "override should drive peak above the pipeline-default 1; \
       got {peak_observed}",
    );
  }

  /// Error in one document must not abort the rest of the batch:
  /// the pre-Q3.9.4 loop bumped `errors` on `Err` but kept going.
  /// Post-fix `buffer_unordered` keeps the same semantic because
  /// `collect` waits for every future regardless of result.
  #[tokio::test]
  async fn index_documents_isolates_per_document_errors() {
    struct OneFailEmbedder {
      counter: AtomicUsize,
    }
    #[async_trait]
    impl EmbeddingProvider for OneFailEmbedder {
      async fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0])
      }
      async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        if n == 1 {
          return Err(RAGError::embedding("simulated"));
        }
        Ok(vec![vec![0.0]; texts.len()])
      }
      fn dimension(&self) -> usize {
        1
      }
      fn model_name(&self) -> &str {
        "one-fail-stub"
      }
    }
    let pipeline = IndexingPipeline::new(
      Box::new(FixedSizeChunker::new(64, 0)),
      Box::new(OneFailEmbedder {
        counter: AtomicUsize::new(0),
      }),
      Box::new(CountingStore::new()),
    );
    let stats = pipeline
      .index_documents("c", make_docs(4))
      .await
      .expect("aggregate doesn't bubble per-doc errors");
    assert_eq!(stats.errors, 1);
    assert_eq!(stats.documents_processed, 3);
  }
}
