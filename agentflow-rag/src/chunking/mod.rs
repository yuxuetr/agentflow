//! Document chunking strategies

use crate::{error::Result, types::TextChunk};

pub mod fixed_size;
pub mod recursive;
pub mod sentence;

pub use fixed_size::FixedSizeChunker;
pub use recursive::RecursiveChunker;
pub use sentence::SentenceChunker;

/// Chunking strategy trait
pub trait ChunkingStrategy: Send + Sync {
  /// Split text into chunks
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>>;

  /// Get chunk size
  fn chunk_size(&self) -> usize;

  /// Get overlap size
  fn overlap(&self) -> usize;

  /// Get strategy name
  fn strategy_name(&self) -> &str;
}

/// Create a chunking strategy from configuration
pub fn create_chunker(
  strategy: crate::types::ChunkingStrategy,
  chunk_size: usize,
  overlap: usize,
) -> Result<Box<dyn ChunkingStrategy>> {
  match strategy {
    crate::types::ChunkingStrategy::FixedSize => Ok(Box::new(FixedSizeChunker::new(chunk_size, overlap))),
    crate::types::ChunkingStrategy::Sentence => Ok(Box::new(SentenceChunker::new(chunk_size, overlap))),
    crate::types::ChunkingStrategy::Recursive => Ok(Box::new(RecursiveChunker::new(chunk_size, overlap))),
    crate::types::ChunkingStrategy::Semantic => {
      Err(crate::error::RAGError::chunking(
        "Semantic chunking not yet implemented",
      ))
    }
  }
}
