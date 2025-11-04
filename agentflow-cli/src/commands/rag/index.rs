use agentflow_rag::{
  embeddings::{EmbeddingProvider, OpenAIEmbedding},
  types::{Document, MetadataValue},
  vectorstore::{QdrantStore, VectorStore},
};
use anyhow::{Context, Result};
use colored::*;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Execute the RAG index command
pub async fn execute(
  qdrant_url: String,
  collection: String,
  documents_json: String,
  embedding_model: String,
) -> Result<()> {
  println!(
    "{}",
    format!("📚 Indexing documents into collection '{}'", collection)
      .bold()
      .blue()
  );

  // Parse documents from JSON
  let documents_value: Value = serde_json::from_str(&documents_json)
    .context("Failed to parse documents JSON. Expected array of {content, metadata?}")?;

  let documents_array = documents_value
    .as_array()
    .context("Documents must be a JSON array")?;

  let mut documents: Vec<Document> = Vec::new();

  for (idx, doc_val) in documents_array.iter().enumerate() {
    // Get or generate document ID
    let id = doc_val
      .get("id")
      .and_then(|v| v.as_str())
      .map(|s| s.to_string())
      .unwrap_or_else(|| Uuid::new_v4().to_string());

    let content = doc_val
      .get("content")
      .and_then(|v| v.as_str())
      .context(format!("Document {} missing 'content' field", idx))?
      .to_string();

    let metadata = if let Some(meta_val) = doc_val.get("metadata") {
      if let Some(meta_obj) = meta_val.as_object() {
        let mut meta_map = HashMap::new();
        for (key, value) in meta_obj {
          // Convert serde_json::Value to MetadataValue
          let meta_value = match value {
            Value::String(s) => MetadataValue::String(s.clone()),
            Value::Number(n) => {
              if let Some(i) = n.as_i64() {
                MetadataValue::Integer(i)
              } else if let Some(f) = n.as_f64() {
                MetadataValue::Float(f)
              } else {
                MetadataValue::String(n.to_string())
              }
            }
            Value::Bool(b) => MetadataValue::Boolean(*b),
            Value::Array(arr) => {
              let strings: Vec<String> = arr
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect();
              MetadataValue::Array(strings)
            }
            _ => MetadataValue::String(value.to_string()),
          };
          meta_map.insert(key.clone(), meta_value);
        }
        meta_map
      } else {
        HashMap::new()
      }
    } else {
      HashMap::new()
    };

    documents.push(Document {
      id,
      content,
      metadata,
      embedding: None,
    });
  }

  println!(
    "{}",
    format!("   Parsed {} documents", documents.len()).dimmed()
  );

  // Connect to Qdrant
  let store = QdrantStore::new(&qdrant_url)
    .await
    .context("Failed to connect to Qdrant")?;

  println!("{}", "✅ Connected to Qdrant".green());

  // Initialize embedding provider
  let embedding = OpenAIEmbedding::new(&embedding_model)?;

  println!(
    "{}",
    format!("   Using embedding model: {}", embedding_model).dimmed()
  );

  // Generate embeddings for all documents
  println!("{}", "🔄 Generating embeddings...".yellow());
  let doc_count = documents.len();
  let texts: Vec<&str> = documents.iter().map(|d| d.content.as_str()).collect();

  let embeddings = embedding
    .embed_batch(texts)
    .await
    .context("Failed to generate embeddings")?;

  println!("{}", "✅ Generated embeddings".green());

  // Add embeddings to documents
  for (doc, embedding_vec) in documents.iter_mut().zip(embeddings.iter()) {
    doc.embedding = Some(embedding_vec.clone());
  }

  // Add documents to Qdrant
  println!("{}", "🔄 Adding documents to Qdrant...".yellow());
  store
    .add_documents(&collection, documents)
    .await
    .context("Failed to add documents to Qdrant")?;

  println!(
    "{}",
    format!("✅ Successfully indexed {} documents!", doc_count)
      .bold()
      .green()
  );

  Ok(())
}
