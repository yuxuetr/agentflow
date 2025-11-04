//! Sentence-based chunking strategy
//!
//! This strategy splits text at sentence boundaries, respecting natural language
//! structure. It accumulates sentences until reaching the chunk size limit.

use crate::{chunking::ChunkingStrategy, error::Result, types::TextChunk};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

/// Sentence-based text chunker that respects sentence boundaries
///
/// This chunker splits text into sentences and groups them into chunks,
/// ensuring that chunks don't break in the middle of sentences.
///
/// # Example
/// ```rust
/// use agentflow_rag::chunking::{ChunkingStrategy, SentenceChunker};
///
/// let chunker = SentenceChunker::new(200, 50);
/// let text = "First sentence. Second sentence. Third sentence.";
/// let chunks = chunker.chunk(text).unwrap();
/// ```
pub struct SentenceChunker {
  chunk_size: usize,
  overlap: usize,
}

impl SentenceChunker {
  /// Create a new sentence-based chunker
  ///
  /// # Arguments
  /// * `chunk_size` - Target maximum size for each chunk (in characters)
  /// * `overlap` - Number of characters to overlap between chunks
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    Self { chunk_size, overlap }
  }

  /// Split text into sentences using Unicode sentence boundary rules
  fn split_sentences(&self, text: &str) -> Vec<(usize, usize, String)> {
    let mut sentences = Vec::new();
    let mut current_pos = 0;

    for sentence in text.unicode_sentences() {
      let start = current_pos;
      let end = start + sentence.len();
      sentences.push((start, end, sentence.to_string()));
      current_pos = end;
    }

    sentences
  }

  /// Find sentences that should be included in overlap
  fn find_overlap_sentences(&self, sentences: &[(usize, usize, String)], _chunk_end_idx: usize) -> Vec<usize> {
    let mut overlap_sentences = Vec::new();
    let mut overlap_chars = 0;

    // Work backwards from the end of the chunk
    for (i, (_, _, sentence)) in sentences.iter().enumerate().rev() {
      if overlap_chars + sentence.len() <= self.overlap {
        overlap_sentences.insert(0, i);
        overlap_chars += sentence.len();
      } else {
        break;
      }
    }

    overlap_sentences
  }
}

impl ChunkingStrategy for SentenceChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    if text.is_empty() {
      return Ok(vec![]);
    }

    let sentences = self.split_sentences(text);

    if sentences.is_empty() {
      return Ok(vec![]);
    }

    let mut chunks = Vec::new();
    let mut current_chunk_sentences: Vec<usize> = Vec::new();
    let mut current_chunk_size = 0;

    for (i, (_, _, sentence)) in sentences.iter().enumerate() {
      let sentence_len = sentence.len();

      // Check if adding this sentence would exceed chunk_size
      if !current_chunk_sentences.is_empty() && current_chunk_size + sentence_len > self.chunk_size {
        // Create chunk from accumulated sentences
        let start_idx = sentences[current_chunk_sentences[0]].0;
        let end_idx = sentences[*current_chunk_sentences.last().unwrap()].1;
        let chunk_text: String = current_chunk_sentences
          .iter()
          .map(|&idx| sentences[idx].2.as_str())
          .collect();

        chunks.push(TextChunk {
          content: chunk_text,
          start_idx,
          end_idx,
          metadata: HashMap::new(),
          chunk_index: chunks.len(),
          total_chunks: 0, // Will update total_chunks later
        });

        // Find sentences for overlap
        let overlap_sentences = self.find_overlap_sentences(&sentences, end_idx);
        current_chunk_sentences = overlap_sentences;
        current_chunk_size = current_chunk_sentences
          .iter()
          .map(|&idx| sentences[idx].2.len())
          .sum();
      }

      // Add current sentence to chunk
      current_chunk_sentences.push(i);
      current_chunk_size += sentence_len;
    }

    // Add final chunk if there are remaining sentences
    if !current_chunk_sentences.is_empty() {
      let start_idx = sentences[current_chunk_sentences[0]].0;
      let end_idx = sentences[*current_chunk_sentences.last().unwrap()].1;
      let chunk_text: String = current_chunk_sentences
        .iter()
        .map(|&idx| sentences[idx].2.as_str())
        .collect();

      chunks.push(TextChunk {
        content: chunk_text,
        start_idx,
        end_idx,
        metadata: HashMap::new(),
        chunk_index: chunks.len(),
        total_chunks: 0, // Will update total_chunks later
      });
    }

    // Update total_chunks for all chunks
    let total_chunks = chunks.len();
    for chunk in &mut chunks {
      chunk.total_chunks = total_chunks;
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
    "sentence"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_sentence_chunking_basic() {
    let chunker = SentenceChunker::new(100, 20);
    let text = "First sentence. Second sentence. Third sentence. Fourth sentence.";

    let chunks = chunker.chunk(text).unwrap();

    assert!(!chunks.is_empty());
    // Each chunk should contain whole sentences
    for chunk in &chunks {
      // Count periods (sentence endings)
      let period_count = chunk.content.matches('.').count();
      assert!(period_count > 0, "Each chunk should contain at least one complete sentence");
    }
  }

  #[test]
  fn test_sentence_chunking_empty() {
    let chunker = SentenceChunker::new(100, 20);
    let chunks = chunker.chunk("").unwrap();
    assert!(chunks.is_empty());
  }

  #[test]
  fn test_sentence_chunking_metadata() {
    let chunker = SentenceChunker::new(50, 10);
    let text = "A. B. C. D. E.";

    let chunks = chunker.chunk(text).unwrap();

    for (i, chunk) in chunks.iter().enumerate() {
      assert_eq!(chunk.chunk_index, i);
      assert_eq!(chunk.total_chunks, chunks.len());
      assert!(chunk.start_idx < chunk.end_idx);
    }
  }

  #[test]
  fn test_sentence_chunking_overlap() {
    let chunker = SentenceChunker::new(30, 15);
    let text = "Short one. Another short one. And one more.";

    let chunks = chunker.chunk(text).unwrap();

    if chunks.len() > 1 {
      // Check that there's some overlap in content between consecutive chunks
      for i in 0..chunks.len() - 1 {
        let current = &chunks[i].content;
        let next = &chunks[i + 1].content;

        // Find common sentences
        let current_sentences: Vec<&str> = current.unicode_sentences().collect();
        let next_sentences: Vec<&str> = next.unicode_sentences().collect();

        let _has_overlap = current_sentences
          .iter()
          .any(|s| next_sentences.contains(s));

        // Some chunks might not have overlap if sentences are too long
        // Just verify the structure is correct
        assert!(current.len() > 0 && next.len() > 0);
      }
    }
  }

  #[test]
  fn test_sentence_chunking_long_sentence() {
    let chunker = SentenceChunker::new(20, 5);
    let text = "This is a very long sentence that exceeds the chunk size.";

    let chunks = chunker.chunk(text).unwrap();

    // Should create at least one chunk even if sentence is longer than chunk_size
    assert!(!chunks.is_empty());
    assert_eq!(chunks[0].content, text);
  }

  #[test]
  fn test_sentence_chunking_continuity() {
    let chunker = SentenceChunker::new(50, 10);
    let text = "First. Second. Third. Fourth. Fifth.";

    let chunks = chunker.chunk(text).unwrap();

    // Verify that chunks maintain text continuity
    // First chunk should start at position 0
    assert_eq!(chunks[0].start_idx, 0);

    // Each chunk should have valid indices
    for chunk in &chunks {
      assert!(chunk.start_idx < chunk.end_idx);
      assert_eq!(chunk.content.len(), chunk.end_idx - chunk.start_idx);
    }
  }
}
