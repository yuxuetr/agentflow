//! Re-ranking strategies for search results

use crate::{error::Result, types::SearchResult};

/// Re-ranking strategy trait
pub trait ReRankingStrategy: Send + Sync {
  /// Re-rank search results
  fn rerank(&self, query: &str, results: Vec<SearchResult>) -> Result<Vec<SearchResult>>;
}

/// No-op re-ranking (keeps original order)
pub struct NoReRanking;

impl ReRankingStrategy for NoReRanking {
  fn rerank(&self, _query: &str, results: Vec<SearchResult>) -> Result<Vec<SearchResult>> {
    Ok(results)
  }
}

/// MMR (Maximal Marginal Relevance) re-ranking
pub struct MMRReRanking {
  lambda: f32, // Diversity parameter (0 = pure diversity, 1 = pure relevance)
}

impl MMRReRanking {
  pub fn new(lambda: f32) -> Self {
    Self { lambda: lambda.clamp(0.0, 1.0) }
  }
}

impl ReRankingStrategy for MMRReRanking {
  fn rerank(&self, _query: &str, mut results: Vec<SearchResult>) -> Result<Vec<SearchResult>> {
    if results.len() <= 1 {
      return Ok(results);
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // TODO: Implement full MMR algorithm with diversity calculation
    // For now, just return sorted results
    Ok(results)
  }
}
