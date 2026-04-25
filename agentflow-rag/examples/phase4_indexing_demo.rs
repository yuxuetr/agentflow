//! Phase 4: Document Processing - Complete Indexing Workflow Demo
//!
//! This example demonstrates the full document processing pipeline:
//! 1. Load documents from various formats (Text, CSV, JSON)
//! 2. Chunk documents using different strategies (Fixed, Sentence, Recursive)
//! 3. Generate embeddings using OpenAI
//! 4. Index documents into Qdrant vector store
//! 5. Perform semantic search
//!
//! # Prerequisites
//! - Running Qdrant server: `docker run -p 6334:6334 qdrant/qdrant`
//! - OPENAI_API_KEY environment variable set
//!
//! # Usage
//! ```bash
//! export OPENAI_API_KEY=sk-...
//! cargo run --example phase4_indexing_demo --features qdrant
//! ```

use agentflow_rag::{
  chunking::{ChunkingStrategy, FixedSizeChunker, RecursiveChunker, SentenceChunker},
  embeddings::{EmbeddingProvider, OpenAIEmbedding},
  indexing::IndexingPipeline,
  sources::{csv::CsvLoader, text::TextLoader, DocumentLoader},
  types::{CollectionConfig, DistanceMetric, Document},
  vectorstore::{QdrantStore, VectorStore},
};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize tracing
  tracing_subscriber::fmt()
    .with_env_filter("agentflow_rag=debug,phase4_indexing_demo=info")
    .init();

  println!("=== Phase 4: Document Processing - Complete Indexing Workflow ===\n");

  // Step 1: Create test documents
  println!("1️⃣  Creating test documents...");
  let temp_dir = create_test_documents().await?;
  println!("   ✅ Created test documents in temporary directory\n");

  // Step 2: Load documents from different sources
  println!("2️⃣  Loading documents from various formats...");
  let documents = load_documents(&temp_dir).await?;
  println!("   ✅ Loaded {} documents", documents.len());
  for doc in &documents {
    if let Some(source) = doc.metadata.get("source") {
      println!("      - {:?}", source);
    }
  }
  println!();

  // Step 3: Create embedding provider
  println!("3️⃣  Creating OpenAI embedding provider...");
  let embedding_provider = Arc::new(
    OpenAIEmbedding::builder("text-embedding-3-small")
      .requests_per_minute(500)
      .timeout_secs(60)
      .build()?,
  );

  println!("   ✅ Model: {}", embedding_provider.model_name());
  println!("   ✅ Dimension: {}", embedding_provider.dimension());
  println!();

  // Step 4: Connect to Qdrant
  println!("4️⃣  Connecting to Qdrant vector store...");
  let store =
    QdrantStore::with_embedding_provider("http://localhost:6334", embedding_provider.clone())
      .await?;
  println!("   ✅ Connected to Qdrant\n");

  // Step 5: Demonstrate different chunking strategies
  println!("5️⃣  Demonstrating different chunking strategies...\n");

  demonstrate_chunking_strategies(&documents[0]).await?;

  // Step 6: Index documents with different strategies
  println!("6️⃣  Indexing documents with different chunking strategies...\n");

  let dimension = embedding_provider.dimension();

  // Strategy 1: Fixed-size chunking
  index_with_strategy(
    &store,
    &documents,
    "fixed_size",
    FixedSizeChunker::new(200, 50),
    dimension,
  )
  .await?;

  // Strategy 2: Sentence-based chunking
  index_with_strategy(
    &store,
    &documents,
    "sentence_based",
    SentenceChunker::new(300, 50),
    dimension,
  )
  .await?;

  // Strategy 3: Recursive chunking
  index_with_strategy(
    &store,
    &documents,
    "recursive",
    RecursiveChunker::new(250, 50),
    dimension,
  )
  .await?;

  // Step 7: Perform semantic search on different collections
  println!("7️⃣  Performing semantic search across collections...\n");

  let query = "What is machine learning?";
  println!("   🔍 Query: \"{}\"", query);

  for collection in ["fixed_size", "sentence_based", "recursive"] {
    let results = store.similarity_search(collection, query, 3, None).await?;

    println!("\n   📋 Results from '{}' collection:", collection);
    for (i, result) in results.iter().enumerate() {
      println!("      {}. Score: {:.4}", i + 1, result.score);
      println!(
        "         {}",
        result.content.chars().take(100).collect::<String>()
      );
      if result.content.len() > 100 {
        println!("         ...");
      }
    }
  }
  println!();

  // Step 8: Show final statistics
  println!("8️⃣  Final Statistics\n");

  let embedding_stats = embedding_provider.get_cost_stats().await;
  println!("   💰 Embedding Costs:");
  println!("      Total cost: ${:.6}", embedding_stats.total_cost);
  println!("      Total tokens: {}", embedding_stats.total_tokens);
  println!("      Total requests: {}", embedding_stats.request_count);
  println!();

  for collection in ["fixed_size", "sentence_based", "recursive"] {
    if let Ok(stats) = store.get_collection_stats(collection).await {
      println!("   📊 Collection: {}", collection);
      println!("      Documents: {}", stats.document_count);
      println!("      Dimension: {}", stats.dimension);
      println!();
    }
  }

  // Cleanup
  println!("9️⃣  Cleaning up...");
  for collection in ["fixed_size", "sentence_based", "recursive"] {
    if let Err(e) = store.delete_collection(collection).await {
      tracing::warn!("Failed to delete collection {}: {}", collection, e);
    }
  }
  println!("   ✅ Collections deleted\n");

  println!("✨ Demo completed successfully!");
  println!("\n=== Phase 4 Features Demonstrated ==");
  println!("✅ Document loading (Text, CSV, JSON)");
  println!("✅ Multiple chunking strategies (Fixed, Sentence, Recursive)");
  println!("✅ OpenAI embedding generation");
  println!("✅ Indexing pipeline");
  println!("✅ Semantic search with different chunking strategies");
  println!("✅ Cost tracking and statistics");

  Ok(())
}

/// Create test documents in a temporary directory
async fn create_test_documents() -> Result<TempDir, Box<dyn std::error::Error>> {
  let temp_dir = TempDir::new()?;

  // Create text file
  let text_content = r#"Machine learning is a subset of artificial intelligence that focuses on building systems that can learn from and make decisions based on data. It involves training algorithms on large datasets to identify patterns and make predictions.

Deep learning is a specialized form of machine learning that uses artificial neural networks with multiple layers. These networks can automatically learn hierarchical representations of data, making them particularly effective for tasks like image recognition and natural language processing.

Natural language processing (NLP) combines machine learning with linguistics to enable computers to understand, interpret, and generate human language. Modern NLP systems use transformer architectures and attention mechanisms to achieve state-of-the-art results."#;

  fs::write(temp_dir.path().join("ml_overview.txt"), text_content).await?;

  // Create markdown file
  let md_content = r#"# AI Technologies

## Computer Vision
Computer vision enables machines to interpret and understand visual information from the world. It powers applications like facial recognition, autonomous vehicles, and medical image analysis.

## Reinforcement Learning
Reinforcement learning trains agents to make sequential decisions by rewarding desired behaviors. It has achieved remarkable success in game playing and robotics."#;

  fs::write(temp_dir.path().join("ai_tech.md"), md_content).await?;

  // Create CSV file
  let csv_content = r#"topic,description,difficulty
ML Basics,Introduction to machine learning concepts,beginner
Deep Learning,Neural networks and backpropagation,advanced
NLP,Processing and understanding text,intermediate"#;

  fs::write(temp_dir.path().join("topics.csv"), csv_content).await?;

  // Create JSON file
  let json_content = r#"[
  {"title": "AI Ethics", "content": "Ethical considerations in AI development include fairness, transparency, and accountability."},
  {"title": "ML Operations", "content": "MLOps practices streamline the deployment and monitoring of machine learning models in production."}
]"#;

  fs::write(temp_dir.path().join("concepts.json"), json_content).await?;

  Ok(temp_dir)
}

/// Load documents from different sources
async fn load_documents(temp_dir: &TempDir) -> Result<Vec<Document>, Box<dyn std::error::Error>> {
  let mut all_docs = Vec::new();

  // Load text/markdown files
  let text_loader = TextLoader::new();
  let mut text_docs = text_loader.load_directory(temp_dir.path(), false).await?;
  all_docs.append(&mut text_docs);

  // Load CSV/JSON files
  let csv_loader = CsvLoader::new().with_content_field("description");
  let mut csv_docs = csv_loader.load_directory(temp_dir.path(), false).await?;
  all_docs.append(&mut csv_docs);

  Ok(all_docs)
}

/// Demonstrate different chunking strategies
async fn demonstrate_chunking_strategies(doc: &Document) -> Result<(), Box<dyn std::error::Error>> {
  let text = &doc.content;

  // Fixed-size chunking
  let fixed_chunker = FixedSizeChunker::new(200, 50);
  let fixed_chunks = fixed_chunker.chunk(text)?;
  println!("   📦 Fixed-Size Chunking:");
  println!("      Chunk size: 200, Overlap: 50");
  println!("      Chunks created: {}", fixed_chunks.len());

  // Sentence-based chunking
  let sentence_chunker = SentenceChunker::new(300, 50);
  let sentence_chunks = sentence_chunker.chunk(text)?;
  println!("\n   📝 Sentence-Based Chunking:");
  println!("      Chunk size: 300, Overlap: 50");
  println!("      Chunks created: {}", sentence_chunks.len());

  // Recursive chunking
  let recursive_chunker = RecursiveChunker::new(250, 50);
  let recursive_chunks = recursive_chunker.chunk(text)?;
  println!("\n   🔄 Recursive Chunking:");
  println!("      Chunk size: 250, Overlap: 50");
  println!("      Chunks created: {}", recursive_chunks.len());

  println!();
  Ok(())
}

/// Index documents with a specific chunking strategy
async fn index_with_strategy<C>(
  store: &QdrantStore,
  documents: &[Document],
  collection_name: &str,
  chunker: C,
  dimension: usize,
) -> Result<(), Box<dyn std::error::Error>>
where
  C: agentflow_rag::chunking::ChunkingStrategy + Send + Sync + 'static,
{
  println!("   Indexing with {} strategy...", collection_name);

  // Create collection
  if store.collection_exists(collection_name).await? {
    store.delete_collection(collection_name).await?;
  }

  let config = CollectionConfig {
    dimension,
    distance: DistanceMetric::Cosine,
    index_config: None,
  };
  store.create_collection(collection_name, config).await?;

  // Create a fresh embedding provider for this pipeline
  let embedder = OpenAIEmbedding::new("text-embedding-3-small")?;

  // Create indexing pipeline
  // Note: We create a new store instance for the pipeline since QdrantStore doesn't implement Clone
  let pipeline_store = QdrantStore::builder("http://localhost:6334")
    .embedding_provider(Arc::new(OpenAIEmbedding::new("text-embedding-3-small")?))
    .build()
    .await?;
  let pipeline = IndexingPipeline::new(
    Box::new(chunker),
    Box::new(embedder),
    Box::new(pipeline_store),
  );

  // Index all documents
  let stats = pipeline
    .index_documents(collection_name, documents.to_vec())
    .await?;

  println!(
    "      ✅ Documents processed: {}",
    stats.documents_processed
  );
  println!("      ✅ Chunks created: {}", stats.chunks_created);
  println!(
    "      ✅ Embeddings generated: {}",
    stats.embeddings_generated
  );
  println!("      ✅ Processing time: {}ms", stats.processing_time_ms);
  if stats.errors > 0 {
    println!("      ⚠️  Errors: {}", stats.errors);
  }
  println!();

  Ok(())
}
