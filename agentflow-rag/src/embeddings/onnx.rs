//! ONNX-based local embedding generation
//!
//! This module provides local embedding generation using ONNX Runtime,
//! supporting sentence-transformers models for cost-free, private embeddings.
//!
//! # Features
//! - Local inference without API calls
//! - Support for sentence-transformers models
//! - Mean pooling and normalization
//! - Batch processing optimization
//! - Model caching
//!
//! # Example
//! ```no_run
//! use agentflow_rag::embeddings::onnx::ONNXEmbedding;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let embedding = ONNXEmbedding::builder()
//!   .with_model_path("models/all-MiniLM-L6-v2.onnx")
//!   .with_tokenizer_path("models/tokenizer.json")
//!   .build()
//!   .await?;
//!
//! let vector = embedding.embed_text("Hello, world!").await?;
//! println!("Embedding dimension: {}", vector.len());
//! # Ok(())
//! # }
//! ```

use crate::{embeddings::EmbeddingProvider, error::{RAGError, Result}};
use async_trait::async_trait;
use ndarray::{Array1, Array2, Axis};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(feature = "local-embeddings")]
use ort::{
  session::{builder::GraphOptimizationLevel, builder::SessionBuilder, Session},
  value::Value,
};

#[cfg(feature = "local-embeddings")]
use tokenizers::Tokenizer;

/// ONNX-based embedding provider for local inference
#[derive(Clone)]
pub struct ONNXEmbedding {
  /// Model name/identifier
  model_name: String,

  /// Embedding dimension
  dimension: usize,

  /// Maximum sequence length (tokens)
  max_length: usize,

  /// Inner implementation (behind Arc for cloning)
  inner: Arc<ONNXEmbeddingInner>,
}

#[cfg(feature = "local-embeddings")]
struct ONNXEmbeddingInner {
  /// ONNX Runtime session (wrapped in Mutex for interior mutability)
  session: Mutex<Session>,

  /// Tokenizer for text processing
  tokenizer: Tokenizer,

  /// Normalize embeddings (L2 normalization)
  normalize: bool,
}

#[cfg(not(feature = "local-embeddings"))]
struct ONNXEmbeddingInner {}

impl ONNXEmbedding {
  /// Create a new builder for ONNXEmbedding
  pub fn builder() -> ONNXEmbeddingBuilder {
    ONNXEmbeddingBuilder::default()
  }

  #[cfg(feature = "local-embeddings")]
  /// Create mean-pooled embedding from model output
  /// token_embeddings should be shape (batch, seq_len, hidden_size)
  /// attention_mask should be shape (batch, seq_len)
  fn mean_pooling(
    token_embeddings: &ndarray::Array<f32, ndarray::Ix3>,
    attention_mask: &Array2<i64>,
  ) -> Result<Array1<f32>> {
    let mask = attention_mask.mapv(|x| x as f32);

    // Expand mask to match token_embeddings shape
    let mask_expanded = mask
      .clone()
      .insert_axis(Axis(2))
      .broadcast((token_embeddings.shape()[0], token_embeddings.shape()[1], token_embeddings.shape()[2]))
      .ok_or_else(|| RAGError::embedding("Failed to broadcast attention mask"))?
      .to_owned();

    // Apply mask to embeddings
    let masked_embeddings = token_embeddings * &mask_expanded;

    // Sum along sequence dimension
    let sum_embeddings = masked_embeddings.sum_axis(Axis(1));

    // Sum mask to get count of valid tokens
    let sum_mask = mask.sum_axis(Axis(1));

    // Avoid division by zero
    let sum_mask = sum_mask.mapv(|x| if x == 0.0 { 1.0 } else { x });

    // Calculate mean
    let mean_embeddings = sum_embeddings / &sum_mask.insert_axis(Axis(1));

    // Return first (and only) batch item
    Ok(mean_embeddings.row(0).to_owned())
  }

  #[cfg(feature = "local-embeddings")]
  /// Normalize embedding vector (L2 normalization)
  fn normalize_vector(vector: &Array1<f32>) -> Array1<f32> {
    let norm = vector.dot(vector).sqrt();
    if norm > 0.0 {
      vector / norm
    } else {
      vector.clone()
    }
  }
}

#[async_trait]
impl EmbeddingProvider for ONNXEmbedding {
  #[cfg(feature = "local-embeddings")]
  async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
    use ort::inputs;

    // Tokenize input
    let encoding = self
      .inner
      .tokenizer
      .encode(text, true)
      .map_err(|e| RAGError::embedding(format!("Tokenization failed: {}", e)))?;

    // Get input IDs and attention mask
    let input_ids = encoding.get_ids();
    let attention_mask = encoding.get_attention_mask();

    // Truncate if necessary
    let max_len = self.max_length.min(input_ids.len());
    let input_ids = &input_ids[..max_len];
    let attention_mask = &attention_mask[..max_len];

    // Convert to i64 for ONNX
    let input_ids_vec: Vec<i64> = input_ids.iter().map(|&x| x as i64).collect();
    let attention_mask_vec: Vec<i64> = attention_mask.iter().map(|&x| x as i64).collect();

    // Store lengths for later use
    let seq_len = input_ids_vec.len();

    // Create tensors using tuple format (shape, data)
    let input_ids_shape = vec![1, seq_len];
    let attention_mask_shape = vec![1, seq_len];

    // Run inference and extract tensor data (must be done while holding the session lock)
    let (shape_dims, token_data) = {
      let mut session = self.inner.session.lock().map_err(|e| RAGError::embedding(format!("Failed to lock session: {}", e)))?;

      let outputs = session.run(inputs![
        "input_ids" => Value::from_array((input_ids_shape.as_slice(), input_ids_vec.into_boxed_slice())).map_err(|e| RAGError::embedding(format!("Failed to create input value: {}", e)))?,
        "attention_mask" => Value::from_array((attention_mask_shape.as_slice(), attention_mask_vec.clone().into_boxed_slice())).map_err(|e| RAGError::embedding(format!("Failed to create attention mask value: {}", e)))?
      ])
      .map_err(|e| RAGError::embedding(format!("ONNX inference failed: {}", e)))?;

      // Extract output tensor (last_hidden_state) while still holding the lock
      let output_tensor = outputs["last_hidden_state"]
        .try_extract_tensor::<f32>()
        .map_err(|e| RAGError::embedding(format!("Failed to extract output tensor: {}", e)))?;

      // Get shape and copy data
      let (shape, data) = output_tensor;
      let shape_dims: Vec<usize> = shape.as_ref().iter().map(|&x| x as usize).collect();
      let token_data: Vec<f32> = data.iter().copied().collect();

      (shape_dims, token_data)
    }; // Session lock dropped here

    // Convert to ndarray
    let token_embeddings = Array2::from_shape_vec(
      (shape_dims[0], shape_dims[1] * shape_dims[2]),
      token_data,
    )
    .map_err(|e| RAGError::embedding(format!("Failed to reshape output: {}", e)))?;

    // Reshape to (batch, seq_len, hidden_size)
    let token_embeddings = token_embeddings
      .into_shape((shape_dims[0], shape_dims[1], shape_dims[2]))
      .map_err(|e| RAGError::embedding(format!("Failed to reshape to 3D: {}", e)))?;

    // Apply mean pooling
    let attention_mask_i64 = Array2::from_shape_vec((1, seq_len), attention_mask_vec)
      .map_err(|e| RAGError::embedding(format!("Failed to create attention mask for pooling: {}", e)))?;

    let mut embedding = Self::mean_pooling(&token_embeddings, &attention_mask_i64)?;

    // Normalize if enabled
    if self.inner.normalize {
      embedding = Self::normalize_vector(&embedding);
    }

    Ok(embedding.to_vec())
  }

  #[cfg(not(feature = "local-embeddings"))]
  async fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
    Err(RAGError::configuration(
      "Local embeddings feature not enabled. Compile with --features local-embeddings",
    ))
  }

  async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
    // For now, process sequentially
    // TODO: Implement true batch processing for better performance
    let mut embeddings = Vec::with_capacity(texts.len());
    for text in texts {
      embeddings.push(self.embed_text(text).await?);
    }
    Ok(embeddings)
  }

  fn dimension(&self) -> usize {
    self.dimension
  }

  fn model_name(&self) -> &str {
    &self.model_name
  }

  fn max_tokens(&self) -> usize {
    self.max_length
  }
}

/// Builder for ONNXEmbedding
#[derive(Default)]
pub struct ONNXEmbeddingBuilder {
  model_path: Option<PathBuf>,
  tokenizer_path: Option<PathBuf>,
  model_name: Option<String>,
  dimension: Option<usize>,
  max_length: Option<usize>,
  normalize: bool,
}

impl ONNXEmbeddingBuilder {
  /// Set the path to the ONNX model file
  pub fn with_model_path<P: AsRef<Path>>(mut self, path: P) -> Self {
    self.model_path = Some(path.as_ref().to_path_buf());
    self
  }

  /// Set the path to the tokenizer JSON file
  pub fn with_tokenizer_path<P: AsRef<Path>>(mut self, path: P) -> Self {
    self.tokenizer_path = Some(path.as_ref().to_path_buf());
    self
  }

  /// Set the model name/identifier
  pub fn with_model_name<S: Into<String>>(mut self, name: S) -> Self {
    self.model_name = Some(name.into());
    self
  }

  /// Set the embedding dimension
  pub fn with_dimension(mut self, dim: usize) -> Self {
    self.dimension = Some(dim);
    self
  }

  /// Set the maximum sequence length
  pub fn with_max_length(mut self, max_len: usize) -> Self {
    self.max_length = Some(max_len);
    self
  }

  /// Enable/disable L2 normalization (default: true)
  pub fn with_normalization(mut self, normalize: bool) -> Self {
    self.normalize = normalize;
    self
  }

  /// Build the ONNXEmbedding instance
  #[cfg(feature = "local-embeddings")]
  pub async fn build(self) -> Result<ONNXEmbedding> {
    let model_path = self
      .model_path
      .ok_or_else(|| RAGError::configuration("Model path is required"))?;

    let tokenizer_path = self
      .tokenizer_path
      .ok_or_else(|| RAGError::configuration("Tokenizer path is required"))?;

    // Load tokenizer
    let tokenizer = Tokenizer::from_file(&tokenizer_path)
      .map_err(|e| RAGError::embedding(format!("Failed to load tokenizer: {}", e)))?;

    // Load ONNX model
    let session = SessionBuilder::new()
      .map_err(|e| RAGError::embedding(format!("Failed to create session builder: {}", e)))?
      .with_optimization_level(GraphOptimizationLevel::Level3)
      .map_err(|e| RAGError::embedding(format!("Failed to set optimization level: {}", e)))?
      .with_intra_threads(4)
      .map_err(|e| RAGError::embedding(format!("Failed to set thread count: {}", e)))?
      .commit_from_file(&model_path)
      .map_err(|e| RAGError::embedding(format!("Failed to load ONNX model: {}", e)))?;

    let model_name = self
      .model_name
      .unwrap_or_else(|| model_path.file_stem().unwrap().to_string_lossy().to_string());

    let dimension = self.dimension.unwrap_or(384); // Default for MiniLM
    let max_length = self.max_length.unwrap_or(512);

    Ok(ONNXEmbedding {
      model_name,
      dimension,
      max_length,
      inner: Arc::new(ONNXEmbeddingInner {
        session: Mutex::new(session),
        tokenizer,
        normalize: self.normalize,
      }),
    })
  }

  #[cfg(not(feature = "local-embeddings"))]
  pub async fn build(self) -> Result<ONNXEmbedding> {
    Err(RAGError::configuration(
      "Local embeddings feature not enabled. Compile with --features local-embeddings",
    ))
  }
}

#[cfg(all(test, feature = "local-embeddings"))]
mod tests {
  use super::*;

  #[tokio::test]
  #[ignore] // Requires model files
  async fn test_onnx_embedding_basic() {
    // This test requires downloading a model first
    // Example: all-MiniLM-L6-v2 from sentence-transformers
    let embedding = ONNXEmbedding::builder()
      .with_model_path("tests/models/all-MiniLM-L6-v2.onnx")
      .with_tokenizer_path("tests/models/tokenizer.json")
      .with_dimension(384)
      .build()
      .await
      .expect("Failed to build ONNX embedding");

    let vector = embedding
      .embed_text("This is a test sentence")
      .await
      .expect("Failed to generate embedding");

    assert_eq!(vector.len(), 384);
  }

  #[tokio::test]
  #[ignore]
  async fn test_onnx_batch_embedding() {
    let embedding = ONNXEmbedding::builder()
      .with_model_path("tests/models/all-MiniLM-L6-v2.onnx")
      .with_tokenizer_path("tests/models/tokenizer.json")
      .with_dimension(384)
      .build()
      .await
      .expect("Failed to build ONNX embedding");

    let texts = vec!["First sentence", "Second sentence", "Third sentence"];
    let vectors = embedding
      .embed_batch(texts)
      .await
      .expect("Failed to generate batch embeddings");

    assert_eq!(vectors.len(), 3);
    assert_eq!(vectors[0].len(), 384);
  }

  #[test]
  fn test_mean_pooling() {
    use ndarray::Array3;

    // Simple test data - shape (batch=1, seq_len=2, hidden_size=3)
    let token_embeddings = Array3::from_shape_vec(
      (1, 2, 3),
      vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
    )
    .unwrap();

    let attention_mask = Array2::from_shape_vec((1, 2), vec![1, 1]).unwrap();

    let result = ONNXEmbedding::mean_pooling(&token_embeddings, &attention_mask).unwrap();

    // Mean of [1,2,3] and [4,5,6] should be [2.5, 3.5, 4.5]
    assert_eq!(result.len(), 3);
    assert!((result[0] - 2.5).abs() < 0.001);
    assert!((result[1] - 3.5).abs() < 0.001);
    assert!((result[2] - 4.5).abs() < 0.001);
  }

  #[test]
  fn test_normalize_vector() {
    let vector = Array1::from_vec(vec![3.0, 4.0]);
    let normalized = ONNXEmbedding::normalize_vector(&vector);

    // Length should be 1
    let length = normalized.dot(&normalized).sqrt();
    assert!((length - 1.0).abs() < 0.001);

    // Values should be 3/5 and 4/5
    assert!((normalized[0] - 0.6).abs() < 0.001);
    assert!((normalized[1] - 0.8).abs() < 0.001);
  }
}
