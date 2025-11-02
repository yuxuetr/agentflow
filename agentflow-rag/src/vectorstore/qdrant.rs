//! Qdrant vector store implementation

use crate::{
  error::{RAGError, Result},
  types::{CollectionConfig, DistanceMetric, Document, Filter, MetadataValue, SearchResult},
  vectorstore::{CollectionStats, VectorStore},
};
use async_trait::async_trait;
use qdrant_client::{
  qdrant::{
    vectors_config::Config, Distance, PointId, PointStruct,
    Value as QdrantValue, VectorParams, VectorsConfig, SearchPointsBuilder, CreateCollectionBuilder,
    UpsertPointsBuilder, DeletePointsBuilder,
  },
  Qdrant,
};
use std::collections::HashMap;

/// Qdrant vector store implementation
pub struct QdrantStore {
  client: Qdrant,
}

impl QdrantStore {
  /// Create a new Qdrant store
  pub async fn new(url: impl Into<String>) -> Result<Self> {
    let url = url.into();
    tracing::info!("Connecting to Qdrant at: {}", url);

    let client = Qdrant::from_url(&url)
      .build()
      .map_err(|e| RAGError::connection(format!("Failed to connect to Qdrant: {}", e)))?;

    Ok(Self { client })
  }

  /// Convert our DistanceMetric to Qdrant Distance
  fn convert_distance(metric: DistanceMetric) -> Distance {
    match metric {
      DistanceMetric::Cosine => Distance::Cosine,
      DistanceMetric::Euclidean => Distance::Euclid,
      DistanceMetric::Dot => Distance::Dot,
    }
  }

  /// Convert Document to Qdrant PointStruct
  fn document_to_point(doc: &Document) -> Result<PointStruct> {
    let embedding = doc
      .embedding
      .as_ref()
      .ok_or_else(|| RAGError::document("Document must have embedding"))?;

    // Convert metadata to Qdrant payload
    let mut payload = HashMap::new();
    payload.insert("content".to_string(), QdrantValue::from(doc.content.clone()));

    for (key, value) in &doc.metadata {
      let qdrant_value = match value {
        MetadataValue::String(s) => QdrantValue::from(s.clone()),
        MetadataValue::Integer(i) => QdrantValue::from(*i),
        MetadataValue::Float(f) => QdrantValue::from(*f),
        MetadataValue::Boolean(b) => QdrantValue::from(*b),
        MetadataValue::Array(arr) => {
          QdrantValue::from(arr.clone())
        }
      };
      payload.insert(key.clone(), qdrant_value);
    }

    Ok(PointStruct::new(
      doc.id.clone(),
      embedding.clone(),
      payload,
    ))
  }

  /// Convert Filter to Qdrant Filter
  ///
  /// Note: This is a simplified implementation for Phase 2.
  /// Full filter support with the new Qdrant 1.15 API will be added in a future phase.
  fn convert_filter(_filter: &Filter) -> Result<qdrant_client::qdrant::Filter> {
    // TODO: Implement filter conversion using the new Qdrant 1.15 filters API
    // For now, return empty filter
    tracing::warn!("Filter conversion not yet implemented for Qdrant 1.15 API");
    Ok(qdrant_client::qdrant::Filter::default())
  }
}

#[async_trait]
impl VectorStore for QdrantStore {
  async fn create_collection(&self, name: &str, config: CollectionConfig) -> Result<()> {
    tracing::info!(
      "Creating collection '{}' with dimension {}",
      name,
      config.dimension
    );

    let vectors_config = VectorsConfig {
      config: Some(Config::Params(VectorParams {
        size: config.dimension as u64,
        distance: Self::convert_distance(config.distance).into(),
        hnsw_config: config.index_config.as_ref().and_then(|ic| {
          ic.hnsw.as_ref().map(|hnsw| {
            qdrant_client::qdrant::HnswConfigDiff {
              m: Some(hnsw.m as u64),
              ef_construct: Some(hnsw.ef_construct as u64),
              ..Default::default()
            }
          })
        }),
        ..Default::default()
      })),
    };

    self
      .client
      .create_collection(
        CreateCollectionBuilder::new(name)
          .vectors_config(vectors_config)
      )
      .await
      .map_err(|e| RAGError::vector_store(format!("Failed to create collection: {}", e)))?;

    tracing::info!("Successfully created collection '{}'", name);
    Ok(())
  }

  async fn delete_collection(&self, name: &str) -> Result<()> {
    tracing::info!("Deleting collection '{}'", name);

    self
      .client
      .delete_collection(name)
      .await
      .map_err(|e| RAGError::vector_store(format!("Failed to delete collection: {}", e)))?;

    tracing::info!("Successfully deleted collection '{}'", name);
    Ok(())
  }

  async fn collection_exists(&self, name: &str) -> Result<bool> {
    let collections = self.client.list_collections().await.map_err(|e| {
      RAGError::vector_store(format!("Failed to list collections: {}", e))
    })?;

    Ok(collections
      .collections
      .iter()
      .any(|c| c.name == name))
  }

  async fn list_collections(&self) -> Result<Vec<String>> {
    let collections = self.client.list_collections().await.map_err(|e| {
      RAGError::vector_store(format!("Failed to list collections: {}", e))
    })?;

    Ok(
      collections
        .collections
        .into_iter()
        .map(|c| c.name)
        .collect(),
    )
  }

  async fn add_documents(&self, collection: &str, docs: Vec<Document>) -> Result<Vec<String>> {
    if docs.is_empty() {
      return Ok(Vec::new());
    }

    tracing::info!("Adding {} documents to collection '{}'", docs.len(), collection);

    // Convert documents to points
    let points: Vec<PointStruct> = docs
      .iter()
      .map(Self::document_to_point)
      .collect::<Result<Vec<_>>>()?;

    let ids: Vec<String> = docs.iter().map(|d| d.id.clone()).collect();

    // Upsert points to Qdrant
    self
      .client
      .upsert_points(UpsertPointsBuilder::new(collection, points))
      .await
      .map_err(|e| RAGError::vector_store(format!("Failed to add documents: {}", e)))?;

    tracing::info!("Successfully added {} documents", ids.len());
    Ok(ids)
  }

  async fn delete_documents(&self, collection: &str, ids: Vec<String>) -> Result<()> {
    if ids.is_empty() {
      return Ok(());
    }

    tracing::info!("Deleting {} documents from collection '{}'", ids.len(), collection);

    let point_ids: Vec<PointId> = ids
      .into_iter()
      .map(|id| PointId::from(id))
      .collect();

    self
      .client
      .delete_points(DeletePointsBuilder::new(collection).points(point_ids))
      .await
      .map_err(|e| RAGError::vector_store(format!("Failed to delete documents: {}", e)))?;

    tracing::info!("Successfully deleted documents");
    Ok(())
  }

  async fn similarity_search(
    &self,
    _collection: &str,
    _query: &str,
    _top_k: usize,
    _filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>> {
    // For now, return an error as we need an embedding provider
    // This will be implemented in Phase 3 when we have embeddings
    tracing::warn!("similarity_search requires embedding provider (Phase 3)");
    Err(RAGError::retrieval(
      "Similarity search requires embedding provider. Use similarity_search_by_vector instead.",
    ))
  }

  async fn similarity_search_by_vector(
    &self,
    collection: &str,
    vector: Vec<f32>,
    top_k: usize,
    filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>> {
    tracing::debug!(
      "Searching collection '{}' for top {} results",
      collection,
      top_k
    );

    let mut search_builder = SearchPointsBuilder::new(collection, vector, top_k as u64)
      .with_payload(true);

    if let Some(f) = filter {
      let qdrant_filter = Self::convert_filter(&f)?;
      search_builder = search_builder.filter(qdrant_filter);
    }

    let search_result = self
      .client
      .search_points(search_builder)
      .await
      .map_err(|e| RAGError::retrieval(format!("Search failed: {}", e)))?;

    // Convert Qdrant results to our SearchResult type
    let results: Vec<SearchResult> = search_result
      .result
      .into_iter()
      .map(|point| {
        let id = match point.id {
          Some(PointId { point_id_options: Some(options) }) => match options {
            qdrant_client::qdrant::point_id::PointIdOptions::Uuid(uuid) => uuid,
            qdrant_client::qdrant::point_id::PointIdOptions::Num(num) => num.to_string(),
          },
          _ => String::new(),
        };

        let score = point.score;

        let mut content = String::new();
        let mut metadata = HashMap::new();

        for (key, value) in point.payload {
          if key == "content" {
            if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = value.kind {
              content = s;
            }
          } else {
            // Convert Qdrant value to our MetadataValue
            if let Some(kind) = value.kind {
              let meta_value = match kind {
                qdrant_client::qdrant::value::Kind::StringValue(s) => {
                  MetadataValue::String(s)
                }
                qdrant_client::qdrant::value::Kind::IntegerValue(i) => {
                  MetadataValue::Integer(i)
                }
                qdrant_client::qdrant::value::Kind::DoubleValue(f) => {
                  MetadataValue::Float(f)
                }
                qdrant_client::qdrant::value::Kind::BoolValue(b) => {
                  MetadataValue::Boolean(b)
                }
                qdrant_client::qdrant::value::Kind::ListValue(list) => {
                  let strings: Vec<String> = list
                    .values
                    .into_iter()
                    .filter_map(|v| {
                      if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = v.kind
                      {
                        Some(s)
                      } else {
                        None
                      }
                    })
                    .collect();
                  MetadataValue::Array(strings)
                }
                _ => continue,
              };
              metadata.insert(key, meta_value);
            }
          }
        }

        SearchResult {
          id,
          content,
          score,
          metadata,
        }
      })
      .collect();

    tracing::debug!("Found {} results", results.len());
    Ok(results)
  }

  async fn get_collection_stats(&self, collection: &str) -> Result<CollectionStats> {
    let info = self
      .client
      .collection_info(collection)
      .await
      .map_err(|e| RAGError::vector_store(format!("Failed to get collection info: {}", e)))?;

    let points_count = info.result.as_ref().and_then(|r| r.points_count).unwrap_or(0);

    let config = info
      .result
      .as_ref()
      .and_then(|r| r.config.as_ref())
      .and_then(|c| c.params.as_ref())
      .and_then(|p| p.vectors_config.as_ref());

    let dimension = if let Some(vectors_config) = config {
      if let Some(Config::Params(params)) = vectors_config.config {
        params.size as usize
      } else {
        0
      }
    } else {
      0
    };

    Ok(CollectionStats {
      name: collection.to_string(),
      document_count: points_count as usize,
      dimension,
      index_size_bytes: 0, // Qdrant doesn't provide this directly
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::HNSWConfig;

  #[test]
  fn test_distance_conversion() {
    assert_eq!(
      QdrantStore::convert_distance(DistanceMetric::Cosine),
      Distance::Cosine
    );
    assert_eq!(
      QdrantStore::convert_distance(DistanceMetric::Euclidean),
      Distance::Euclid
    );
    assert_eq!(
      QdrantStore::convert_distance(DistanceMetric::Dot),
      Distance::Dot
    );
  }

  #[test]
  fn test_document_to_point() {
    let doc = Document::new("test content")
      .with_embedding(vec![0.1, 0.2, 0.3])
      .with_metadata("source".to_string(), "test".into());

    let point = QdrantStore::document_to_point(&doc).unwrap();
    // Verify point was created with vectors
    assert!(point.vectors.is_some());
    // Verify payload contains content
    assert!(point.payload.contains_key("content"));
    // Verify payload contains metadata
    assert!(point.payload.contains_key("source"));
  }

  #[test]
  fn test_document_without_embedding_fails() {
    let doc = Document::new("test content");
    let result = QdrantStore::document_to_point(&doc);
    assert!(result.is_err());
  }

  // Integration tests require running Qdrant server
  #[tokio::test]
  #[ignore]
  async fn test_qdrant_integration() {
    let store = QdrantStore::new("http://localhost:6334").await.unwrap();

    let config = CollectionConfig {
      dimension: 384,
      distance: DistanceMetric::Cosine,
      index_config: Some(crate::types::IndexConfig {
        hnsw: Some(HNSWConfig::default()),
      }),
    };

    // Create collection
    store
      .create_collection("test_collection", config)
      .await
      .unwrap();

    // Verify it exists
    assert!(store.collection_exists("test_collection").await.unwrap());

    // Add documents
    let doc = Document::new("Test document")
      .with_embedding(vec![0.1; 384])
      .with_metadata("test".to_string(), "value".into());

    let ids = store
      .add_documents("test_collection", vec![doc])
      .await
      .unwrap();
    assert_eq!(ids.len(), 1);

    // Search
    let results = store
      .similarity_search_by_vector("test_collection", vec![0.1; 384], 5, None)
      .await
      .unwrap();
    assert!(!results.is_empty());

    // Cleanup
    store.delete_collection("test_collection").await.unwrap();
  }
}
