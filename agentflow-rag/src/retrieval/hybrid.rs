//! Hybrid search combining semantic and keyword-based retrieval
//!
//! Hybrid search uses Reciprocal Rank Fusion (RRF) to combine results from
//! semantic (vector) search and keyword (BM25) search.

use crate::{retrieval::bm25::BM25Retriever, types::SearchResult};
use std::collections::HashMap;

/// Hybrid search combining semantic and keyword search with RRF
///
/// # Algorithm: Reciprocal Rank Fusion (RRF)
/// ```text
/// RRFScore(d) = Σ 1 / (k + rank_i(d))
/// ```
///
/// Where:
/// - d = document
/// - k = constant (typically 60)
/// - rank_i(d) = rank of document d in ranking system i
///
/// # Benefits
/// - Combines semantic understanding with exact keyword matching
/// - Robust to different scoring scales
/// - Simple but effective fusion method
/// - No need to normalize scores
///
/// # Example
/// ```ignore
/// use agentflow_rag::retrieval::hybrid::HybridRetriever;
///
/// let mut hybrid = HybridRetriever::new();
/// hybrid.add_document("doc1", "machine learning algorithms");
///
/// let results = hybrid.search(
///     semantic_results,  // From vector search
///     "machine learning",
///     10,
///     0.5  // Equal weight for semantic and keyword
/// );
/// ```
pub struct HybridRetriever {
  /// BM25 retriever for keyword search
  bm25: BM25Retriever,

  /// RRF constant (typically 60)
  rrf_k: f32,
}

impl HybridRetriever {
  /// Create a new hybrid retriever with default RRF constant (k=60)
  pub fn new() -> Self {
    Self {
      bm25: BM25Retriever::new(),
      rrf_k: 60.0,
    }
  }

  /// Create a new hybrid retriever with custom RRF constant
  ///
  /// # Arguments
  /// * `rrf_k` - RRF constant (typically 60, higher values reduce rank influence)
  pub fn with_rrf_k(rrf_k: f32) -> Self {
    Self {
      bm25: BM25Retriever::new(),
      rrf_k,
    }
  }

  /// Create with custom BM25 parameters and RRF constant
  ///
  /// # Arguments
  /// * `bm25_k1` - BM25 term frequency saturation (typically 1.2)
  /// * `bm25_b` - BM25 length normalization (typically 0.75)
  /// * `rrf_k` - RRF constant (typically 60)
  pub fn with_params(bm25_k1: f32, bm25_b: f32, rrf_k: f32) -> Self {
    Self {
      bm25: BM25Retriever::with_params(bm25_k1, bm25_b),
      rrf_k,
    }
  }

  /// Add a document to the keyword index
  pub fn add_document(&mut self, id: impl Into<String>, content: impl Into<String>) {
    self.bm25.add_document(id, content);
  }

  /// Add a document with metadata
  pub fn add_document_with_metadata(
    &mut self,
    id: impl Into<String>,
    content: impl Into<String>,
    metadata: HashMap<String, crate::types::MetadataValue>,
  ) {
    self.bm25.add_document_with_metadata(id, content, metadata);
  }

  /// Remove a document from the index
  pub fn remove_document(&mut self, id: &str) -> bool {
    self.bm25.remove_document(id)
  }

  /// Clear all documents from the index
  pub fn clear(&mut self) {
    self.bm25.clear();
  }

  /// Get number of indexed documents
  pub fn num_documents(&self) -> usize {
    self.bm25.num_documents()
  }

  /// Perform hybrid search with RRF fusion
  ///
  /// # Arguments
  /// * `semantic_results` - Results from semantic (vector) search
  /// * `query` - Search query text
  /// * `top_k` - Maximum number of results to return
  /// * `alpha` - Weight parameter (0.0 = pure keyword, 1.0 = pure semantic)
  ///
  /// # Returns
  /// Fused and re-ranked search results
  ///
  /// # Algorithm
  /// 1. Perform BM25 keyword search
  /// 2. Calculate RRF scores for all documents across both rankings
  /// 3. Apply alpha weighting: final_score = alpha * semantic_score + (1-alpha) * keyword_score
  /// 4. Sort by combined score and return top-k
  pub fn search(
    &self,
    semantic_results: Vec<SearchResult>,
    query: &str,
    top_k: usize,
    alpha: f32,
  ) -> Vec<SearchResult> {
    let alpha = alpha.clamp(0.0, 1.0);

    // Perform BM25 keyword search
    let keyword_results = self.bm25.search(query, top_k * 2); // Get more candidates

    // Calculate RRF scores for semantic results
    let semantic_rrf = self.calculate_rrf_scores(&semantic_results);

    // Calculate RRF scores for keyword results
    let keyword_rrf = self.calculate_rrf_scores(&keyword_results);

    // Combine scores with alpha weighting
    let mut combined_scores: HashMap<String, (f32, SearchResult)> = HashMap::new();

    // Add semantic results
    for result in semantic_results {
      let semantic_score = semantic_rrf.get(&result.id).copied().unwrap_or(0.0);
      let combined_score = alpha * semantic_score;

      combined_scores.insert(result.id.clone(), (combined_score, result));
    }

    // Add/update with keyword results
    for result in keyword_results {
      let keyword_score = keyword_rrf.get(&result.id).copied().unwrap_or(0.0);
      let keyword_contribution = (1.0 - alpha) * keyword_score;

      combined_scores
        .entry(result.id.clone())
        .and_modify(|(score, _)| *score += keyword_contribution)
        .or_insert((keyword_contribution, result));
    }

    // Sort by combined score and take top-k
    let mut results: Vec<(f32, SearchResult)> = combined_scores.into_values().collect();

    results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    results
      .into_iter()
      .take(top_k)
      .map(|(score, mut result)| {
        result.score = score; // Update with combined score
        result
      })
      .collect()
  }

  /// Calculate RRF scores for a list of search results
  ///
  /// RRF Score = 1 / (k + rank)
  fn calculate_rrf_scores(&self, results: &[SearchResult]) -> HashMap<String, f32> {
    results
      .iter()
      .enumerate()
      .map(|(rank, result)| {
        let rrf_score = 1.0 / (self.rrf_k + rank as f32 + 1.0); // rank is 0-indexed
        (result.id.clone(), rrf_score)
      })
      .collect()
  }

  /// Simplified search that automatically balances semantic and keyword (alpha=0.5)
  pub fn balanced_search(
    &self,
    semantic_results: Vec<SearchResult>,
    query: &str,
    top_k: usize,
  ) -> Vec<SearchResult> {
    self.search(semantic_results, query, top_k, 0.5)
  }

  /// Semantic-focused search (alpha=0.8)
  pub fn semantic_focused_search(
    &self,
    semantic_results: Vec<SearchResult>,
    query: &str,
    top_k: usize,
  ) -> Vec<SearchResult> {
    self.search(semantic_results, query, top_k, 0.8)
  }

  /// Keyword-focused search (alpha=0.2)
  pub fn keyword_focused_search(
    &self,
    semantic_results: Vec<SearchResult>,
    query: &str,
    top_k: usize,
  ) -> Vec<SearchResult> {
    self.search(semantic_results, query, top_k, 0.2)
  }
}

impl Default for HybridRetriever {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  fn create_result(id: &str, content: &str, score: f32) -> SearchResult {
    SearchResult {
      id: id.to_string(),
      content: content.to_string(),
      score,
      metadata: HashMap::new(),
    }
  }

  #[test]
  fn test_rrf_score_calculation() {
    let hybrid = HybridRetriever::new();
    let results = vec![
      create_result("doc1", "content", 0.9),
      create_result("doc2", "other", 0.8),
      create_result("doc3", "another", 0.7),
    ];

    let rrf_scores = hybrid.calculate_rrf_scores(&results);

    // First result (rank 0): 1 / (60 + 0 + 1) ≈ 0.0164
    assert!((rrf_scores["doc1"] - 1.0 / 61.0).abs() < 0.001);

    // Second result (rank 1): 1 / (60 + 1 + 1) ≈ 0.0161
    assert!((rrf_scores["doc2"] - 1.0 / 62.0).abs() < 0.001);

    // Third result (rank 2): 1 / (60 + 2 + 1) ≈ 0.0159
    assert!((rrf_scores["doc3"] - 1.0 / 63.0).abs() < 0.001);
  }

  #[test]
  fn test_hybrid_search_pure_semantic() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "machine learning algorithms");
    hybrid.add_document("doc2", "deep neural networks");

    let semantic_results = vec![
      create_result("doc1", "machine learning algorithms", 0.95),
      create_result("doc2", "deep neural networks", 0.85),
    ];

    // Alpha = 1.0 means pure semantic (keyword search ignored)
    let results = hybrid.search(semantic_results.clone(), "machine learning", 10, 1.0);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "doc1"); // Higher semantic score
  }

  #[test]
  fn test_hybrid_search_pure_keyword() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "machine learning is fascinating");
    hybrid.add_document("doc2", "deep learning and machine learning");

    let semantic_results = vec![
      create_result("doc1", "machine learning is fascinating", 0.7),
      create_result("doc2", "deep learning and machine learning", 0.9),
    ];

    // Alpha = 0.0 means pure keyword (semantic search ignored)
    let results = hybrid.search(semantic_results, "machine learning", 10, 0.0);

    assert!(!results.is_empty());
    // doc2 contains "machine learning" twice, should rank higher in BM25
    assert_eq!(results[0].id, "doc2");
  }

  #[test]
  fn test_hybrid_search_balanced() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "artificial intelligence and machine learning");
    hybrid.add_document("doc2", "machine learning machine learning");
    hybrid.add_document("doc3", "deep learning systems");

    let semantic_results = vec![
      create_result("doc1", "artificial intelligence and machine learning", 0.95),
      create_result("doc3", "deep learning systems", 0.90),
    ];

    // Balanced search (alpha = 0.5)
    let results = hybrid.balanced_search(semantic_results, "machine learning", 10);

    assert!(!results.is_empty());
    // Results should combine both semantic similarity and keyword matches
  }

  #[test]
  fn test_empty_semantic_results() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "machine learning");

    let results = hybrid.search(vec![], "machine learning", 10, 0.5);

    // Should still return keyword results
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
  }

  #[test]
  fn test_empty_keyword_results() {
    let hybrid = HybridRetriever::new(); // No documents indexed

    let semantic_results = vec![create_result("doc1", "content", 0.9)];

    let results = hybrid.search(semantic_results.clone(), "query", 10, 0.5);

    // Should still return semantic results
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
  }

  #[test]
  fn test_alpha_clamping() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "test");

    let semantic_results = vec![create_result("doc1", "test", 0.9)];

    // Alpha > 1.0 should be clamped to 1.0
    let results1 = hybrid.search(semantic_results.clone(), "test", 10, 1.5);
    assert!(!results1.is_empty());

    // Alpha < 0.0 should be clamped to 0.0
    let results2 = hybrid.search(semantic_results, "test", 10, -0.5);
    assert!(!results2.is_empty());
  }

  #[test]
  fn test_custom_rrf_k() {
    let hybrid1 = HybridRetriever::with_rrf_k(30.0);
    assert_eq!(hybrid1.rrf_k, 30.0);

    let hybrid2 = HybridRetriever::with_rrf_k(100.0);
    assert_eq!(hybrid2.rrf_k, 100.0);
  }

  #[test]
  fn test_convenience_methods() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "test content");

    let semantic_results = vec![create_result("doc1", "test content", 0.9)];

    let balanced = hybrid.balanced_search(semantic_results.clone(), "test", 10);
    assert!(!balanced.is_empty());

    let semantic_focused = hybrid.semantic_focused_search(semantic_results.clone(), "test", 10);
    assert!(!semantic_focused.is_empty());

    let keyword_focused = hybrid.keyword_focused_search(semantic_results, "test", 10);
    assert!(!keyword_focused.is_empty());
  }

  #[test]
  fn test_document_management() {
    let mut hybrid = HybridRetriever::new();

    assert_eq!(hybrid.num_documents(), 0);

    hybrid.add_document("doc1", "content");
    assert_eq!(hybrid.num_documents(), 1);

    hybrid.add_document("doc2", "other");
    assert_eq!(hybrid.num_documents(), 2);

    assert!(hybrid.remove_document("doc1"));
    assert_eq!(hybrid.num_documents(), 1);

    hybrid.clear();
    assert_eq!(hybrid.num_documents(), 0);
  }

  #[test]
  fn test_top_k_limit() {
    let mut hybrid = HybridRetriever::new();
    hybrid.add_document("doc1", "test");
    hybrid.add_document("doc2", "test");
    hybrid.add_document("doc3", "test");
    hybrid.add_document("doc4", "test");

    let semantic_results = vec![
      create_result("doc1", "test", 0.9),
      create_result("doc2", "test", 0.8),
      create_result("doc3", "test", 0.7),
      create_result("doc4", "test", 0.6),
    ];

    let results = hybrid.search(semantic_results, "test", 2, 0.5);
    assert_eq!(results.len(), 2); // Should respect top_k limit
  }
}
