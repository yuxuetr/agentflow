use agentflow_rag::{
  types::{CollectionConfig, DistanceMetric},
  vectorstore::{QdrantStore, VectorStore},
};
use anyhow::{Context, Result};
use colored::*;

/// Execute the RAG collections command (create, delete, list, stats)
pub async fn execute(
  qdrant_url: String,
  operation: String,
  collection: Option<String>,
  vector_size: Option<usize>,
  distance: Option<String>,
) -> Result<()> {
  // Connect to Qdrant
  let store = QdrantStore::new(&qdrant_url)
    .await
    .context("Failed to connect to Qdrant")?;

  println!("{}", "✅ Connected to Qdrant".green());

  match operation.as_str() {
    "create" => {
      let collection_name = collection.context("Collection name required for create operation")?;

      let dimension = vector_size.unwrap_or(1536); // Default to text-embedding-3-small size
      let dist_metric = match distance.as_deref().unwrap_or("cosine") {
        "cosine" => DistanceMetric::Cosine,
        "euclidean" => DistanceMetric::Euclidean,
        "dot" => DistanceMetric::Dot,
        other => anyhow::bail!(
          "Invalid distance metric: {}. Must be: cosine, euclidean, or dot",
          other
        ),
      };

      println!(
        "{}",
        format!("🔨 Creating collection '{}'", collection_name)
          .bold()
          .blue()
      );
      println!(
        "{}",
        format!("   Dimension: {}, Distance: {:?}", dimension, dist_metric).dimmed()
      );

      let config = CollectionConfig {
        dimension,
        distance: dist_metric,
        index_config: None,
      };

      store
        .create_collection(&collection_name, config)
        .await
        .context("Failed to create collection")?;

      println!(
        "{}",
        format!("✅ Collection '{}' created successfully!", collection_name)
          .bold()
          .green()
      );
    }

    "delete" => {
      let collection_name = collection.context("Collection name required for delete operation")?;

      println!(
        "{}",
        format!("🗑️  Deleting collection '{}'", collection_name)
          .bold()
          .red()
      );

      store
        .delete_collection(&collection_name)
        .await
        .context("Failed to delete collection")?;

      println!(
        "{}",
        format!("✅ Collection '{}' deleted successfully!", collection_name)
          .bold()
          .green()
      );
    }

    "list" => {
      println!("{}", "📋 Listing collections...".bold().blue());

      let collections = store
        .list_collections()
        .await
        .context("Failed to list collections")?;

      if collections.is_empty() {
        println!("{}", "⚠️  No collections found".yellow());
        return Ok(());
      }

      println!();
      println!(
        "{}",
        format!("Available Collections ({}):", collections.len())
          .bold()
          .green()
      );
      println!();

      for (i, name) in collections.iter().enumerate() {
        println!("  {}. {}", i + 1, name.cyan().bold());
      }

      println!();
      println!(
        "{}",
        format!("Total: {} collections", collections.len())
          .bold()
          .green()
      );
    }

    "stats" => {
      let collection_name = collection.context("Collection name required for stats operation")?;

      println!(
        "{}",
        format!("📊 Checking collection '{}'", collection_name)
          .bold()
          .blue()
      );

      // For now, just check if collection exists by trying to list collections
      let collections = store
        .list_collections()
        .await
        .context("Failed to list collections")?;

      if collections.contains(&collection_name) {
        println!();
        println!(
          "{}",
          format!("✅ Collection '{}' exists", collection_name)
            .bold()
            .green()
        );
        println!(
          "{}",
          "Note: Detailed stats coming in future version".dimmed()
        );
      } else {
        println!();
        println!(
          "{}",
          format!("❌ Collection '{}' not found", collection_name)
            .bold()
            .red()
        );
      }
    }

    _ => {
      anyhow::bail!(
        "Invalid operation: {}. Must be: create, delete, list, or stats",
        operation
      );
    }
  }

  Ok(())
}
