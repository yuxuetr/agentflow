//! Plain text and Markdown document loader
//!
//! Supports loading .txt and .md files from individual files or directories.

use crate::{error::Result, sources::DocumentLoader, types::Document};
use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

/// Text and Markdown document loader
///
/// # Supported Formats
/// - `.txt` - Plain text files
/// - `.md` - Markdown files
///
/// # Example
/// ```rust,no_run
/// use agentflow_rag::sources::{DocumentLoader, text::TextLoader};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let loader = TextLoader;
/// let doc = loader.load(Path::new("document.txt")).await?;
/// # Ok(())
/// # }
/// ```
pub struct TextLoader;

impl TextLoader {
  /// Create a new text loader
  pub fn new() -> Self {
    Self
  }
}

impl Default for TextLoader {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl DocumentLoader for TextLoader {
  async fn load(&self, path: &Path) -> Result<Document> {
    let content = fs::read_to_string(path).await?;
    let mut doc = Document::new(content);

    // Add metadata
    doc.metadata.insert(
      "source".to_string(),
      path.to_string_lossy().to_string().into(),
    );

    if let Some(extension) = path.extension() {
      doc.metadata.insert(
        "file_type".to_string(),
        extension.to_string_lossy().to_string().into(),
      );
    }

    if let Some(file_name) = path.file_name() {
      doc.metadata.insert(
        "file_name".to_string(),
        file_name.to_string_lossy().to_string().into(),
      );
    }

    Ok(doc)
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
        // Check if file has supported extension
        if let Some(ext) = path.extension() {
          let ext_str = ext.to_string_lossy();
          if supported_exts.contains(&ext_str.as_ref()) {
            match self.load(&path).await {
              Ok(doc) => documents.push(doc),
              Err(e) => {
                tracing::warn!("Failed to load {}: {}", path.display(), e);
              }
            }
          }
        }
      } else if path.is_dir() && recursive {
        // Recursively load subdirectory
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
    vec!["txt", "md"]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;
  use tokio::fs;

  #[tokio::test]
  async fn test_load_text_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Hello, world!").await.unwrap();

    let loader = TextLoader::new();
    let doc = loader.load(&file_path).await.unwrap();

    assert_eq!(doc.content, "Hello, world!");
    assert!(doc.metadata.contains_key("source"));
    assert!(doc.metadata.contains_key("file_type"));
  }

  #[tokio::test]
  async fn test_load_markdown_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.md");
    fs::write(&file_path, "# Heading\n\nContent").await.unwrap();

    let loader = TextLoader::new();
    let doc = loader.load(&file_path).await.unwrap();

    assert_eq!(doc.content, "# Heading\n\nContent");
    assert!(doc.metadata.contains_key("file_type"));
  }

  #[tokio::test]
  async fn test_load_directory_non_recursive() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    fs::write(temp_dir.path().join("file1.txt"), "Content 1")
      .await
      .unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "Content 2")
      .await
      .unwrap();
    fs::write(temp_dir.path().join("file3.md"), "# Markdown")
      .await
      .unwrap();
    fs::write(temp_dir.path().join("ignored.rs"), "code")
      .await
      .unwrap();

    let loader = TextLoader::new();
    let docs = loader.load_directory(temp_dir.path(), false).await.unwrap();

    assert_eq!(docs.len(), 3); // Only .txt and .md files
  }

  #[tokio::test]
  async fn test_load_directory_recursive() {
    let temp_dir = TempDir::new().unwrap();

    // Create subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).await.unwrap();

    // Create files in root and subdirectory
    fs::write(temp_dir.path().join("root.txt"), "Root")
      .await
      .unwrap();
    fs::write(subdir.join("sub.txt"), "Sub").await.unwrap();

    let loader = TextLoader::new();
    let docs = loader.load_directory(temp_dir.path(), true).await.unwrap();

    assert_eq!(docs.len(), 2); // Both root and subdirectory files
  }

  #[tokio::test]
  async fn test_supported_extensions() {
    let loader = TextLoader::new();
    let exts = loader.supported_extensions();

    assert_eq!(exts, vec!["txt", "md"]);
  }
}
