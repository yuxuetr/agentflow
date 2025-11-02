//! Basic Qdrant vector store usage example
//!
//! This example demonstrates:
//! - Connecting to a Qdrant instance
//! - Creating a collection with configuration
//! - Adding documents with embeddings
//! - Performing vector similarity search
//! - Retrieving collection statistics
//!
//! ## Prerequisites
//!
//! Start a local Qdrant server:
//! ```bash
//! docker run -p 6333:6333 -p 6334:6334 qdrant/qdrant:latest
//! ```
//!
//! ## Running this example
//!
//! ```bash
//! cargo run --package agentflow-rag --example qdrant_basic --features qdrant
//! ```

use agentflow_rag::{
  error::Result,
  types::{CollectionConfig, DistanceMetric, Document},
  vectorstore::{QdrantStore, VectorStore},
};

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize tracing for logging
  tracing_subscriber::fmt::init();

  println!("=== Qdrant Vector Store Example ===\n");

  // 1. Connect to Qdrant
  println!("1. Connecting to Qdrant at http://localhost:6334...");
  let store = QdrantStore::new("http://localhost:6334").await?;
  println!("   ✓ Connected successfully\n");

  // 2. Create a collection
  let collection_name = "example_documents";
  println!("2. Creating collection '{}'...", collection_name);

  let config = CollectionConfig {
    dimension: 384, // Typical for sentence-transformers/all-MiniLM-L6-v2
    distance: DistanceMetric::Cosine,
    index_config: None, // Use default HNSW configuration
  };

  // Delete collection if it exists (for clean runs)
  if store.collection_exists(collection_name).await? {
    println!("   Collection already exists, deleting...");
    store.delete_collection(collection_name).await?;
  }

  store.create_collection(collection_name, config).await?;
  println!("   ✓ Collection created\n");

  // 3. Prepare sample documents with embeddings
  println!("3. Adding sample documents...");

  let documents = vec![
    Document::new("The quick brown fox jumps over the lazy dog")
      .with_embedding(generate_mock_embedding(1))
      .with_metadata("category".to_string(), "animals".into())
      .with_metadata("language".to_string(), "english".into()),
    Document::new("Rust is a systems programming language")
      .with_embedding(generate_mock_embedding(2))
      .with_metadata("category".to_string(), "programming".into())
      .with_metadata("language".to_string(), "english".into()),
    Document::new("Vector databases enable semantic search")
      .with_embedding(generate_mock_embedding(3))
      .with_metadata("category".to_string(), "technology".into())
      .with_metadata("language".to_string(), "english".into()),
    Document::new("Machine learning models generate embeddings")
      .with_embedding(generate_mock_embedding(4))
      .with_metadata("category".to_string(), "ai".into())
      .with_metadata("language".to_string(), "english".into()),
  ];

  let doc_ids = store.add_documents(collection_name, documents).await?;
  println!("   ✓ Added {} documents", doc_ids.len());
  println!("   Document IDs: {:?}\n", doc_ids);

  // 4. Perform similarity search
  println!("4. Performing similarity search...");

  let query_vector = generate_mock_embedding(2); // Similar to the Rust document
  let results = store
    .similarity_search_by_vector(collection_name, query_vector, 3, None)
    .await?;

  println!("   Found {} results:", results.len());
  for (i, result) in results.iter().enumerate() {
    println!("   {}. Score: {:.4}", i + 1, result.score);
    println!("      Content: {}", result.content);
    println!("      Metadata: {:?}", result.metadata);
  }
  println!();

  // 5. Get collection statistics
  println!("5. Retrieving collection statistics...");
  let stats = store.get_collection_stats(collection_name).await?;
  println!("   Collection: {}", stats.name);
  println!("   Document count: {}", stats.document_count);
  println!("   Vector dimension: {}", stats.dimension);
  println!();

  // 6. List all collections
  println!("6. Listing all collections...");
  let collections = store.list_collections().await?;
  println!("   Collections: {:?}\n", collections);

  // 7. Delete specific documents
  println!("7. Deleting first document...");
  store
    .delete_documents(collection_name, vec![doc_ids[0].clone()])
    .await?;
  println!("   ✓ Document deleted\n");

  // 8. Cleanup - delete collection
  println!("8. Cleaning up - deleting collection...");
  store.delete_collection(collection_name).await?;
  println!("   ✓ Collection deleted\n");

  println!("=== Example completed successfully ===");

  Ok(())
}

/// Generate a mock embedding vector for demonstration
/// In real applications, use an embedding model like:
/// - OpenAI text-embedding-3-small
/// - sentence-transformers/all-MiniLM-L6-v2
/// - Local ONNX models
fn generate_mock_embedding(seed: u32) -> Vec<f32> {
  // Generate deterministic mock embedding based on seed
  (0..384)
    .map(|i| ((seed * 1000 + i) as f32).sin() * 0.1)
    .collect()
}
