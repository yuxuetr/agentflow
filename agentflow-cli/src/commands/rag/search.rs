use agentflow_rag::{
  embeddings::OpenAIEmbedding,
  retrieval::{bm25::BM25Retriever, hybrid::HybridRetriever},
  vectorstore::{QdrantStore, VectorStore},
};
use anyhow::{Context, Result};
use colored::*;
use serde_json::json;

/// Execute the RAG search command
pub async fn execute(
  qdrant_url: String,
  collection: String,
  query: String,
  top_k: usize,
  search_type: String,
  alpha: f32,
  rerank: bool,
  lambda: f32,
  _embedding_model: String,
  output: Option<String>,
) -> Result<()> {
  println!(
    "{}",
    format!("🔍 Searching collection '{}' for: \"{}\"", collection, query)
      .bold()
      .blue()
  );
  println!(
    "{}",
    format!("   Search type: {}, Top-K: {}", search_type, top_k).dimmed()
  );

  // Connect to Qdrant
  let store = QdrantStore::new(&qdrant_url)
    .await
    .context("Failed to connect to Qdrant")?;

  println!("{}", "✅ Connected to Qdrant".green());

  // Perform search based on type
  let results = match search_type.as_str() {
    "semantic" => {
      // Semantic search using embeddings
      store
        .similarity_search(&collection, &query, top_k, None)
        .await
        .context("Failed to perform semantic search")?
    }
    "hybrid" => {
      // Hybrid search: semantic + keyword (BM25)
      println!(
        "{}",
        format!("   Alpha: {} ({}% semantic, {}% keyword)", alpha, (alpha * 100.0) as i32, ((1.0 - alpha) * 100.0) as i32).dimmed()
      );

      // Get semantic results
      let semantic_results = store
        .similarity_search(&collection, &query, top_k * 2, None)
        .await
        .context("Failed to perform semantic search")?;

      // Initialize BM25 retriever (requires document loading)
      // Note: In practice, you'd have the BM25 index pre-built or loaded from storage
      let mut bm25 = BM25Retriever::new();

      // Add documents from semantic results for keyword search
      for result in &semantic_results {
        bm25.add_document(&result.id, &result.content);
      }

      // Perform hybrid search with RRF fusion
      let mut hybrid = HybridRetriever::new();
      for result in &semantic_results {
        hybrid.add_document(&result.id, &result.content);
      }

      hybrid.search(semantic_results, &query, top_k, alpha)
    }
    "keyword" => {
      // Keyword search using BM25
      // First get all documents (or a subset) to build BM25 index
      let all_docs = store
        .similarity_search(&collection, &query, top_k * 10, None)
        .await
        .context("Failed to fetch documents for keyword search")?;

      let mut bm25 = BM25Retriever::new();
      for doc in &all_docs {
        bm25.add_document(&doc.id, &doc.content);
      }

      bm25.search(&query, top_k)
    }
    _ => {
      anyhow::bail!("Invalid search type. Must be: semantic, hybrid, or keyword");
    }
  };

  // Apply MMR re-ranking if requested
  let final_results = if rerank {
    println!(
      "{}",
      format!("   Applying MMR re-ranking (λ={})", lambda).dimmed()
    );

    use agentflow_rag::reranking::{MMRReRanking, ReRankingStrategy};
    let mmr = MMRReRanking::new(lambda);
    mmr.rerank(&query, results)?
  } else {
    results
  };

  // Disconnect from Qdrant
  drop(store);

  // Display results
  if final_results.is_empty() {
    println!("{}", "⚠️  No results found".yellow());
    return Ok(());
  }

  println!();
  println!(
    "{}",
    format!("Search Results ({}):", final_results.len())
      .bold()
      .green()
  );
  println!();

  for (i, result) in final_results.iter().enumerate() {
    println!("{}", format!("{}. ", i + 1).bold());
    println!(
      "   {}: {}",
      "ID".cyan(),
      result.id
    );
    println!(
      "   {}: {:.4}",
      "Score".yellow(),
      result.score
    );

    // Display content (truncate if too long)
    let content = if result.content.len() > 200 {
      format!("{}...", &result.content[..200])
    } else {
      result.content.clone()
    };
    println!("   {}:", "Content".green());
    println!("   {}", content.dimmed());

    // Display metadata if present
    if !result.metadata.is_empty() {
      println!("   {}:", "Metadata".magenta());
      for (key, value) in &result.metadata {
        println!("     {}: {:?}", key.cyan(), value);
      }
    }

    println!();
  }

  // Save results to file if output specified
  if let Some(output_path) = output {
    let output_json = json!({
      "query": query,
      "search_type": search_type,
      "top_k": top_k,
      "results": final_results.iter().map(|r| {
        json!({
          "id": r.id,
          "content": r.content,
          "score": r.score,
          "metadata": r.metadata,
        })
      }).collect::<Vec<_>>()
    });

    std::fs::write(&output_path, serde_json::to_string_pretty(&output_json)?)
      .context(format!("Failed to write results to {}", output_path))?;

    println!(
      "{}",
      format!("✅ Results saved to: {}", output_path).green()
    );
  }

  println!(
    "{}",
    format!("Total: {} results", final_results.len())
      .bold()
      .green()
  );

  Ok(())
}
