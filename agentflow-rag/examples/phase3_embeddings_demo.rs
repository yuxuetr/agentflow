//! Phase 3: Embeddings Integration Demo
//!
//! This example demonstrates the new features added in Phase 3:
//! 1. OpenAI embedding provider with rate limiting and cost tracking
//! 2. Text-based similarity search
//! 3. Advanced filter support
//!
//! # Prerequisites
//! - Running Qdrant server: `docker run -p 6334:6334 qdrant/qdrant`
//! - OPENAI_API_KEY environment variable set
//!
//! # Usage
//! ```bash
//! export OPENAI_API_KEY=sk-...
//! cargo run --example phase3_embeddings_demo --features qdrant
//! ```

use agentflow_rag::{
  embeddings::{EmbeddingProvider, OpenAIEmbedding},
  types::{CollectionConfig, Condition, DistanceMetric, Document, Filter, MetadataValue},
  vectorstore::{QdrantStore, VectorStore},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize tracing for better logging
  tracing_subscriber::fmt()
    .with_env_filter("agentflow_rag=debug,phase3_embeddings_demo=info")
    .init();

  println!("=== Phase 3: Embeddings Integration Demo ===\n");

  // Step 1: Create OpenAI embedding provider with custom configuration
  println!("1️⃣  Creating OpenAI embedding provider...");
  let embedding_provider = OpenAIEmbedding::builder("text-embedding-3-small")
    .requests_per_minute(500) // Custom rate limit
    .timeout_secs(60) // 60 second timeout
    .build()?;

  println!("   ✅ Model: {}", embedding_provider.model_name());
  println!("   ✅ Dimension: {}", embedding_provider.dimension());
  println!("   ✅ Max tokens: {}\n", embedding_provider.max_tokens());

  // Step 2: Connect to Qdrant with embedding provider
  println!("2️⃣  Connecting to Qdrant with embedding provider...");
  let store = QdrantStore::with_embedding_provider(
    "http://localhost:6334",
    Arc::new(embedding_provider),
  )
  .await?;
  println!("   ✅ Connected to Qdrant\n");

  // Step 3: Create collection
  let collection_name = "phase3_demo";
  println!("3️⃣  Creating collection '{}'...", collection_name);

  // Delete if exists
  if store.collection_exists(collection_name).await? {
    store.delete_collection(collection_name).await?;
    println!("   🗑️  Deleted existing collection");
  }

  let config = CollectionConfig {
    dimension: 1536, // text-embedding-3-small dimension
    distance: DistanceMetric::Cosine,
    index_config: None,
  };

  store.create_collection(collection_name, config).await?;
  println!("   ✅ Collection created\n");

  // Step 4: Demonstrate embedding generation
  println!("4️⃣  Generating embeddings...");
  // Create a separate provider instance for demonstration
  let demo_provider = OpenAIEmbedding::new("text-embedding-3-small")?;

  let sample_texts = vec![
    "Rust is a systems programming language focused on safety and performance.",
    "Python is a high-level programming language known for readability.",
    "JavaScript is the language of the web, running in browsers and Node.js.",
  ];

  let embeddings = demo_provider.embed_batch(sample_texts.clone()).await?;
  println!("   ✅ Generated {} embeddings", embeddings.len());

  // Show cost tracking
  let stats = demo_provider.get_cost_stats().await;
  println!("   💰 Cost: ${:.6}", stats.total_cost);
  println!("   📊 Tokens used: {}", stats.total_tokens);
  println!("   🔢 Requests: {}\n", stats.request_count);

  // Step 5: Index documents with metadata
  println!("5️⃣  Indexing documents with metadata and embeddings...");

  let documents = vec![
    Document::new(sample_texts[0])
      .with_embedding(embeddings[0].clone())
      .with_metadata("language".to_string(), "Rust".into())
      .with_metadata("category".to_string(), "systems".into())
      .with_metadata("year".to_string(), 2015i64.into())
      .with_metadata("difficulty".to_string(), "advanced".into()),
    Document::new(sample_texts[1])
      .with_embedding(embeddings[1].clone())
      .with_metadata("language".to_string(), "Python".into())
      .with_metadata("category".to_string(), "scripting".into())
      .with_metadata("year".to_string(), 1991i64.into())
      .with_metadata("difficulty".to_string(), "beginner".into()),
    Document::new(sample_texts[2])
      .with_embedding(embeddings[2].clone())
      .with_metadata("language".to_string(), "JavaScript".into())
      .with_metadata("category".to_string(), "web".into())
      .with_metadata("year".to_string(), 1995i64.into())
      .with_metadata("difficulty".to_string(), "intermediate".into()),
  ];

  let ids = store.add_documents(collection_name, documents).await?;
  println!("   ✅ Indexed {} documents\n", ids.len());

  // Step 6: Text-based similarity search (NEW in Phase 3!)
  println!("6️⃣  TEXT-BASED SIMILARITY SEARCH (Phase 3 Feature!)");
  let query = "What's the best language for web development?";
  println!("   🔍 Query: \"{}\"", query);

  let results = store
    .similarity_search(collection_name, query, 3, None)
    .await?;

  println!("   📋 Results:");
  for (i, result) in results.iter().enumerate() {
    println!("      {}. Score: {:.4}", i + 1, result.score);
    println!("         Content: {}", result.content);
    if let Some(MetadataValue::String(lang)) = result.metadata.get("language") {
      println!("         Language: {}", lang);
    }
  }
  println!();

  // Step 7: Advanced filter search (NEW in Phase 3!)
  println!("7️⃣  ADVANCED FILTER SEARCH (Phase 3 Feature!)");

  // Filter 1: Match condition
  println!("   Filter 1: Languages with difficulty = 'beginner'");
  let filter = Filter {
    must: Some(vec![Condition::Match {
      field: "difficulty".to_string(),
      value: MetadataValue::String("beginner".to_string()),
    }]),
    should: None,
    must_not: None,
  };

  let filtered_results = store
    .similarity_search(collection_name, "programming language", 5, Some(filter))
    .await?;

  println!("   📋 Found {} results:", filtered_results.len());
  for result in &filtered_results {
    if let Some(MetadataValue::String(lang)) = result.metadata.get("language") {
      println!("      - {}", lang);
    }
  }
  println!();

  // Filter 2: Range condition
  println!("   Filter 2: Languages created after 1993");
  let filter = Filter {
    must: Some(vec![Condition::Range {
      field: "year".to_string(),
      gte: Some(1993.0),
      lte: None,
    }]),
    should: None,
    must_not: None,
  };

  let filtered_results = store
    .similarity_search(collection_name, "modern programming", 5, Some(filter))
    .await?;

  println!("   📋 Found {} results:", filtered_results.len());
  for result in &filtered_results {
    if let (Some(MetadataValue::String(lang)), Some(MetadataValue::Integer(year))) =
      (result.metadata.get("language"), result.metadata.get("year"))
    {
      println!("      - {} ({})", lang, year);
    }
  }
  println!();

  // Filter 3: Complex filter (must + must_not)
  println!("   Filter 3: Complex - web OR scripting, but NOT beginner");
  let filter = Filter {
    must: None,
    should: Some(vec![
      Condition::Match {
        field: "category".to_string(),
        value: MetadataValue::String("web".to_string()),
      },
      Condition::Match {
        field: "category".to_string(),
        value: MetadataValue::String("scripting".to_string()),
      },
    ]),
    must_not: Some(vec![Condition::Match {
      field: "difficulty".to_string(),
      value: MetadataValue::String("beginner".to_string()),
    }]),
  };

  let filtered_results = store
    .similarity_search(collection_name, "programming", 5, Some(filter))
    .await?;

  println!("   📋 Found {} results:", filtered_results.len());
  for result in &filtered_results {
    if let (Some(MetadataValue::String(lang)), Some(MetadataValue::String(cat))) = (
      result.metadata.get("language"),
      result.metadata.get("category"),
    ) {
      println!("      - {} (category: {})", lang, cat);
    }
  }
  println!();

  // Step 8: Show final cost statistics
  println!("8️⃣  Final Cost Statistics");
  let final_stats = demo_provider.get_cost_stats().await;
  println!("   💰 Total cost: ${:.6}", final_stats.total_cost);
  println!("   📊 Total tokens: {}", final_stats.total_tokens);
  println!("   🔢 Total requests: {}\n", final_stats.request_count);

  // Step 9: Collection statistics
  println!("9️⃣  Collection Statistics");
  let collection_stats = store.get_collection_stats(collection_name).await?;
  println!("   📁 Collection: {}", collection_stats.name);
  println!("   📄 Documents: {}", collection_stats.document_count);
  println!("   📐 Dimension: {}\n", collection_stats.dimension);

  // Cleanup
  println!("🧹 Cleaning up...");
  store.delete_collection(collection_name).await?;
  println!("   ✅ Collection deleted\n");

  println!("✨ Demo completed successfully!");
  println!("\n=== Phase 3 Features Demonstrated ===");
  println!("✅ OpenAI embedding provider with rate limiting");
  println!("✅ Cost tracking for API usage");
  println!("✅ Text-based similarity search");
  println!("✅ Advanced filter support (Match, Range, Complex)");
  println!("✅ Builder pattern for flexible configuration");

  Ok(())
}
