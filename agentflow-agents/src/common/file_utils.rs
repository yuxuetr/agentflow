//! File handling utilities for agents

use std::path::{Path, PathBuf};
use tokio::fs;

/// Discover files with specific extensions in a directory
pub async fn discover_files_with_extensions<P: AsRef<Path>>(
  directory: P, 
  extensions: &[&str]
) -> crate::AgentResult<Vec<PathBuf>> {
  let mut files = Vec::new();
  let mut dir = fs::read_dir(directory).await?;
  
  while let Some(entry) = dir.next_entry().await? {
    let path = entry.path();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
      if extensions.contains(&ext) {
        files.push(path);
      }
    }
  }

  Ok(files)
}

/// Create timestamped output directory
pub async fn create_timestamped_output_dir<P: AsRef<Path>>(
  base_dir: P,
  prefix: &str
) -> crate::AgentResult<PathBuf> {
  let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
  let output_dir = base_dir.as_ref().join(format!("{}_{}", prefix, timestamp));
  
  fs::create_dir_all(&output_dir).await?;
  
  Ok(output_dir)
}

/// Save content to file with proper error handling
pub async fn save_content<P: AsRef<Path>>(
  file_path: P,
  content: &str
) -> crate::AgentResult<()> {
  if let Some(parent) = file_path.as_ref().parent() {
    fs::create_dir_all(parent).await?;
  }
  
  fs::write(file_path, content).await?;
  Ok(())
}

/// Load content from file
pub async fn load_content<P: AsRef<Path>>(
  file_path: P
) -> crate::AgentResult<String> {
  let content = fs::read_to_string(file_path).await?;
  Ok(content)
}