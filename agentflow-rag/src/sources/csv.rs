//! CSV and JSON document loader
//!
//! Supports loading structured data from CSV and JSON files.

use crate::{error::Result, sources::DocumentLoader, types::Document};
use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

/// CSV and JSON document loader
///
/// # Supported Formats
/// - `.csv` - Comma-separated values
/// - `.json` - JSON files (both single objects and arrays)
///
/// # CSV Format
/// Each row in the CSV becomes a separate document. The loader uses the header
/// row to create field names.
///
/// # JSON Format
/// - Single object: Creates one document
/// - Array of objects: Creates one document per object
///
/// # Example
/// ```rust,no_run
/// use agentflow_rag::sources::{DocumentLoader, csv::CsvLoader};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let loader = CsvLoader::new();
/// let docs = loader.load(Path::new("data.csv")).await?;
/// # Ok(())
/// # }
/// ```
pub struct CsvLoader {
  /// Field to use as main content (default: first field)
  content_field: Option<String>,
}

impl CsvLoader {
  /// Create a new CSV/JSON loader
  pub fn new() -> Self {
    Self {
      content_field: None,
    }
  }

  /// Set the field to use as document content
  pub fn with_content_field(mut self, field: impl Into<String>) -> Self {
    self.content_field = Some(field.into());
    self
  }

  /// Load a CSV file
  async fn load_csv(&self, path: &Path) -> Result<Vec<Document>> {
    let content = fs::read_to_string(path).await?;
    let mut reader = csv::Reader::from_reader(content.as_bytes());

    let headers = reader.headers()?.clone();
    let mut documents = Vec::new();

    for (row_idx, result) in reader.records().enumerate() {
      let record = result?;

      // Determine content field
      let content = if let Some(ref field_name) = self.content_field {
        // Use specified field
        if let Some(pos) = headers.iter().position(|h| h == field_name) {
          record.get(pos).unwrap_or("").to_string()
        } else {
          // Field not found, use first column
          record.get(0).unwrap_or("").to_string()
        }
      } else {
        // Use first column
        record.get(0).unwrap_or("").to_string()
      };

      let mut doc = Document::new(content);

      // Add all fields as metadata
      for (i, header) in headers.iter().enumerate() {
        if let Some(value) = record.get(i) {
          doc
            .metadata
            .insert(header.to_string(), value.to_string().into());
        }
      }

      // Add source metadata
      doc.metadata.insert(
        "source".to_string(),
        path.to_string_lossy().to_string().into(),
      );
      doc
        .metadata
        .insert("file_type".to_string(), "csv".to_string().into());
      doc
        .metadata
        .insert("row_index".to_string(), (row_idx as i64).into());

      documents.push(doc);
    }

    Ok(documents)
  }

  /// Load a JSON file
  async fn load_json(&self, path: &Path) -> Result<Vec<Document>> {
    let content = fs::read_to_string(path).await?;
    let value: serde_json::Value = serde_json::from_str(&content)?;

    let mut documents = Vec::new();

    match value {
      serde_json::Value::Array(array) => {
        // Array of objects
        for (idx, item) in array.into_iter().enumerate() {
          if let Some(doc) = self.json_value_to_document(item, path, Some(idx)) {
            documents.push(doc);
          }
        }
      }
      serde_json::Value::Object(_) => {
        // Single object
        if let Some(doc) = self.json_value_to_document(value, path, None) {
          documents.push(doc);
        }
      }
      _ => {
        // Primitive value
        let doc = Document::new(value.to_string());
        documents.push(doc);
      }
    }

    Ok(documents)
  }

  /// Convert a JSON value to a Document
  fn json_value_to_document(
    &self,
    value: serde_json::Value,
    path: &Path,
    index: Option<usize>,
  ) -> Option<Document> {
    let obj = value.as_object()?;

    // Determine content
    let content = if let Some(ref field_name) = self.content_field {
      obj
        .get(field_name)
        .map(|v| v.to_string())
        .unwrap_or_default()
    } else {
      // Use first field value or serialize the whole object
      obj
        .values()
        .next()
        .map(|v| v.to_string())
        .unwrap_or_else(|| serde_json::to_string(obj).unwrap_or_default())
    };

    let mut doc = Document::new(content);

    // Add all fields as metadata
    for (key, val) in obj.iter() {
      let metadata_value = match val {
        serde_json::Value::String(s) => s.clone().into(),
        serde_json::Value::Number(n) => {
          if let Some(i) = n.as_i64() {
            i.into()
          } else if let Some(f) = n.as_f64() {
            f.into()
          } else {
            val.to_string().into()
          }
        }
        serde_json::Value::Bool(b) => (*b).into(),
        serde_json::Value::Array(arr) => {
          let strings: Vec<String> = arr.iter().map(|v| v.to_string()).collect();
          crate::types::MetadataValue::Array(strings)
        }
        _ => val.to_string().into(),
      };

      doc.metadata.insert(key.clone(), metadata_value);
    }

    // Add source metadata
    doc.metadata.insert(
      "source".to_string(),
      path.to_string_lossy().to_string().into(),
    );
    doc
      .metadata
      .insert("file_type".to_string(), "json".to_string().into());

    if let Some(idx) = index {
      doc
        .metadata
        .insert("array_index".to_string(), (idx as i64).into());
    }

    Some(doc)
  }
}

impl Default for CsvLoader {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl DocumentLoader for CsvLoader {
  async fn load(&self, path: &Path) -> Result<Document> {
    // Load and return first document
    let docs = if let Some(ext) = path.extension() {
      match ext.to_string_lossy().as_ref() {
        "csv" => self.load_csv(path).await?,
        "json" => self.load_json(path).await?,
        _ => {
          return Err(crate::error::RAGError::DocumentError {
            message: format!("Unsupported file extension: {:?}", ext),
          })
        }
      }
    } else {
      return Err(crate::error::RAGError::DocumentError {
        message: "File has no extension".to_string(),
      });
    };

    docs
      .into_iter()
      .next()
      .ok_or_else(|| crate::error::RAGError::DocumentError {
        message: "No documents loaded from file".to_string(),
      })
  }

  async fn load_directory(&self, dir: &Path, recursive: bool) -> Result<Vec<Document>> {
    let mut documents = Vec::new();
    let supported_exts = self.supported_extensions();

    if !dir.is_dir() {
      return Err(crate::error::RAGError::DocumentError {
        message: format!("Path is not a directory: {}", dir.display()),
      });
    }

    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
      let path = entry.path();

      if path.is_file() {
        if let Some(ext) = path.extension() {
          let ext_str = ext.to_string_lossy();
          if supported_exts.contains(&ext_str.as_ref()) {
            let file_docs = match ext_str.as_ref() {
              "csv" => self.load_csv(&path).await,
              "json" => self.load_json(&path).await,
              _ => continue,
            };

            match file_docs {
              Ok(mut docs) => documents.append(&mut docs),
              Err(e) => {
                tracing::warn!("Failed to load {}: {}", path.display(), e);
              }
            }
          }
        }
      } else if path.is_dir() && recursive {
        match self.load_directory(&path, recursive).await {
          Ok(mut subdocs) => documents.append(&mut subdocs),
          Err(e) => {
            tracing::warn!("Failed to load directory {}: {}", path.display(), e);
          }
        }
      }
    }

    Ok(documents)
  }

  fn supported_extensions(&self) -> Vec<&'static str> {
    vec!["csv", "json"]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  #[tokio::test]
  async fn test_load_csv() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.csv");

    let csv_content = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    fs::write(&file_path, csv_content).await.unwrap();

    let loader = CsvLoader::new();
    let docs = loader.load_csv(&file_path).await.unwrap();

    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].content, "Alice");
    assert!(docs[0].metadata.contains_key("name"));
    assert!(docs[0].metadata.contains_key("age"));
  }

  #[tokio::test]
  async fn test_load_json_array() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.json");

    let json_content = r#"[
      {"name": "Alice", "age": 30},
      {"name": "Bob", "age": 25}
    ]"#;
    fs::write(&file_path, json_content).await.unwrap();

    let loader = CsvLoader::new();
    let docs = loader.load_json(&file_path).await.unwrap();

    assert_eq!(docs.len(), 2);
    assert!(docs[0].metadata.contains_key("name"));
  }

  #[tokio::test]
  async fn test_load_json_object() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.json");

    let json_content = r#"{"title": "Document", "content": "Text"}"#;
    fs::write(&file_path, json_content).await.unwrap();

    let loader = CsvLoader::new();
    let docs = loader.load_json(&file_path).await.unwrap();

    assert_eq!(docs.len(), 1);
  }

  #[tokio::test]
  async fn test_csv_with_content_field() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.csv");

    let csv_content = "id,title,body\n1,Title1,Body1\n2,Title2,Body2";
    fs::write(&file_path, csv_content).await.unwrap();

    let loader = CsvLoader::new().with_content_field("body");
    let docs = loader.load_csv(&file_path).await.unwrap();

    assert_eq!(docs[0].content, "Body1");
    assert_eq!(docs[1].content, "Body2");
  }

  #[tokio::test]
  async fn test_supported_extensions() {
    let loader = CsvLoader::new();
    let exts = loader.supported_extensions();

    assert_eq!(exts, vec!["csv", "json"]);
  }
}
