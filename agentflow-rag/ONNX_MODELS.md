# ONNX Local Embeddings - Model Setup Guide

This guide explains how to download and prepare ONNX models for local embedding generation with agentflow-rag.

## Overview

Local ONNX embeddings provide:
- ✅ **Zero API costs** - No per-token charges
- ✅ **Complete privacy** - Data never leaves your machine
- ✅ **Offline operation** - No internet required after setup
- ✅ **Fast inference** - Optimized local execution
- ✅ **Model flexibility** - Use any sentence-transformers model

## Prerequisites

### 1. Install Python Dependencies

```bash
pip install optimum[exporters] transformers torch
```

### 2. Enable Feature Flag

Make sure to build with the `local-embeddings` feature:

```bash
cargo build --features local-embeddings
cargo run --example phase7_local_embeddings --features local-embeddings
```

## Quick Start: all-MiniLM-L6-v2

The recommended model for getting started is `all-MiniLM-L6-v2`:
- **Dimension**: 384
- **Size**: ~90 MB
- **Speed**: Very fast (~10ms per text)
- **Quality**: Good for most use cases

### Download and Convert

```bash
# Create models directory
mkdir -p models

# Export to ONNX format
optimum-cli export onnx \
  --model sentence-transformers/all-MiniLM-L6-v2 \
  models/all-MiniLM-L6-v2
```

This will create:
```
models/all-MiniLM-L6-v2/
├── model.onnx           # ONNX model file
├── tokenizer.json       # Tokenizer configuration
├── tokenizer_config.json
├── config.json
└── special_tokens_map.json
```

### Test the Model

```bash
cargo run --example phase7_local_embeddings --features local-embeddings
```

## Alternative Models

### Larger Models (Better Quality)

#### all-mpnet-base-v2
- **Dimension**: 768
- **Size**: ~420 MB
- **Speed**: Moderate (~30ms per text)
- **Best for**: High-quality embeddings

```bash
optimum-cli export onnx \
  --model sentence-transformers/all-mpnet-base-v2 \
  models/all-mpnet-base-v2
```

#### all-MiniLM-L12-v2
- **Dimension**: 384
- **Size**: ~120 MB
- **Speed**: Fast (~15ms per text)
- **Best for**: Balance of speed and quality

```bash
optimum-cli export onnx \
  --model sentence-transformers/all-MiniLM-L12-v2 \
  models/all-MiniLM-L12-v2
```

### Multilingual Models

#### paraphrase-multilingual-MiniLM-L12-v2
- **Dimension**: 384
- **Languages**: 50+
- **Best for**: Non-English or mixed-language content

```bash
optimum-cli export onnx \
  --model sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2 \
  models/paraphrase-multilingual-MiniLM-L12-v2
```

### Specialized Models

#### msmarco-distilbert-base-v4
- **Dimension**: 768
- **Best for**: Information retrieval and search

```bash
optimum-cli export onnx \
  --model sentence-transformers/msmarco-distilbert-base-v4 \
  models/msmarco-distilbert-base-v4
```

## Using Custom Models

### From HuggingFace Hub

Any sentence-transformers model on HuggingFace can be used:

```bash
optimum-cli export onnx \
  --model <huggingface-model-id> \
  models/<model-name>
```

### Using Pre-Converted ONNX Models

Some models are available pre-converted:

```bash
# Download from HuggingFace (if available)
git lfs install
git clone https://huggingface.co/<org>/<model-name>-onnx models/<model-name>
```

## Usage in Code

### Basic Usage

```rust
use agentflow_rag::embeddings::{EmbeddingProvider, ONNXEmbedding};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let embedding = ONNXEmbedding::builder()
    .with_model_path("models/all-MiniLM-L6-v2/model.onnx")
    .with_tokenizer_path("models/all-MiniLM-L6-v2/tokenizer.json")
    .with_model_name("all-MiniLM-L6-v2")
    .with_dimension(384)
    .with_max_length(512)
    .with_normalization(true)  // Enable L2 normalization
    .build()
    .await?;

  // Generate single embedding
  let vector = embedding.embed_text("Hello, world!").await?;
  println!("Embedding dimension: {}", vector.len());

  // Generate batch embeddings
  let texts = vec!["First text", "Second text", "Third text"];
  let vectors = embedding.embed_batch(texts).await?;
  println!("Generated {} embeddings", vectors.len());

  Ok(())
}
```

### Integration with RAG Pipeline

```rust
use agentflow_rag::{
  embeddings::{EmbeddingProvider, ONNXEmbedding},
  vectorstore::QdrantVectorStore,
  sources::Document,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  // Create local embedding provider
  let embedding = ONNXEmbedding::builder()
    .with_model_path("models/all-MiniLM-L6-v2/model.onnx")
    .with_tokenizer_path("models/all-MiniLM-L6-v2/tokenizer.json")
    .with_dimension(384)
    .build()
    .await?;

  // Create vector store with local embeddings
  let vectorstore = QdrantVectorStore::connect("http://localhost:6334")
    .await?
    .with_embedding_provider(embedding);

  // Index documents (no API calls, all local!)
  let docs = vec![
    Document::new("Machine learning is fascinating"),
    Document::new("Deep learning uses neural networks"),
  ];

  vectorstore.add_documents("my_collection", docs).await?;

  // Search with local embeddings
  let results = vectorstore
    .similarity_search("my_collection", "AI and ML", 5)
    .await?;

  println!("Found {} results", results.len());

  Ok(())
}
```

## Model Comparison

| Model | Dimension | Size | Speed | Quality | Use Case |
|-------|-----------|------|-------|---------|----------|
| all-MiniLM-L6-v2 | 384 | 90 MB | ⚡⚡⚡ | ⭐⭐⭐ | General purpose, fast |
| all-MiniLM-L12-v2 | 384 | 120 MB | ⚡⚡ | ⭐⭐⭐⭐ | Balanced |
| all-mpnet-base-v2 | 768 | 420 MB | ⚡ | ⭐⭐⭐⭐⭐ | High quality |
| msmarco-distilbert | 768 | 250 MB | ⚡⚡ | ⭐⭐⭐⭐ | Information retrieval |
| multilingual-MiniLM | 384 | 120 MB | ⚡⚡ | ⭐⭐⭐ | Multilingual |

## Performance Tips

### 1. Model Optimization

The ONNX models are automatically optimized during loading:
- Graph optimization level 3
- 4 intra-op threads for parallel execution

### 2. Batch Processing

Process multiple texts together for better throughput:

```rust
// More efficient than processing individually
let texts = vec!["text1", "text2", "text3", "text4", "text5"];
let embeddings = embedding.embed_batch(texts).await?;
```

### 3. Model Selection

- **Development/Testing**: Use all-MiniLM-L6-v2 (fastest)
- **Production**: Use all-mpnet-base-v2 (best quality)
- **Large scale**: Consider dimension vs. storage tradeoffs

### 4. Caching

The Session is reused across calls for optimal performance. The first call may be slower due to model loading.

## Troubleshooting

### Model Loading Fails

**Error**: `Failed to load ONNX model`

**Solution**: Ensure model file exists and is valid ONNX format:
```bash
ls -lh models/all-MiniLM-L6-v2/model.onnx
```

### Tokenizer Error

**Error**: `Failed to load tokenizer`

**Solution**: Ensure tokenizer.json exists:
```bash
ls -lh models/all-MiniLM-L6-v2/tokenizer.json
```

If missing, re-export with optimum-cli.

### Dimension Mismatch

**Error**: Output dimension doesn't match expected

**Solution**: Check model's actual dimension and update builder:
```rust
.with_dimension(768)  // Match your model's output dimension
```

### Memory Issues

**Error**: Out of memory during inference

**Solution**:
1. Use a smaller model (all-MiniLM-L6-v2)
2. Reduce batch size
3. Process texts sequentially

### Feature Not Enabled

**Error**: `Local embeddings feature not enabled`

**Solution**: Build with feature flag:
```bash
cargo build --features local-embeddings
```

## Advanced Configuration

### Custom Thread Count

Modify `src/embeddings/onnx.rs` to adjust thread count:

```rust
SessionBuilder::new()
  .with_intra_threads(8)  // Change from default 4
  .with_optimization_level(GraphOptimizationLevel::Level3)
  // ...
```

### GPU Acceleration

ONNX Runtime supports GPU execution. To enable:

1. Install CUDA/ROCm ONNX Runtime binaries
2. Modify Cargo.toml to use GPU-enabled ort crate
3. Configure execution provider in SessionBuilder

## References

- [Sentence Transformers](https://www.sbert.net/)
- [ONNX Runtime](https://onnxruntime.ai/)
- [Optimum Documentation](https://huggingface.co/docs/optimum)
- [HuggingFace Models](https://huggingface.co/models?library=sentence-transformers)

## Next Steps

1. ✅ Download and test a model
2. ✅ Run the Phase 7 example
3. ✅ Integrate into your RAG pipeline
4. ✅ Measure performance and quality
5. ✅ Experiment with different models for your use case

For more examples, see `agentflow-rag/examples/phase7_local_embeddings.rs`.
