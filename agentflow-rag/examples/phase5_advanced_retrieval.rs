//! Phase 5: Advanced Retrieval - BM25, Hybrid Search, and MMR Re-ranking
//!
//! This example demonstrates Phase 5 features:
//! 1. BM25 keyword search
//! 2. Hybrid search with RRF fusion (semantic + keyword)
//! 3. MMR re-ranking for diversity
//!
//! # Usage
//! ```bash
//! cargo run --example phase5_advanced_retrieval
//! ```

use agentflow_rag::{
  reranking::{MMRReRanking, ReRankingStrategy, ScoreReRanking},
  retrieval::{bm25::BM25Retriever, hybrid::HybridRetriever},
  types::SearchResult,
};
use std::collections::HashMap;

fn main() {
  println!("=== Phase 5: Advanced Retrieval Strategies ===\n");

  // Sample documents about AI/ML topics
  let documents = vec![
    (
      "doc1",
      "Machine learning is a subset of artificial intelligence focused on data-driven predictions",
    ),
    (
      "doc2",
      "Deep learning uses neural networks with multiple layers for complex pattern recognition",
    ),
    (
      "doc3",
      "Machine learning algorithms can be supervised, unsupervised, or reinforcement-based",
    ),
    (
      "doc4",
      "Neural networks are inspired by biological neurons and process information in layers",
    ),
    (
      "doc5",
      "Artificial intelligence encompasses machine learning, deep learning, and expert systems",
    ),
    (
      "doc6",
      "Machine learning applications include recommendation systems and predictive analytics",
    ),
    (
      "doc7",
      "Deep learning has revolutionized computer vision and natural language processing",
    ),
  ];

  // Demo 1: BM25 Keyword Search
  println!("1️⃣  BM25 Keyword Search");
  println!("{}", "=".repeat(60));
  demo_bm25_search(&documents);
  println!();

  // Demo 2: Hybrid Search with RRF
  println!("2️⃣  Hybrid Search (Semantic + Keyword with RRF)");
  println!("{}", "=".repeat(60));
  demo_hybrid_search(&documents);
  println!();

  // Demo 3: MMR Re-ranking for Diversity
  println!("3️⃣  MMR Re-ranking (Relevance + Diversity)");
  println!("{}", "=".repeat(60));
  demo_mmr_reranking(&documents);
  println!();

  println!("✨ Phase 5 demonstration complete!\n");
  println!("=== Phase 5 Features Demonstrated ===");
  println!("✅ BM25 keyword search with TF-IDF scoring");
  println!("✅ Hybrid search combining semantic and keyword results");
  println!("✅ RRF (Reciprocal Rank Fusion) for score fusion");
  println!("✅ MMR re-ranking for diversity in results");
  println!("✅ Configurable alpha parameter for semantic/keyword balance");
  println!("✅ Configurable lambda parameter for relevance/diversity tradeoff");
}

fn demo_bm25_search(documents: &[(&str, &str)]) {
  let mut retriever = BM25Retriever::new();

  // Index documents
  println!("📚 Indexing {} documents...", documents.len());
  for (id, content) in documents {
    retriever.add_document(*id, *content);
  }
  println!("✅ Indexed {} documents\n", retriever.num_documents());

  // Perform searches with different queries
  let queries = vec![
    "machine learning algorithms",
    "deep learning neural networks",
    "artificial intelligence",
  ];

  for query in queries {
    println!("🔍 Query: \"{}\"", query);
    let results = retriever.search(query, 3);

    println!("   📋 Top {} BM25 results:", results.len());
    for (i, result) in results.iter().enumerate() {
      println!(
        "      {}. [Score: {:.4}] {}",
        i + 1,
        result.score,
        truncate(&result.content, 60)
      );
    }
    println!();
  }
}

fn demo_hybrid_search(documents: &[(&str, &str)]) {
  let mut hybrid = HybridRetriever::new();

  // Index documents for keyword search
  println!("📚 Indexing documents for hybrid search...");
  for (id, content) in documents {
    hybrid.add_document(*id, *content);
  }
  println!("✅ Indexed {} documents\n", hybrid.num_documents());

  // Simulate semantic search results (in real usage, these come from vector store)
  let semantic_results = simulate_semantic_results(documents);

  let query = "machine learning applications";
  println!("🔍 Query: \"{}\"", query);
  println!();

  // Compare different alpha values
  let alphas = vec![
    (1.0, "Pure Semantic"),
    (0.8, "Semantic-Focused"),
    (0.5, "Balanced"),
    (0.2, "Keyword-Focused"),
    (0.0, "Pure Keyword"),
  ];

  for (alpha, description) in alphas {
    println!("   📊 {} (α={}):", description, alpha);
    let results = hybrid.search(semantic_results.clone(), query, 3, alpha);

    for (i, result) in results.iter().enumerate() {
      println!(
        "      {}. [Score: {:.4}] {}",
        i + 1,
        result.score,
        truncate(&result.content, 55)
      );
    }
    println!();
  }
}

fn demo_mmr_reranking(_documents: &[(&str, &str)]) {
  // Create test results with redundant content
  let results = vec![
    create_result("doc1", "Machine learning is a subset of AI", 0.95),
    create_result("doc2", "Machine learning is used in AI systems", 0.92), // Similar to doc1
    create_result("doc3", "Deep learning is a type of machine learning", 0.90),
    create_result(
      "doc4",
      "Machine learning enables predictive analytics",
      0.88,
    ), // Similar to doc1
    create_result("doc5", "Neural networks power deep learning", 0.85),
  ];

  println!("📋 Original results (by relevance score):");
  for (i, result) in results.iter().enumerate() {
    println!(
      "   {}. [Score: {:.2}] {}",
      i + 1,
      result.score,
      result.content
    );
  }
  println!();

  // Apply MMR with different lambda values
  let lambdas = vec![
    (1.0, "Pure Relevance"),
    (0.7, "Mostly Relevant"),
    (0.5, "Balanced"),
    (0.3, "Mostly Diverse"),
    (0.0, "Pure Diversity"),
  ];

  for (lambda, description) in lambdas {
    println!("   🔄 {} (λ={}):", description, lambda);
    let mmr = MMRReRanking::new(lambda);
    let reranked = mmr.rerank("machine learning", results.clone()).unwrap();

    for (i, result) in reranked.iter().enumerate() {
      println!("      {}. {}", i + 1, truncate(&result.content, 50));
    }
    println!();
  }

  // Also demonstrate score re-ranking
  println!("   📊 Score Re-ranking (descending):");
  let score_reranker = ScoreReRanking::descending();
  let score_reranked = score_reranker.rerank("query", results.clone()).unwrap();
  for (i, result) in score_reranked.iter().enumerate() {
    println!(
      "      {}. [Score: {:.2}] {}",
      i + 1,
      result.score,
      truncate(&result.content, 45)
    );
  }
}

// Helper function to simulate semantic search results
fn simulate_semantic_results(documents: &[(&str, &str)]) -> Vec<SearchResult> {
  // Simulate vector similarity scores (in real usage, these come from vector DB)
  let scores = vec![0.92, 0.88, 0.95, 0.75, 0.90, 0.85, 0.70];

  documents
    .iter()
    .zip(scores.iter())
    .map(|((id, content), score)| SearchResult {
      id: id.to_string(),
      content: content.to_string(),
      score: *score,
      metadata: HashMap::new(),
    })
    .collect()
}

fn create_result(id: &str, content: &str, score: f32) -> SearchResult {
  SearchResult {
    id: id.to_string(),
    content: content.to_string(),
    score,
    metadata: HashMap::new(),
  }
}

fn truncate(s: &str, max_len: usize) -> String {
  if s.len() <= max_len {
    s.to_string()
  } else {
    format!("{}...", &s[..max_len - 3])
  }
}
