//! Semantic chunking strategy
//!
//! This module implements embedding-based semantic chunking that splits text
//! at natural topic boundaries rather than arbitrary character positions.
//!
//! # Algorithm
//!
//! 1. Split text into candidate segments (sentences)
//! 2. Generate embeddings for each segment
//! 3. Calculate cosine similarity between consecutive segments
//! 4. Detect topic boundaries where similarity drops below threshold
//! 5. Merge segments within topic boundaries into chunks
//! 6. Respect maximum chunk size constraints
//! 7. Add overlap between chunks for context preservation
//!
//! # Benefits
//!
//! - Respects semantic boundaries (paragraphs, topics)
//! - Better context preservation
//! - More coherent chunks for retrieval
//! - Improved search quality
//!
//! # Example
//!
//! ```no_run
//! use agentflow_rag::chunking::{ChunkingStrategy, SemanticChunker};
//! use agentflow_rag::embeddings::{EmbeddingProvider, OpenAIEmbedding};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let embedding = OpenAIEmbedding::builder()
//!   .with_api_key("sk-...")
//!   .build()?;
//!
//! let chunker = SemanticChunker::builder()
//!   .with_embedding_provider(embedding)
//!   .with_chunk_size(512)
//!   .with_overlap(50)
//!   .with_similarity_threshold(0.6)
//!   .build();
//!
//! let text = "Long document with multiple topics...";
//! let chunks = chunker.chunk_async(text).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
  chunking::ChunkingStrategy,
  embeddings::EmbeddingProvider,
  error::{RAGError, Result},
  types::TextChunk,
};
use std::sync::Arc;

/// Semantic chunking strategy using embeddings
pub struct SemanticChunker {
  /// Embedding provider for semantic similarity
  embedding_provider: Arc<dyn EmbeddingProvider>,

  /// Target chunk size in characters
  chunk_size: usize,

  /// Overlap between chunks in characters
  overlap: usize,

  /// Similarity threshold for topic boundaries (0.0-1.0)
  /// Lower values = more chunks (stricter boundaries)
  /// Higher values = fewer chunks (more merging)
  similarity_threshold: f32,

  /// Minimum segment size in characters
  min_segment_size: usize,

  /// Buffer percentile for determining boundary threshold
  /// Used to dynamically adjust threshold based on similarity distribution
  buffer_percentile: f32,
}

impl SemanticChunker {
  /// Create a new builder for semantic chunker
  pub fn builder() -> SemanticChunkerBuilder {
    SemanticChunkerBuilder::default()
  }

  /// Chunk text asynchronously using semantic similarity
  pub async fn chunk_async(&self, text: &str) -> Result<Vec<TextChunk>> {
    if text.is_empty() {
      return Ok(Vec::new());
    }

    // Step 1: Split into sentences
    let sentences = self.split_into_sentences(text);

    if sentences.is_empty() {
      return Ok(vec![TextChunk {
        content: text.to_string(),
        start_idx: 0,
        end_idx: text.len(),
        metadata: std::collections::HashMap::new(),
        chunk_index: 0,
        total_chunks: 1,
      }]);
    }

    if sentences.len() == 1 {
      let content = sentences[0].to_string();
      return Ok(vec![TextChunk {
        content: content.clone(),
        start_idx: 0,
        end_idx: content.len(),
        metadata: std::collections::HashMap::new(),
        chunk_index: 0,
        total_chunks: 1,
      }]);
    }

    // Step 2: Generate embeddings for each sentence
    let embeddings = self
      .embedding_provider
      .embed_batch(sentences.iter().map(|s| s.as_str()).collect())
      .await?;

    // Step 3: Calculate similarity scores between consecutive sentences
    let similarities = self.calculate_consecutive_similarities(&embeddings);

    // Step 4: Detect topic boundaries
    let boundaries = self.detect_boundaries(&similarities);

    // Step 5: Group sentences into chunks based on boundaries
    let chunks = self.group_into_chunks(&sentences, &boundaries)?;

    // Step 6: Apply overlap if needed
    let final_chunks = self.apply_overlap(chunks);

    Ok(final_chunks)
  }

  /// Split text into sentences using simple heuristics
  fn split_into_sentences(&self, text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for c in text.chars() {
      current.push(c);

      // Check for sentence endings
      if matches!(c, '.' | '!' | '?') {
        // Look ahead to see if this is really the end of a sentence
        // (not an abbreviation or decimal point)
        let trimmed = current.trim();
        if trimmed.len() >= self.min_segment_size {
          sentences.push(trimmed.to_string());
          current.clear();
        }
      }
    }

    // Add remaining text
    if !current.trim().is_empty() {
      sentences.push(current.trim().to_string());
    }

    // If no sentences were detected, split by newlines
    if sentences.is_empty() {
      sentences = text
        .split('\n')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .collect();
    }

    sentences
  }

  /// Calculate cosine similarity between consecutive embeddings
  fn calculate_consecutive_similarities(&self, embeddings: &[Vec<f32>]) -> Vec<f32> {
    let mut similarities = Vec::with_capacity(embeddings.len().saturating_sub(1));

    for i in 0..embeddings.len().saturating_sub(1) {
      let sim = cosine_similarity(&embeddings[i], &embeddings[i + 1]);
      similarities.push(sim);
    }

    similarities
  }

  /// Detect topic boundaries based on similarity drops
  fn detect_boundaries(&self, similarities: &[f32]) -> Vec<usize> {
    if similarities.is_empty() {
      return vec![0];
    }

    let mut boundaries = vec![0]; // Start with first sentence

    // Use dynamic threshold based on similarity distribution
    let threshold = if self.buffer_percentile > 0.0 {
      self.calculate_dynamic_threshold(similarities)
    } else {
      self.similarity_threshold
    };

    // Find significant drops in similarity
    for (i, &sim) in similarities.iter().enumerate() {
      if sim < threshold {
        boundaries.push(i + 1); // Boundary after index i
      }
    }

    boundaries
  }

  /// Calculate dynamic threshold using percentile
  fn calculate_dynamic_threshold(&self, similarities: &[f32]) -> f32 {
    let mut sorted = similarities.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let index = (sorted.len() as f32 * self.buffer_percentile) as usize;
    let percentile_value = sorted
      .get(index)
      .copied()
      .unwrap_or(self.similarity_threshold);

    // Use the lower of percentile value and fixed threshold
    percentile_value.min(self.similarity_threshold)
  }

  /// Group sentences into chunks based on boundaries
  fn group_into_chunks(
    &self,
    sentences: &[String],
    boundaries: &[usize],
  ) -> Result<Vec<TextChunk>> {
    let mut chunks = Vec::new();
    let mut char_offset = 0;

    for window in boundaries.windows(2) {
      let start = window[0];
      let end = window[1];

      let chunk_text = sentences[start..end].join(" ");

      // Split large chunks that exceed chunk_size
      if chunk_text.len() > self.chunk_size {
        let sub_chunks = self.split_large_chunk(&chunk_text, char_offset);
        for chunk in sub_chunks {
          char_offset += chunk.content.len() + 1; // +1 for space
          chunks.push(chunk);
        }
      } else {
        let start_idx = char_offset;
        let end_idx = start_idx + chunk_text.len();
        chunks.push(TextChunk {
          content: chunk_text.clone(),
          start_idx,
          end_idx,
          metadata: std::collections::HashMap::new(),
          chunk_index: chunks.len(),
          total_chunks: 0, // Will be updated later
        });
        char_offset += chunk_text.len() + 1;
      }
    }

    // Handle last segment
    if let Some(&last_boundary) = boundaries.last() {
      if last_boundary < sentences.len() {
        let chunk_text = sentences[last_boundary..].join(" ");

        if chunk_text.len() > self.chunk_size {
          let sub_chunks = self.split_large_chunk(&chunk_text, char_offset);
          chunks.extend(sub_chunks);
        } else {
          let start_idx = char_offset;
          let end_idx = start_idx + chunk_text.len();
          chunks.push(TextChunk {
            content: chunk_text,
            start_idx,
            end_idx,
            metadata: std::collections::HashMap::new(),
            chunk_index: chunks.len(),
            total_chunks: 0,
          });
        }
      }
    }

    // Update total_chunks
    let total = chunks.len();
    for chunk in &mut chunks {
      chunk.total_chunks = total;
    }

    Ok(chunks)
  }

  /// Split a large chunk into smaller pieces
  fn split_large_chunk(&self, text: &str, start_offset: usize) -> Vec<TextChunk> {
    let mut chunks = Vec::new();
    let mut current_offset = start_offset;

    for chunk_text in text
      .chars()
      .collect::<Vec<_>>()
      .chunks(self.chunk_size)
      .map(|c| c.iter().collect::<String>())
    {
      let start_idx = current_offset;
      let end_idx = start_idx + chunk_text.len();
      chunks.push(TextChunk {
        content: chunk_text.clone(),
        start_idx,
        end_idx,
        metadata: std::collections::HashMap::new(),
        chunk_index: chunks.len(),
        total_chunks: 0, // Will be updated by caller
      });
      current_offset += chunk_text.len();
    }

    chunks
  }

  /// Apply overlap between chunks for context preservation
  fn apply_overlap(&self, chunks: Vec<TextChunk>) -> Vec<TextChunk> {
    if self.overlap == 0 || chunks.len() <= 1 {
      return chunks;
    }

    let mut result = Vec::with_capacity(chunks.len());

    for (i, chunk) in chunks.iter().enumerate() {
      if i == 0 {
        // First chunk - no prefix overlap
        result.push(chunk.clone());
      } else {
        // Add overlap from previous chunk
        let prev_chunk = &chunks[i - 1];
        let overlap_text = prev_chunk
          .content
          .chars()
          .rev()
          .take(self.overlap)
          .collect::<Vec<_>>()
          .into_iter()
          .rev()
          .collect::<String>();

        let new_content = format!("{} {}", overlap_text, chunk.content);
        let new_end_idx = chunk.start_idx + new_content.len();

        result.push(TextChunk {
          content: new_content,
          start_idx: chunk.start_idx,
          end_idx: new_end_idx,
          metadata: chunk.metadata.clone(),
          chunk_index: chunk.chunk_index,
          total_chunks: chunk.total_chunks,
        });
      }
    }

    result
  }
}

impl ChunkingStrategy for SemanticChunker {
  fn chunk(&self, _text: &str) -> Result<Vec<TextChunk>> {
    // Semantic chunking requires async operations, so we can't implement
    // the sync trait directly. Users should use chunk_async() instead.
    Err(RAGError::chunking(
      "Semantic chunking requires async context. Use chunk_async() instead.",
    ))
  }

  fn chunk_size(&self) -> usize {
    self.chunk_size
  }

  fn overlap(&self) -> usize {
    self.overlap
  }

  fn strategy_name(&self) -> &str {
    "semantic"
  }
}

/// Builder for SemanticChunker
#[derive(Default)]
pub struct SemanticChunkerBuilder {
  embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
  chunk_size: Option<usize>,
  overlap: Option<usize>,
  similarity_threshold: Option<f32>,
  min_segment_size: Option<usize>,
  buffer_percentile: Option<f32>,
}

impl SemanticChunkerBuilder {
  /// Set the embedding provider
  pub fn with_embedding_provider<E: EmbeddingProvider + 'static>(mut self, provider: E) -> Self {
    self.embedding_provider = Some(Arc::new(provider));
    self
  }

  /// Set the embedding provider from Arc
  pub fn with_embedding_provider_arc(mut self, provider: Arc<dyn EmbeddingProvider>) -> Self {
    self.embedding_provider = Some(provider);
    self
  }

  /// Set the target chunk size in characters
  pub fn with_chunk_size(mut self, size: usize) -> Self {
    self.chunk_size = Some(size);
    self
  }

  /// Set the overlap between chunks in characters
  pub fn with_overlap(mut self, overlap: usize) -> Self {
    self.overlap = Some(overlap);
    self
  }

  /// Set the similarity threshold for topic boundaries (0.0-1.0)
  pub fn with_similarity_threshold(mut self, threshold: f32) -> Self {
    self.similarity_threshold = Some(threshold.clamp(0.0, 1.0));
    self
  }

  /// Set the minimum segment size in characters
  pub fn with_min_segment_size(mut self, size: usize) -> Self {
    self.min_segment_size = Some(size);
    self
  }

  /// Set the buffer percentile for dynamic threshold calculation
  pub fn with_buffer_percentile(mut self, percentile: f32) -> Self {
    self.buffer_percentile = Some(percentile.clamp(0.0, 1.0));
    self
  }

  /// Build the semantic chunker
  pub fn build(self) -> Result<SemanticChunker> {
    let embedding_provider = self.embedding_provider.ok_or_else(|| {
      RAGError::configuration("Embedding provider is required for semantic chunking")
    })?;

    Ok(SemanticChunker {
      embedding_provider,
      chunk_size: self.chunk_size.unwrap_or(512),
      overlap: self.overlap.unwrap_or(50),
      similarity_threshold: self.similarity_threshold.unwrap_or(0.6),
      min_segment_size: self.min_segment_size.unwrap_or(20),
      buffer_percentile: self.buffer_percentile.unwrap_or(0.25),
    })
  }
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
  if a.len() != b.len() {
    return 0.0;
  }

  let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
  let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
  let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

  if norm_a == 0.0 || norm_b == 0.0 {
    return 0.0;
  }

  dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_cosine_similarity() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    assert!(cosine_similarity(&a, &b).abs() < 0.001);

    let a = vec![1.0, 1.0];
    let b = vec![1.0, 1.0];
    assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
  }

  #[test]
  fn test_split_into_sentences() {
    let embedding = Arc::new(
      crate::embeddings::openai::OpenAIEmbedding::builder("text-embedding-3-small")
        .api_key("test")
        .build()
        .unwrap(),
    );

    let chunker = SemanticChunker {
      embedding_provider: embedding,
      chunk_size: 512,
      overlap: 50,
      similarity_threshold: 0.6,
      min_segment_size: 10,
      buffer_percentile: 0.25,
    };

    let text = "First sentence. Second sentence! Third sentence?";
    let sentences = chunker.split_into_sentences(text);
    assert_eq!(sentences.len(), 3);
  }

  #[test]
  fn test_detect_boundaries() {
    let embedding = Arc::new(
      crate::embeddings::openai::OpenAIEmbedding::builder("text-embedding-3-small")
        .api_key("test")
        .build()
        .unwrap(),
    );

    let chunker = SemanticChunker {
      embedding_provider: embedding,
      chunk_size: 512,
      overlap: 50,
      similarity_threshold: 0.6,
      min_segment_size: 10,
      buffer_percentile: 0.0, // Disable dynamic threshold for test
    };

    let similarities = vec![0.8, 0.9, 0.4, 0.85, 0.5];
    let boundaries = chunker.detect_boundaries(&similarities);

    // Should have boundaries at index 0 (start), and after low similarities
    assert!(boundaries.contains(&0));
    assert!(boundaries.contains(&3)); // After 0.4
    assert!(boundaries.contains(&5)); // After 0.5
  }
}
