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
  /// Q3.9.6: hard cap on the PDF file size in bytes. `pdf_extract`
  /// loads the entire byte buffer into memory and parses it eagerly,
  /// so a single user-uploaded multi-GiB PDF will OOM the process
  /// before any text extraction starts. The 50 MiB default is well
  /// above any realistic research-paper / report size and well
  /// below the typical container memory budget. `None` disables
  /// the cap (opt-in for trusted single-tenant pipelines).
  max_bytes: Option<u64>,
}

/// Q3.9.6: default `max_bytes` cap for the PDF loader.
/// Mirrors the 50 MiB ceiling used by typical document-ingest
/// pipelines (Confluence, Notion, Google Drive API quotas).
pub const DEFAULT_PDF_MAX_BYTES: u64 = 50 * 1024 * 1024;

impl PdfLoader {
  /// Create a new PDF loader with a 50 MiB file-size cap. Pass
  /// `Some(N)` to [`Self::with_max_bytes`] to override, or `None`
  /// to disable.
  pub fn new() -> Self {
    Self {
      include_page_numbers: false,
      max_bytes: Some(DEFAULT_PDF_MAX_BYTES),
    }
  }

  /// Include page numbers in the extracted content
  pub fn with_page_numbers(mut self) -> Self {
    self.include_page_numbers = true;
    self
  }

  /// Q3.9.6: override the file-size cap. Pass `Some(N)` to lower /
  /// raise the limit, or `None` to disable entirely (trusted
  /// pipelines only — `pdf_extract` reads the whole file into
  /// memory).
  pub fn with_max_bytes(mut self, max_bytes: Option<u64>) -> Self {
    self.max_bytes = max_bytes;
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
    // Q3.9.6: stat the file BEFORE reading so a gigabyte payload
    // fails fast instead of allocating its way to OOM. We check
    // again after read in case the file grew between stat and read
    // (rare but possible with a producer that's still writing).
    if let Some(max) = self.max_bytes {
      let metadata =
        fs::metadata(path)
          .await
          .map_err(|e| crate::error::RAGError::DocumentError {
            message: format!("Failed to stat PDF {}: {}", path.display(), e),
          })?;
      if metadata.len() > max {
        return Err(crate::error::RAGError::DocumentError {
          message: format!(
            "PDF {} is {} bytes which exceeds the configured max_bytes={}; \
             raise the limit with PdfLoader::with_max_bytes or `None` to disable",
            path.display(),
            metadata.len(),
            max
          ),
        });
      }
    }

    // Read PDF file
    let bytes = fs::read(path).await?;
    if let Some(max) = self.max_bytes
      && bytes.len() as u64 > max
    {
      return Err(crate::error::RAGError::DocumentError {
        message: format!(
          "PDF {} grew to {} bytes during read which exceeds max_bytes={}",
          path.display(),
          bytes.len(),
          max
        ),
      });
    }

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

  /// Q3.9.6 regression — files bigger than the configured cap must
  /// surface a descriptive error instead of being loaded into
  /// memory. Uses a non-PDF blob (the size check fires before any
  /// PDF parsing) so the test doesn't need a real giant PDF
  /// fixture.
  #[tokio::test]
  async fn pdf_loader_rejects_files_above_max_bytes() {
    use tempfile::TempDir;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("oversize.pdf");
    // 2 KiB of bytes — bigger than our 1 KiB test cap.
    fs::write(&path, vec![0u8; 2048]).await.unwrap();

    let loader = PdfLoader::new().with_max_bytes(Some(1024));
    let err = loader.load(&path).await.unwrap_err();
    let msg = err.to_string();
    assert!(
      msg.contains("exceeds the configured max_bytes") || msg.contains("exceeds max_bytes"),
      "error must explain the size-cap rejection; got: {msg}"
    );
    assert!(
      msg.contains("with_max_bytes") || msg.contains("max_bytes="),
      "error must point operators at the override; got: {msg}"
    );
  }

  #[test]
  fn pdf_loader_default_has_50_mib_cap() {
    let loader = PdfLoader::default();
    assert_eq!(loader.max_bytes, Some(DEFAULT_PDF_MAX_BYTES));
    assert_eq!(DEFAULT_PDF_MAX_BYTES, 50 * 1024 * 1024);
  }

  #[test]
  fn pdf_loader_with_max_bytes_none_disables_cap() {
    let loader = PdfLoader::new().with_max_bytes(None);
    assert!(loader.max_bytes.is_none());
  }

  // Note: Integration tests for actual PDF loading would require test PDF files
}
