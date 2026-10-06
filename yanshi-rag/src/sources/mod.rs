//! Data source loaders for documents

use crate::{error::Result, types::Document};
use std::path::Path;

pub mod csv;
pub mod preprocessing;
pub mod text;

#[cfg(feature = "pdf")]
pub mod pdf;

#[cfg(feature = "html")]
pub mod html;

/// Document loader trait
#[async_trait::async_trait]
pub trait DocumentLoader: Send + Sync {
  /// Load a document from a path
  async fn load(&self, path: &Path) -> Result<Document>;

  /// Load multiple documents from a directory
  async fn load_directory(&self, dir: &Path, recursive: bool) -> Result<Vec<Document>>;

  /// Supported file extensions
  fn supported_extensions(&self) -> Vec<&'static str>;
}
