//! Phase 7: Local Embeddings with ONNX Runtime
//!
//! This example demonstrates using local ONNX models for embedding generation
//! without API calls, providing cost-free and private embeddings.
//!
//! # Prerequisites
//! 1. Download a sentence-transformers model in ONNX format
//! 2. Extract the model.onnx and tokenizer.json files
//!
//! # Model Download
//! For testing, you can use all-MiniLM-L6-v2:
//! ```bash
//! # Install optimum for ONNX export
//! pip install optimum[exporters]
//!
//! # Export model to ONNX
//! optimum-cli export onnx --model sentence-transformers/all-MiniLM-L6-v2 models/all-MiniLM-L6-v2
//! ```
//!
//! # Usage
//! ```bash
//! cargo run --example phase7_local_embeddings --features local-embeddings
//! ```

#[cfg(feature = "local-embeddings")]
use agentflow_rag::embeddings::{EmbeddingProvider, ONNXEmbedding};

#[cfg(not(feature = "local-embeddings"))]
fn main() {
  println!("This example requires the 'local-embeddings' feature.");
  println!("Run with: cargo run --example phase7_local_embeddings --features local-embeddings");
}

#[cfg(feature = "local-embeddings")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
  println!("=== Phase 7: Local Embeddings with ONNX Runtime ===\n");

  // Check if model files exist
  let model_path = "models/all-MiniLM-L6-v2/model.onnx";
  let tokenizer_path = "models/all-MiniLM-L6-v2/tokenizer.json";

  if !std::path::Path::new(model_path).exists() {
    println!("❌ Model file not found: {}", model_path);
    println!("\n📥 To download and convert the model:");
    println!("   1. Install optimum: pip install optimum[exporters]");
    println!("   2. Export model:");
    println!("      optimum-cli export onnx --model sentence-transformers/all-MiniLM-L6-v2 models/all-MiniLM-L6-v2");
    println!("\nAlternatively, download pre-converted models from Hugging Face.");
    return Ok(());
  }

  println!("📚 Loading ONNX model...");
  println!("   Model: {}", model_path);
  println!("   Tokenizer: {}", tokenizer_path);

  let embedding = ONNXEmbedding::builder()
    .with_model_path(model_path)
    .with_tokenizer_path(tokenizer_path)
    .with_model_name("all-MiniLM-L6-v2")
    .with_dimension(384)
    .with_max_length(512)
    .with_normalization(true)
    .build()
    .await?;

  println!("✅ Model loaded successfully!");
  println!("   Model: {}", embedding.model_name());
  println!("   Dimension: {}", embedding.dimension());
  println!("   Max tokens: {}\n", embedding.max_tokens());

  // Demo 1: Single text embedding
  println!("1️⃣  Single Text Embedding");
  println!("{}", "=".repeat(60));

  let text = "Machine learning is a subset of artificial intelligence.";
  println!("Text: \"{}\"", text);

  let start = std::time::Instant::now();
  let vector = embedding.embed_text(text).await?;
  let duration = start.elapsed();

  println!("✅ Generated embedding in {:?}", duration);
  println!("   Vector dimension: {}", vector.len());
  println!("   First 5 values: {:?}", &vector[..5]);
  println!();

  // Demo 2: Batch embeddings
  println!("2️⃣  Batch Embeddings");
  println!("{}", "=".repeat(60));

  let texts = vec![
    "Deep learning uses neural networks.",
    "Natural language processing analyzes human language.",
    "Computer vision enables machines to interpret visual data.",
    "Reinforcement learning learns from trial and error.",
    "Unsupervised learning finds patterns in unlabeled data.",
  ];

  println!("Processing {} texts...", texts.len());

  let start = std::time::Instant::now();
  let vectors = embedding.embed_batch(texts.clone()).await?;
  let duration = start.elapsed();

  println!("✅ Generated {} embeddings in {:?}", vectors.len(), duration);
  println!("   Average time per text: {:?}", duration / texts.len() as u32);
  println!();

  // Demo 3: Similarity calculation
  println!("3️⃣  Semantic Similarity");
  println!("{}", "=".repeat(60));

  let query = "AI and machine learning";
  let query_vec = embedding.embed_text(query).await?;

  println!("Query: \"{}\"", query);
  println!("\nSimilarity scores:");

  for (text, vector) in texts.iter().zip(vectors.iter()) {
    let similarity = cosine_similarity(&query_vec, vector);
    println!("   {:.4} - \"{}\"", similarity, text);
  }
  println!();

  // Demo 4: Performance comparison
  println!("4️⃣  Performance Metrics");
  println!("{}", "=".repeat(60));

  let test_texts: Vec<&str> = (0..10)
    .map(|i| texts[i % texts.len()])
    .collect();

  let start = std::time::Instant::now();
  let _ = embedding.embed_batch(test_texts.clone()).await?;
  let batch_duration = start.elapsed();

  let start = std::time::Instant::now();
  for text in &test_texts {
    let _ = embedding.embed_text(text).await?;
  }
  let sequential_duration = start.elapsed();

  println!("Batch processing (10 texts): {:?}", batch_duration);
  println!("Sequential processing (10 texts): {:?}", sequential_duration);
  println!("Speedup: {:.2}x", sequential_duration.as_secs_f64() / batch_duration.as_secs_f64());
  println!();

  println!("✨ Phase 7 demonstration complete!\n");
  println!("=== Phase 7 Features Demonstrated ===");
  println!("✅ Local ONNX model loading");
  println!("✅ Tokenization with sentence-transformers");
  println!("✅ Mean pooling and L2 normalization");
  println!("✅ Single text embedding generation");
  println!("✅ Batch embedding generation");
  println!("✅ Semantic similarity calculation");
  println!("✅ Performance benchmarking");
  println!("\n💡 Benefits:");
  println!("   • No API calls - completely offline");
  println!("   • Zero cost - no per-token charges");
  println!("   • Privacy - data never leaves your machine");
  println!("   • Fast - local inference with optimization");
  println!("   • Flexible - use any sentence-transformers model");

  Ok(())
}

#[cfg(feature = "local-embeddings")]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
  let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
  dot_product // Vectors are already normalized
}
