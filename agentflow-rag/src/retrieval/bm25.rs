//! BM25 (Best Matching 25) keyword search algorithm
//!
//! BM25 is a probabilistic ranking function used for keyword-based search.
//! It considers term frequency, inverse document frequency, and document length
//! normalization to rank documents.

use crate::types::SearchResult;
use std::collections::HashMap;

/// BM25 retriever for keyword-based search
///
/// # Algorithm
/// ```text
/// BM25(D,Q) = Σ IDF(qi) × (f(qi,D) × (k1 + 1)) / (f(qi,D) + k1 × (1 - b + b × |D| / avgdl))
/// ```
///
/// Where:
/// - D = document
/// - Q = query
/// - qi = query terms
/// - f(qi,D) = frequency of term qi in document D
/// - |D| = length of document D in tokens
/// - avgdl = average document length in collection
/// - k1 = term frequency saturation (default: 1.2)
/// - b = length normalization (default: 0.75)
/// - IDF(qi) = log((N - n(qi) + 0.5) / (n(qi) + 0.5))
///
/// # Example
/// ```rust,no_run
/// use agentflow_rag::retrieval::bm25::BM25Retriever;
/// use agentflow_rag::types::SearchResult;
///
/// let mut retriever = BM25Retriever::new();
/// retriever.add_document("doc1", "the quick brown fox");
/// retriever.add_document("doc2", "the lazy brown dog");
///
/// let results = retriever.search("brown fox", 10);
/// ```
pub struct BM25Retriever {
  /// Documents indexed by ID
  documents: HashMap<String, DocumentIndex>,

  /// Inverse document frequency for each term
  idf: HashMap<String, f32>,

  /// Total number of documents
  num_docs: usize,

  /// Average document length
  avg_doc_length: f32,

  /// Term frequency saturation parameter (typically 1.2-2.0)
  k1: f32,

  /// Length normalization parameter (typically 0.75)
  b: f32,
}

/// Document index containing term frequencies and metadata
#[derive(Debug, Clone)]
struct DocumentIndex {
  /// Document ID
  id: String,

  /// Original content
  content: String,

  /// Term frequencies (term -> count)
  term_freq: HashMap<String, usize>,

  /// Document length in tokens
  doc_length: usize,

  /// Document metadata
  metadata: HashMap<String, crate::types::MetadataValue>,
}

impl BM25Retriever {
  /// Create a new BM25 retriever with default parameters
  pub fn new() -> Self {
    Self {
      documents: HashMap::new(),
      idf: HashMap::new(),
      num_docs: 0,
      avg_doc_length: 0.0,
      k1: 1.2,
      b: 0.75,
    }
  }

  /// Create a new BM25 retriever with custom parameters
  ///
  /// # Arguments
  /// * `k1` - Term frequency saturation (typically 1.2-2.0)
  /// * `b` - Length normalization (typically 0.75)
  pub fn with_params(k1: f32, b: f32) -> Self {
    Self {
      documents: HashMap::new(),
      idf: HashMap::new(),
      num_docs: 0,
      avg_doc_length: 0.0,
      k1,
      b: b.clamp(0.0, 1.0),
    }
  }

  /// Tokenize text into lowercase terms
  fn tokenize(text: &str) -> Vec<String> {
    text
      .to_lowercase()
      .split_whitespace()
      .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()))
      .filter(|s| !s.is_empty())
      .map(|s| s.to_string())
      .collect()
  }

  /// Compute term frequencies for a document
  fn compute_term_freq(tokens: &[String]) -> HashMap<String, usize> {
    let mut freq = HashMap::new();
    for token in tokens {
      *freq.entry(token.clone()).or_insert(0) += 1;
    }
    freq
  }

  /// Add a document to the index
  ///
  /// # Arguments
  /// * `id` - Unique document ID
  /// * `content` - Document text content
  pub fn add_document(&mut self, id: impl Into<String>, content: impl Into<String>) {
    self.add_document_with_metadata(id, content, HashMap::new());
  }

  /// Add a document with metadata to the index
  pub fn add_document_with_metadata(
    &mut self,
    id: impl Into<String>,
    content: impl Into<String>,
    metadata: HashMap<String, crate::types::MetadataValue>,
  ) {
    let id = id.into();
    let content = content.into();
    let tokens = Self::tokenize(&content);
    let term_freq = Self::compute_term_freq(&tokens);
    let doc_length = tokens.len();

    let doc_index = DocumentIndex {
      id: id.clone(),
      content,
      term_freq,
      doc_length,
      metadata,
    };

    self.documents.insert(id, doc_index);
    self.num_docs = self.documents.len();

    // Recompute IDF and average document length
    self.recompute_statistics();
  }

  /// Remove a document from the index
  pub fn remove_document(&mut self, id: &str) -> bool {
    let removed = self.documents.remove(id).is_some();
    if removed {
      self.num_docs = self.documents.len();
      self.recompute_statistics();
    }
    removed
  }

  /// Recompute IDF values and average document length
  fn recompute_statistics(&mut self) {
    if self.documents.is_empty() {
      self.idf.clear();
      self.avg_doc_length = 0.0;
      return;
    }

    // Compute average document length
    let total_length: usize = self.documents.values().map(|doc| doc.doc_length).sum();
    self.avg_doc_length = total_length as f32 / self.num_docs as f32;

    // Compute document frequency for each term
    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    for doc in self.documents.values() {
      for term in doc.term_freq.keys() {
        *doc_freq.entry(term.clone()).or_insert(0) += 1;
      }
    }

    // Compute IDF for each term
    self.idf.clear();
    for (term, df) in doc_freq {
      let idf = ((self.num_docs as f32 - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0).ln();
      self.idf.insert(term, idf);
    }
  }

  /// Calculate BM25 score for a document given query terms
  fn calculate_score(&self, doc: &DocumentIndex, query_terms: &[String]) -> f32 {
    let mut score = 0.0;

    for term in query_terms {
      // Get IDF for term (0.0 if term not in corpus)
      let idf = self.idf.get(term).copied().unwrap_or(0.0);

      // Get term frequency in document
      let tf = doc.term_freq.get(term).copied().unwrap_or(0) as f32;

      if tf > 0.0 {
        // Calculate BM25 component for this term
        let numerator = tf * (self.k1 + 1.0);
        let denominator =
          tf + self.k1 * (1.0 - self.b + self.b * doc.doc_length as f32 / self.avg_doc_length);

        score += idf * (numerator / denominator);
      }
    }

    score
  }

  /// Search for documents matching the query
  ///
  /// # Arguments
  /// * `query` - Search query text
  /// * `top_k` - Maximum number of results to return
  ///
  /// # Returns
  /// Vector of search results sorted by BM25 score (descending)
  pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
    if self.documents.is_empty() {
      return Vec::new();
    }

    let query_terms = Self::tokenize(query);
    if query_terms.is_empty() {
      return Vec::new();
    }

    // Calculate scores for all documents
    let mut scores: Vec<(String, f32)> = self
      .documents
      .values()
      .map(|doc| {
        let score = self.calculate_score(doc, &query_terms);
        (doc.id.clone(), score)
      })
      .filter(|(_, score)| *score > 0.0) // Only return documents with non-zero score
      .collect();

    // Sort by score descending
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top k and convert to SearchResult
    scores
      .into_iter()
      .take(top_k)
      .filter_map(|(id, score)| {
        self.documents.get(&id).map(|doc| SearchResult {
          id: doc.id.clone(),
          content: doc.content.clone(),
          score,
          metadata: doc.metadata.clone(),
        })
      })
      .collect()
  }

  /// Get the number of indexed documents
  pub fn num_documents(&self) -> usize {
    self.num_docs
  }

  /// Clear all documents from the index
  pub fn clear(&mut self) {
    self.documents.clear();
    self.idf.clear();
    self.num_docs = 0;
    self.avg_doc_length = 0.0;
  }
}

impl Default for BM25Retriever {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_tokenize() {
    let tokens = BM25Retriever::tokenize("The quick, brown fox!");
    assert_eq!(tokens, vec!["the", "quick", "brown", "fox"]);

    let tokens2 = BM25Retriever::tokenize("  spaces   everywhere  ");
    assert_eq!(tokens2, vec!["spaces", "everywhere"]);

    let tokens3 = BM25Retriever::tokenize("");
    assert_eq!(tokens3.len(), 0);
  }

  #[test]
  fn test_add_document() {
    let mut retriever = BM25Retriever::new();
    assert_eq!(retriever.num_documents(), 0);

    retriever.add_document("doc1", "the quick brown fox");
    assert_eq!(retriever.num_documents(), 1);

    retriever.add_document("doc2", "the lazy dog");
    assert_eq!(retriever.num_documents(), 2);
  }

  #[test]
  fn test_remove_document() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "content");
    retriever.add_document("doc2", "other");

    assert!(retriever.remove_document("doc1"));
    assert_eq!(retriever.num_documents(), 1);

    assert!(!retriever.remove_document("nonexistent"));
    assert_eq!(retriever.num_documents(), 1);
  }

  #[test]
  fn test_simple_search() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "the quick brown fox jumps over the lazy dog");
    retriever.add_document("doc2", "the brown fox is quick and clever");
    retriever.add_document("doc3", "the lazy dog sleeps all day");

    let results = retriever.search("brown fox", 10);
    assert!(results.len() >= 2);

    // Both doc1 and doc2 contain "brown fox"
    // doc2 should score higher because it has shorter length
    assert_eq!(results[0].id, "doc2");
    assert!(results[0].score > 0.0);
  }

  #[test]
  fn test_search_with_limit() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "machine learning");
    retriever.add_document("doc2", "deep learning");
    retriever.add_document("doc3", "machine intelligence");

    let results = retriever.search("machine learning", 2);
    assert_eq!(results.len(), 2);
  }

  #[test]
  fn test_search_no_matches() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "machine learning algorithms");
    retriever.add_document("doc2", "deep neural networks");

    let results = retriever.search("quantum computing", 10);
    assert_eq!(results.len(), 0);
  }

  #[test]
  fn test_search_empty_query() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "content");

    let results = retriever.search("", 10);
    assert_eq!(results.len(), 0);
  }

  #[test]
  fn test_search_empty_index() {
    let retriever = BM25Retriever::new();
    let results = retriever.search("query", 10);
    assert_eq!(results.len(), 0);
  }

  #[test]
  fn test_clear() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "content");
    retriever.add_document("doc2", "other");

    retriever.clear();
    assert_eq!(retriever.num_documents(), 0);

    let results = retriever.search("content", 10);
    assert_eq!(results.len(), 0);
  }

  #[test]
  fn test_custom_parameters() {
    let retriever1 = BM25Retriever::with_params(2.0, 0.5);
    assert_eq!(retriever1.k1, 2.0);
    assert_eq!(retriever1.b, 0.5);

    // Test clamping of b parameter
    let retriever2 = BM25Retriever::with_params(1.5, 1.5);
    assert_eq!(retriever2.b, 1.0); // Should be clamped to 1.0
  }

  #[test]
  fn test_term_frequency() {
    let tokens = vec!["the".to_string(), "quick".to_string(), "the".to_string()];
    let tf = BM25Retriever::compute_term_freq(&tokens);

    assert_eq!(*tf.get("the").unwrap(), 2);
    assert_eq!(*tf.get("quick").unwrap(), 1);
  }

  #[test]
  fn test_idf_calculation() {
    let mut retriever = BM25Retriever::new();
    retriever.add_document("doc1", "the quick brown fox");
    retriever.add_document("doc2", "the lazy dog");
    retriever.add_document("doc3", "quick brown fox");

    // "the" appears in 2 out of 3 documents
    let idf_the = retriever.idf.get("the").copied().unwrap();
    assert!(idf_the > 0.0);

    // "fox" appears in 2 out of 3 documents
    let idf_fox = retriever.idf.get("fox").copied().unwrap();
    assert!(idf_fox > 0.0);

    // "lazy" appears in 1 out of 3 documents (more rare, higher IDF)
    let idf_lazy = retriever.idf.get("lazy").copied().unwrap();
    assert!(idf_lazy > idf_the);
  }

  #[test]
  fn test_document_length_normalization() {
    let mut retriever = BM25Retriever::new();

    // Short document with query term
    retriever.add_document("short", "machine learning");

    // Long document with same query term
    retriever.add_document(
      "long",
      "machine learning is a fascinating field of artificial intelligence that involves algorithms and statistical models",
    );

    let results = retriever.search("machine learning", 10);

    // Short document should score higher due to length normalization
    assert_eq!(results[0].id, "short");
    assert!(results[0].score > results[1].score);
  }
}
