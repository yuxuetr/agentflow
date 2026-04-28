//! Integration tests for Phase 3: Embeddings Integration
//!
//! These tests require:
//! - Running Qdrant server on localhost:6334
//! - OPENAI_API_KEY environment variable
//!
//! Run with: `cargo test --test embeddings_integration --features qdrant -- --ignored`

use agentflow_rag::{
  embeddings::{EmbeddingProvider, OpenAIEmbedding},
  types::{CollectionConfig, Condition, DistanceMetric, Document, Filter, MetadataValue},
  vectorstore::{QdrantStore, VectorStore},
};
use std::sync::Arc;

/// Helper to create a test collection with embeddings
async fn setup_test_collection(
  store: &QdrantStore,
  collection_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  // Create collection
  let config = CollectionConfig {
    dimension: 1536,
    distance: DistanceMetric::Cosine,
    index_config: None,
  };

  store.create_collection(collection_name, config).await?;

  // Create embedding provider for generating embeddings
  let provider = OpenAIEmbedding::new("text-embedding-3-small")?;

  // Create sample documents
  let texts = vec![
    "Machine learning is a subset of artificial intelligence.",
    "Deep learning uses neural networks with multiple layers.",
    "Natural language processing helps computers understand human language.",
    "Computer vision enables machines to interpret visual information.",
  ];

  let embeddings = provider.embed_batch(texts.clone()).await?;

  let documents: Vec<Document> = texts
    .into_iter()
    .zip(embeddings)
    .enumerate()
    .map(|(i, (text, embedding))| {
      Document::new(text)
        .with_embedding(embedding)
        .with_metadata("topic".to_string(), "AI".into())
        .with_metadata("index".to_string(), (i as i64).into())
    })
    .collect();

  store.add_documents(collection_name, documents).await?;

  Ok(())
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_text_based_similarity_search() {
  // Setup
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let store = QdrantStore::with_embedding_provider("http://localhost:6334", Arc::new(provider))
    .await
    .unwrap();

  let collection = "test_text_search";

  // Clean up if exists
  if store.collection_exists(collection).await.unwrap() {
    store.delete_collection(collection).await.unwrap();
  }

  setup_test_collection(&store, collection).await.unwrap();

  // Test: Text-based search
  let results = store
    .similarity_search(collection, "understanding language", 3, None)
    .await
    .unwrap();

  assert!(!results.is_empty());
  assert!(results.len() <= 3);

  // Verify results are ordered by relevance
  for i in 1..results.len() {
    assert!(results[i - 1].score >= results[i].score);
  }

  // Cleanup
  store.delete_collection(collection).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_filter_match_condition() {
  // Setup
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let store = QdrantStore::with_embedding_provider("http://localhost:6334", Arc::new(provider))
    .await
    .unwrap();

  let collection = "test_filter_match";

  if store.collection_exists(collection).await.unwrap() {
    store.delete_collection(collection).await.unwrap();
  }

  // Create documents with different categories
  let config = CollectionConfig {
    dimension: 1536,
    distance: DistanceMetric::Cosine,
    index_config: None,
  };
  store.create_collection(collection, config).await.unwrap();

  let embedding_provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let texts = vec![
    "Rust programming language",
    "Python programming language",
    "JavaScript programming language",
  ];

  let embeddings = embedding_provider.embed_batch(texts.clone()).await.unwrap();

  let documents = vec![
    Document::new(texts[0])
      .with_embedding(embeddings[0].clone())
      .with_metadata("category".to_string(), "systems".into()),
    Document::new(texts[1])
      .with_embedding(embeddings[1].clone())
      .with_metadata("category".to_string(), "scripting".into()),
    Document::new(texts[2])
      .with_embedding(embeddings[2].clone())
      .with_metadata("category".to_string(), "web".into()),
  ];

  store.add_documents(collection, documents).await.unwrap();

  // Test: Match filter
  let filter = Filter {
    must: Some(vec![Condition::Match {
      field: "category".to_string(),
      value: MetadataValue::String("web".to_string()),
    }]),
    should: None,
    must_not: None,
  };

  let results = store
    .similarity_search(collection, "programming", 5, Some(filter))
    .await
    .unwrap();

  assert_eq!(results.len(), 1);
  assert!(results[0].content.contains("JavaScript"));

  // Cleanup
  store.delete_collection(collection).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_filter_range_condition() {
  // Setup
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let store = QdrantStore::with_embedding_provider("http://localhost:6334", Arc::new(provider))
    .await
    .unwrap();

  let collection = "test_filter_range";

  if store.collection_exists(collection).await.unwrap() {
    store.delete_collection(collection).await.unwrap();
  }

  let config = CollectionConfig {
    dimension: 1536,
    distance: DistanceMetric::Cosine,
    index_config: None,
  };
  store.create_collection(collection, config).await.unwrap();

  let embedding_provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let texts = vec!["Document 1", "Document 2", "Document 3"];
  let embeddings = embedding_provider.embed_batch(texts.clone()).await.unwrap();

  let documents = vec![
    Document::new(texts[0])
      .with_embedding(embeddings[0].clone())
      .with_metadata("score".to_string(), 0.5.into()),
    Document::new(texts[1])
      .with_embedding(embeddings[1].clone())
      .with_metadata("score".to_string(), 0.75.into()),
    Document::new(texts[2])
      .with_embedding(embeddings[2].clone())
      .with_metadata("score".to_string(), 0.9.into()),
  ];

  store.add_documents(collection, documents).await.unwrap();

  // Test: Range filter
  let filter = Filter {
    must: Some(vec![Condition::Range {
      field: "score".to_string(),
      gte: Some(0.7),
      lte: None,
    }]),
    should: None,
    must_not: None,
  };

  let results = store
    .similarity_search(collection, "document", 5, Some(filter))
    .await
    .unwrap();

  assert_eq!(results.len(), 2); // Only documents with score >= 0.7

  // Cleanup
  store.delete_collection(collection).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_complex_filter() {
  // Setup
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let store = QdrantStore::with_embedding_provider("http://localhost:6334", Arc::new(provider))
    .await
    .unwrap();

  let collection = "test_complex_filter";

  if store.collection_exists(collection).await.unwrap() {
    store.delete_collection(collection).await.unwrap();
  }

  let config = CollectionConfig {
    dimension: 1536,
    distance: DistanceMetric::Cosine,
    index_config: None,
  };
  store.create_collection(collection, config).await.unwrap();

  let embedding_provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  let texts = vec!["Doc A", "Doc B", "Doc C", "Doc D"];
  let embeddings = embedding_provider.embed_batch(texts.clone()).await.unwrap();

  let documents = vec![
    Document::new(texts[0])
      .with_embedding(embeddings[0].clone())
      .with_metadata("category".to_string(), "tech".into())
      .with_metadata("status".to_string(), "active".into()),
    Document::new(texts[1])
      .with_embedding(embeddings[1].clone())
      .with_metadata("category".to_string(), "science".into())
      .with_metadata("status".to_string(), "active".into()),
    Document::new(texts[2])
      .with_embedding(embeddings[2].clone())
      .with_metadata("category".to_string(), "tech".into())
      .with_metadata("status".to_string(), "archived".into()),
    Document::new(texts[3])
      .with_embedding(embeddings[3].clone())
      .with_metadata("category".to_string(), "science".into())
      .with_metadata("status".to_string(), "archived".into()),
  ];

  store.add_documents(collection, documents).await.unwrap();

  // Test: Complex filter - (tech OR science) AND NOT archived
  let filter = Filter {
    must: None,
    should: Some(vec![
      Condition::Match {
        field: "category".to_string(),
        value: MetadataValue::String("tech".to_string()),
      },
      Condition::Match {
        field: "category".to_string(),
        value: MetadataValue::String("science".to_string()),
      },
    ]),
    must_not: Some(vec![Condition::Match {
      field: "status".to_string(),
      value: MetadataValue::String("archived".to_string()),
    }]),
  };

  let results = store
    .similarity_search(collection, "document", 10, Some(filter))
    .await
    .unwrap();

  assert_eq!(results.len(), 2); // Only active tech/science docs

  // Cleanup
  store.delete_collection(collection).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_cost_tracking() {
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();

  // Reset cost tracker
  provider.reset_cost_tracker().await;

  // Generate embeddings
  provider.embed_text("Test text").await.unwrap();

  // Check cost tracker
  let stats = provider.get_cost_stats().await;
  assert!(stats.total_tokens > 0);
  assert!(stats.total_cost > 0.0);
  assert_eq!(stats.request_count, 1);

  // Generate batch
  provider
    .embed_batch(vec!["Text 1", "Text 2", "Text 3"])
    .await
    .unwrap();

  let stats = provider.get_cost_stats().await;
  assert_eq!(stats.request_count, 2);
}

#[tokio::test]
#[ignore] // Requires Qdrant server and API key
async fn test_batch_embedding_splitting() {
  let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
  provider.reset_cost_tracker().await;

  // Create a large batch that should be split
  let texts: Vec<&str> = (0..100)
    .map(|i| {
      if i % 2 == 0 {
        "Short text"
      } else {
        "Another short text"
      }
    })
    .collect();

  let result = provider.embed_batch(texts).await;
  assert!(result.is_ok());

  let embeddings = result.unwrap();
  assert_eq!(embeddings.len(), 100);

  // All embeddings should have correct dimension
  for embedding in &embeddings {
    assert_eq!(embedding.len(), 1536);
  }
}
