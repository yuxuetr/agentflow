//! Plain text document loader

use crate::{error::Result, sources::DocumentLoader, types::Document};
use async_trait::async_trait;
use std::path::Path;

pub struct TextLoader;

#[async_trait]
impl DocumentLoader for TextLoader {
  async fn load(&self, path: &Path) -> Result<Document> {
    let content = tokio::fs::read_to_string(path).await?;
    let mut doc = Document::new(content);
    
    doc.metadata.insert(
      "source".to_string(),
      path.to_string_lossy().to_string().into(),
    );
    
    Ok(doc)
  }

  async fn load_directory(&self, _dir: &Path, _recursive: bool) -> Result<Vec<Document>> {
    // TODO: Implement directory loading
    Ok(Vec::new())
  }

  fn supported_extensions(&self) -> Vec<&'static str> {
    vec!["txt", "md"]
  }
}
