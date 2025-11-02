//! Document indexing pipeline

use crate::{
  chunking::ChunkingStrategy,
  embeddings::EmbeddingProvider,
  error::Result,
  types::{Document, IndexingStats},
  vectorstore::VectorStore,
};
use std::time::Instant;

/// Indexing pipeline for processing and storing documents
pub struct IndexingPipeline {
  chunker: Box<dyn ChunkingStrategy>,
  embedder: Box<dyn EmbeddingProvider>,
  store: Box<dyn VectorStore>,
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
    }
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

  /// Index multiple documents in batch
  pub async fn index_documents(
    &self,
    collection: &str,
    docs: Vec<Document>,
  ) -> Result<IndexingStats> {
    let start = Instant::now();
    let mut total_stats = IndexingStats::default();

    for doc in docs {
      match self.index_document(collection, doc).await {
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
