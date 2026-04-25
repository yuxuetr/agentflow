//! PDF document loader
//!
//! Supports loading PDF files and extracting text content.

use crate::{error::Result, sources::DocumentLoader, types::Document};
use async_trait::async_trait;
use pdf_extract::extract_text_from_mem;
use std::path::Path;
use tokio::fs;

/// PDF document loader
///
/// Extracts text content from PDF files using the pdf-extract library.
///
/// # Example
/// ```rust,no_run
/// use agentflow_rag::sources::{DocumentLoader, pdf::PdfLoader};
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let loader = PdfLoader::new();
/// let doc = loader.load(Path::new("document.pdf")).await?;
/// # Ok(())
/// # }
/// ```
pub struct PdfLoader {
  /// Include page numbers in content
  include_page_numbers: bool,
}

impl PdfLoader {
  /// Create a new PDF loader
  pub fn new() -> Self {
    Self {
      include_page_numbers: false,
    }
  }

  /// Include page numbers in the extracted content
  pub fn with_page_numbers(mut self) -> Self {
    self.include_page_numbers = true;
    self
  }
}

impl Default for PdfLoader {
  fn default() -> Self {
    Self::new()
  }
}

#[async_trait]
impl DocumentLoader for PdfLoader {
  async fn load(&self, path: &Path) -> Result<Document> {
    // Read PDF file
    let bytes = fs::read(path).await?;

    // Extract text from PDF
    let text =
      extract_text_from_mem(&bytes).map_err(|e| crate::error::RAGError::DocumentError {
        message: format!("Failed to extract text from PDF: {}", e),
      })?;

    let mut doc = Document::new(text);

    // Add metadata
    doc.metadata.insert(
      "source".to_string(),
      path.to_string_lossy().to_string().into(),
    );
    doc
      .metadata
      .insert("file_type".to_string(), "pdf".to_string().into());

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
        if let Some(ext) = path.extension() {
          let ext_str = ext.to_string_lossy();
          if supported_exts.contains(&ext_str.as_ref()) {
            match self.load(&path).await {
              Ok(doc) => documents.push(doc),
              Err(e) => {
                tracing::warn!("Failed to load PDF {}: {}", path.display(), e);
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
    vec!["pdf"]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_new_loader() {
    let loader = PdfLoader::new();
    assert!(!loader.include_page_numbers);
  }

  #[test]
  fn test_with_page_numbers() {
    let loader = PdfLoader::new().with_page_numbers();
    assert!(loader.include_page_numbers);
  }

  #[test]
  fn test_supported_extensions() {
    let loader = PdfLoader::new();
    let exts = loader.supported_extensions();
    assert_eq!(exts, vec!["pdf"]);
  }

  // Note: Integration tests for actual PDF loading would require test PDF files
}
