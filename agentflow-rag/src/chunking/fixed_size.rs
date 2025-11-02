//! Fixed-size chunking strategy

use crate::{chunking::ChunkingStrategy, error::Result, types::TextChunk};
use std::collections::HashMap;

pub struct FixedSizeChunker {
  chunk_size: usize,
  overlap: usize,
}

impl FixedSizeChunker {
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    Self { chunk_size, overlap }
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
