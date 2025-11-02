//! Sentence-based chunking strategy

use crate::{chunking::ChunkingStrategy, error::Result, types::TextChunk};

pub struct SentenceChunker {
  chunk_size: usize,
  overlap: usize,
}

impl SentenceChunker {
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    Self { chunk_size, overlap }
  }
}

impl ChunkingStrategy for SentenceChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    // TODO: Implement sentence-based chunking
    // For now, fall back to fixed size
    let fixed_chunker = super::fixed_size::FixedSizeChunker::new(self.chunk_size, self.overlap);
    fixed_chunker.chunk(text)
  }

  fn chunk_size(&self) -> usize {
    self.chunk_size
  }

  fn overlap(&self) -> usize {
    self.overlap
  }

  fn strategy_name(&self) -> &str {
    "sentence"
  }
}
