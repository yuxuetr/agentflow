//! Fixed-size chunking strategy

use crate::{
  chunking::ChunkingStrategy,
  error::{RAGError, Result},
  types::TextChunk,
};
use std::collections::HashMap;

pub struct FixedSizeChunker {
  chunk_size: usize,
  overlap: usize,
}

impl FixedSizeChunker {
  /// Q3.9.1: infallible constructor that clamps the inputs so the
  /// `chunk_size - overlap` arithmetic at line 47 below cannot
  /// underflow / panic. Specifically:
  ///   * `chunk_size = 0` is silently bumped to `1` (a zero-sized
  ///     chunker is meaningless; pre-Q3.9.1 it would loop forever).
  ///   * `overlap >= chunk_size` is clamped to `chunk_size - 1`
  ///     (any forward stride of at least 1 prevents infinite loops).
  /// Callers that want a hard error on bad inputs should use
  /// [`try_new`](Self::try_new) instead.
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    let chunk_size = chunk_size.max(1);
    let overlap = overlap.min(chunk_size.saturating_sub(1));
    Self { chunk_size, overlap }
  }

  /// Q3.9.1: fallible constructor that rejects `chunk_size == 0` and
  /// `overlap >= chunk_size`. Use this from YAML / config-driven
  /// code paths where a misconfiguration should surface an error
  /// rather than be silently clamped.
  pub fn try_new(chunk_size: usize, overlap: usize) -> Result<Self> {
    if chunk_size == 0 {
      return Err(RAGError::chunking(
        "FixedSizeChunker requires chunk_size > 0",
      ));
    }
    if overlap >= chunk_size {
      return Err(RAGError::chunking(format!(
        "FixedSizeChunker requires overlap < chunk_size; got overlap={overlap} chunk_size={chunk_size}"
      )));
    }
    Ok(Self { chunk_size, overlap })
  }
}

impl ChunkingStrategy for FixedSizeChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    let mut chunks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let total_len = chars.len();

    if total_len == 0 {
      return Ok(chunks);
    }

    let mut start_idx = 0;
    let mut chunk_index = 0;

    while start_idx < total_len {
      let end_idx = (start_idx + self.chunk_size).min(total_len);
      let chunk_text: String = chars[start_idx..end_idx].iter().collect();

      chunks.push(TextChunk {
        content: chunk_text,
        start_idx,
        end_idx,
        metadata: HashMap::new(),
        chunk_index,
        total_chunks: 0, // Will be updated
      });

      chunk_index += 1;
      start_idx += self.chunk_size - self.overlap;

      if start_idx >= total_len {
        break;
      }
    }

    // Update total_chunks
    let total = chunks.len();
    for chunk in &mut chunks {
      chunk.total_chunks = total;
    }

    Ok(chunks)
  }

  fn chunk_size(&self) -> usize {
    self.chunk_size
  }

  fn overlap(&self) -> usize {
    self.overlap
  }

  fn strategy_name(&self) -> &str {
    "fixed_size"
  }
}
