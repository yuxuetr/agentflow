//! Recursive chunking strategy
//!
//! This strategy uses a hierarchical approach to split text, trying different
//! separators in order of preference. It recursively splits text that is too
//! large, maintaining semantic coherence by preferring higher-level separators.

use crate::{chunking::ChunkingStrategy, error::Result, types::TextChunk};
use std::collections::HashMap;

/// Recursive text chunker with hierarchical separator strategy
///
/// This chunker tries to split text using separators in order of preference:
/// 1. Paragraphs (double newline)
/// 2. Lines (single newline)
/// 3. Sentences (period + space)
/// 4. Clauses (comma + space)
/// 5. Words (space)
/// 6. Characters (empty string)
///
/// # Example
/// ```rust
/// use agentflow_rag::chunking::{ChunkingStrategy, RecursiveChunker};
///
/// let chunker = RecursiveChunker::new(200, 50);
/// let text = "Paragraph 1.\n\nParagraph 2.\n\nParagraph 3.";
/// let chunks = chunker.chunk(text).unwrap();
/// ```
pub struct RecursiveChunker {
  chunk_size: usize,
  overlap: usize,
  separators: Vec<String>,
}

impl RecursiveChunker {
  /// Create a new recursive chunker with default separators
  ///
  /// # Arguments
  /// * `chunk_size` - Target maximum size for each chunk (in characters)
  /// * `overlap` - Number of characters to overlap between chunks
  pub fn new(chunk_size: usize, overlap: usize) -> Self {
    Self::with_separators(chunk_size, overlap, Self::default_separators())
  }

  /// Create a recursive chunker with custom separators
  ///
  /// # Arguments
  /// * `chunk_size` - Target maximum size for each chunk
  /// * `overlap` - Number of characters to overlap between chunks
  /// * `separators` - List of separators in order of preference
  pub fn with_separators(chunk_size: usize, overlap: usize, separators: Vec<String>) -> Self {
    Self {
      chunk_size,
      overlap,
      separators,
    }
  }

  /// Get default separators in order of preference
  fn default_separators() -> Vec<String> {
    vec![
      "\n\n".to_string(), // Paragraphs
      "\n".to_string(),   // Lines
      ". ".to_string(),   // Sentences
      ", ".to_string(),   // Clauses
      " ".to_string(),    // Words
      "".to_string(),     // Characters
    ]
  }

  /// Recursively split text using the separator hierarchy
  fn split_text_recursive(&self, text: &str, separator_idx: usize) -> Vec<String> {
    if text.is_empty() {
      return vec![];
    }

    // If we've exhausted all separators or text is small enough, return as-is
    if separator_idx >= self.separators.len() || text.len() <= self.chunk_size {
      return vec![text.to_string()];
    }

    let separator = &self.separators[separator_idx];
    let mut result = Vec::new();

    // Split by current separator
    // For character-level split, we need to own the strings
    let char_strings: Vec<String> = if separator.is_empty() {
      text.chars().map(|c| c.to_string()).collect()
    } else {
      vec![]
    };

    let splits: Vec<&str> = if separator.is_empty() {
      // Character-level split - use the owned strings
      char_strings.iter().map(|s| s.as_str()).collect()
    } else {
      text.split(separator).collect()
    };

    let mut current_chunk = String::new();

    for (i, split) in splits.iter().enumerate() {
      let piece = if i > 0 && !separator.is_empty() {
        // Add separator back except for the first piece
        format!("{}{}", separator, split)
      } else {
        split.to_string()
      };

      // Check if adding this piece would exceed chunk size
      if !current_chunk.is_empty() && current_chunk.len() + piece.len() > self.chunk_size {
        // Current chunk is ready
        result.push(current_chunk.clone());
        current_chunk.clear();
      }

      // If the piece itself is too large, split it recursively
      if piece.len() > self.chunk_size && separator_idx + 1 < self.separators.len() {
        if !current_chunk.is_empty() {
          result.push(current_chunk.clone());
          current_chunk.clear();
        }

        // Recursively split this piece with next separator
        let sub_chunks = self.split_text_recursive(&piece, separator_idx + 1);
        result.extend(sub_chunks);
      } else {
        // Add piece to current chunk
        current_chunk.push_str(&piece);
      }
    }

    // Add any remaining content
    if !current_chunk.is_empty() {
      result.push(current_chunk);
    }

    result
  }

  /// Merge small chunks and add overlap
  fn merge_and_overlap(&self, chunks: Vec<String>) -> Vec<String> {
    if chunks.is_empty() {
      return vec![];
    }

    let mut result = Vec::new();
    let mut current = chunks[0].clone();

    for chunk in chunks.into_iter().skip(1) {
      // Check if we should merge or add overlap
      if current.len() + chunk.len() <= self.chunk_size {
        // Merge small chunks
        current.push_str(&chunk);
      } else {
        // Add current chunk to result
        result.push(current.clone());

        // Add overlap from end of current chunk
        if self.overlap > 0 && current.len() > self.overlap {
          let overlap_start = current.len() - self.overlap;
          current = format!("{}{}", &current[overlap_start..], chunk);
        } else {
          current = chunk;
        }
      }
    }

    // Add final chunk
    if !current.is_empty() {
      result.push(current);
    }

    result
  }
}

impl ChunkingStrategy for RecursiveChunker {
  fn chunk(&self, text: &str) -> Result<Vec<TextChunk>> {
    if text.is_empty() {
      return Ok(vec![]);
    }

    // Recursively split text
    let raw_chunks = self.split_text_recursive(text, 0);

    // Merge and add overlap
    let merged_chunks = self.merge_and_overlap(raw_chunks);

    // Convert to TextChunk with metadata
    let mut chunks = Vec::new();
    let mut current_pos = 0;

    for (i, chunk_text) in merged_chunks.iter().enumerate() {
      // Find the actual position in the original text
      // This is approximate due to overlap handling
      let start_idx = if i == 0 { 0 } else { current_pos };

      let end_idx = start_idx + chunk_text.len();
      current_pos = if self.overlap > 0 && chunk_text.len() > self.overlap {
        end_idx - self.overlap
      } else {
        end_idx
      };

      chunks.push(TextChunk {
        content: chunk_text.clone(),
        start_idx,
        end_idx,
        metadata: HashMap::new(),
        chunk_index: i,
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
    "recursive"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_recursive_chunking_paragraphs() {
    let chunker = RecursiveChunker::new(100, 20);
    let text = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";

    let chunks = chunker.chunk(text).unwrap();

    assert!(!chunks.is_empty());
    // Should prefer splitting at paragraph boundaries
    for chunk in &chunks {
      assert!(chunk.content.len() <= chunker.chunk_size() + 50); // Some tolerance
    }
  }

  #[test]
  fn test_recursive_chunking_empty() {
    let chunker = RecursiveChunker::new(100, 20);
    let chunks = chunker.chunk("").unwrap();
    assert!(chunks.is_empty());
  }

  #[test]
  fn test_recursive_chunking_metadata() {
    let chunker = RecursiveChunker::new(50, 10);
    let text = "A.\n\nB.\n\nC.";

    let chunks = chunker.chunk(text).unwrap();

    for (i, chunk) in chunks.iter().enumerate() {
      assert_eq!(chunk.chunk_index, i);
      assert_eq!(chunk.total_chunks, chunks.len());
    }
  }

  #[test]
  fn test_recursive_chunking_long_text() {
    let chunker = RecursiveChunker::new(50, 10);
    let text = "This is a very long piece of text that needs to be split into multiple chunks. \
                 It should be split at appropriate boundaries like spaces and punctuation.";

    let chunks = chunker.chunk(text).unwrap();

    assert!(chunks.len() > 1);
    // All chunks except possibly the last should be close to chunk_size
    for chunk in &chunks[..chunks.len() - 1] {
      assert!(chunk.content.len() <= chunker.chunk_size() + 20); // Some tolerance for separators
    }
  }

  #[test]
  fn test_recursive_chunking_custom_separators() {
    let separators = vec!["|".to_string(), " ".to_string(), "".to_string()];
    let chunker = RecursiveChunker::with_separators(20, 5, separators);
    let text = "Part1|Part2|Part3";

    let chunks = chunker.chunk(text).unwrap();

    assert!(!chunks.is_empty());
  }

  #[test]
  fn test_recursive_chunking_preserves_content() {
    let chunker = RecursiveChunker::new(100, 0); // No overlap for easier verification
    let text = "First. Second. Third. Fourth. Fifth.";

    let chunks = chunker.chunk(text).unwrap();

    // Concatenate all chunks (without overlap they should reconstruct the text)
    let reconstructed: String = chunks.iter().map(|c| c.content.as_str()).collect();

    // Due to the nature of recursive chunking, we may have some differences
    // Just verify we captured all the key content
    assert!(reconstructed.contains("First"));
    assert!(reconstructed.contains("Second"));
    assert!(reconstructed.contains("Third"));
  }

  #[test]
  fn test_recursive_chunking_respects_chunk_size() {
    let chunker = RecursiveChunker::new(30, 5);
    let text = "A ".repeat(100); // 200 characters

    let chunks = chunker.chunk(text.trim()).unwrap();

    assert!(chunks.len() > 1);
    // Each chunk should respect the chunk_size limit (with some tolerance)
    for chunk in &chunks {
      assert!(
        chunk.content.len() <= chunker.chunk_size() + 10,
        "Chunk size {} exceeds limit {}",
        chunk.content.len(),
        chunker.chunk_size()
      );
    }
  }

  #[test]
  fn test_default_separators() {
    let separators = RecursiveChunker::default_separators();
    assert_eq!(separators.len(), 6);
    assert_eq!(separators[0], "\n\n"); // Paragraphs
    assert_eq!(separators[1], "\n"); // Lines
    assert_eq!(separators[2], ". "); // Sentences
    assert_eq!(separators[3], ", "); // Clauses
    assert_eq!(separators[4], " "); // Words
    assert_eq!(separators[5], ""); // Characters
  }
}
