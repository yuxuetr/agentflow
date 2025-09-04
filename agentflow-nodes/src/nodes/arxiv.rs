//! Arxiv Node Implementation
//!
//! This module provides the ArxivNode which retrieves LaTeX source content
//! from arXiv papers using HTTP requests.

use crate::error::NodeError;
use agentflow_core::{AsyncNode, Result, SharedState};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};

/// Configuration for Arxiv node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivConfig {
  /// Request timeout in seconds
  pub timeout_seconds: Option<u64>,
  /// Whether to save LaTeX source to file
  pub save_latex: Option<bool>,
  /// Whether to extract and save individual files from tar.gz
  pub extract_files: Option<bool>,
  /// Whether to expand all included LaTeX content into a single text
  pub expand_content: Option<bool>,
  /// Maximum recursion depth for file inclusion (default: 10)
  pub max_include_depth: Option<u32>,
  /// User agent string for HTTP requests
  pub user_agent: Option<String>,
}

impl Default for ArxivConfig {
  fn default() -> Self {
    Self {
      timeout_seconds: Some(60),
      save_latex: Some(false),
      extract_files: Some(false),
      expand_content: Some(true),
      max_include_depth: Some(10),
      user_agent: Some("AgentFlow/1.0 (arxiv-node)".to_string()),
    }
  }
}

/// Arxiv Node for retrieving LaTeX source content from arXiv papers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivNode {
  /// Node identifier
  pub name: String,
  /// ArXiv paper URL or ID (supports template variables)
  pub arxiv_url: String,
  /// Configuration options
  pub config: Option<ArxivConfig>,
  /// Where to store the result in shared state
  pub output_key: Option<String>,
  /// Directory to save extracted files
  pub output_directory: Option<String>,
}

/// Information about an arXiv paper extracted from URL
#[derive(Debug, Clone)]
struct ArxivPaperInfo {
  paper_id: String,
  version: Option<String>,
}

/// Information about extracted LaTeX content
#[derive(Debug, Clone)]
struct LatexContent {
  /// The main LaTeX file content
  main_content: String,
  /// Expanded content with all includes resolved
  expanded_content: Option<String>,
  /// Main file path within the archive
  main_file: Option<String>,
  /// All extracted file paths
  extracted_files: Vec<String>,
  /// File contents mapping (filename -> content)
  file_contents: HashMap<String, String>,
}

/// LaTeX file processing utilities
struct LatexProcessor {
  /// Maximum recursion depth for includes
  max_depth: u32,
  /// Processed files to avoid circular includes
  processed_files: HashSet<String>,
  /// File contents cache
  file_cache: HashMap<String, String>,
}

impl ArxivNode {
  /// Create a new Arxiv node with basic configuration
  pub fn new(name: impl Into<String>, arxiv_url: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      arxiv_url: arxiv_url.into(),
      config: Some(ArxivConfig::default()),
      output_key: None,
      output_directory: None,
    }
  }

  /// Set the output key for storing results in shared state
  pub fn with_output_key(mut self, key: impl Into<String>) -> Self {
    self.output_key = Some(key.into());
    self
  }

  /// Set the output directory for saving LaTeX files
  pub fn with_output_directory(mut self, dir: impl Into<String>) -> Self {
    self.output_directory = Some(dir.into());
    self
  }

  /// Set custom configuration
  pub fn with_config(mut self, config: ArxivConfig) -> Self {
    self.config = Some(config);
    self
  }

  /// Resolve template variables in URL
  fn resolve_url(&self, shared: &SharedState) -> Result<String> {
    let mut resolved = self.arxiv_url.clone();
    
    // Simple template variable resolution - replace {{key}} with values from shared state
    for (key, value) in shared.iter() {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder) {
        let replacement = match value {
          Value::String(s) => s.clone(),
          _ => value.to_string(),
        };
        resolved = resolved.replace(&placeholder, &replacement);
      }
    }
    
    Ok(resolved)
  }

  /// Extract paper information from arXiv URL
  fn parse_arxiv_url(&self, url: &str) -> Result<ArxivPaperInfo> {
    // Handle various arXiv URL formats:
    // - https://arxiv.org/abs/2312.07104
    // - https://arxiv.org/abs/2312.07104v2
    // - https://arxiv.org/pdf/2312.07104.pdf
    // - 2312.07104
    // - 2312.07104v2

    let url = url.trim();
    
    // Extract paper ID from various formats
    let paper_part = if url.starts_with("http") {
      // Extract from URL
      url.split("/")
        .last()
        .unwrap_or("")
        .replace(".pdf", "")
    } else {
      // Assume it's already a paper ID
      url.to_string()
    };

    // Parse paper ID and version
    let (paper_id, version) = if let Some(v_pos) = paper_part.find('v') {
      let (id, version_str) = paper_part.split_at(v_pos);
      (id.to_string(), Some(version_str[1..].to_string())) // Remove 'v' prefix
    } else {
      (paper_part, None)
    };

    // Validate paper ID format (simplified validation)
    if paper_id.is_empty() || (!paper_id.contains('.') && paper_id.len() < 4) {
      return Err(NodeError::ConfigurationError {
        message: format!("Invalid arXiv paper ID format: {}", paper_id),
      }.into());
    }

    Ok(ArxivPaperInfo { paper_id, version })
  }

  /// Build the source download URL for arXiv paper
  fn build_source_url(&self, paper_info: &ArxivPaperInfo) -> String {
    // ArXiv source download URL format: https://arxiv.org/src/PAPER_ID[vVERSION]
    let full_id = match &paper_info.version {
      Some(v) => format!("{}v{}", paper_info.paper_id, v),
      None => paper_info.paper_id.clone(),
    };
    
    format!("https://arxiv.org/src/{}", full_id)
  }

  /// Save LaTeX content to directory if specified
  async fn save_to_directory(&self, content: &[u8], paper_info: &ArxivPaperInfo) -> Result<Option<String>> {
    let default_config = ArxivConfig::default();
    let config = self.config.as_ref().unwrap_or(&default_config);
    
    if let Some(output_dir) = &self.output_directory {
      // Create output directory
      tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
        NodeError::FileOperationError {
          message: format!("Failed to create output directory {}: {}", output_dir, e),
        }
      })?;

      let file_name = format!("{}.tar.gz", paper_info.paper_id.replace('/', "_"));
      let file_path = Path::new(output_dir).join(&file_name);
      
      tokio::fs::write(&file_path, content).await.map_err(|e| {
        NodeError::FileOperationError {
          message: format!("Failed to save LaTeX source to {}: {}", file_path.display(), e),
        }
      })?;

      // Extract files if configured
      if config.extract_files.unwrap_or(false) {
        self.extract_tar_gz(&file_path, output_dir, paper_info).await?;
      }

      return Ok(Some(file_path.to_string_lossy().to_string()));
    }

    Ok(None)
  }

  /// Extract tar.gz file to directory
  async fn extract_tar_gz(
    &self, 
    tar_path: &Path, 
    output_dir: &str, 
    paper_info: &ArxivPaperInfo
  ) -> Result<()> {
    // Create extraction directory
    let extract_dir = Path::new(output_dir).join(&paper_info.paper_id.replace('/', "_"));
    tokio::fs::create_dir_all(&extract_dir).await.map_err(|e| {
      NodeError::FileOperationError {
        message: format!("Failed to create extraction directory: {}", e),
      }
    })?;

    // Read tar.gz file
    let tar_data = tokio::fs::read(tar_path).await.map_err(|e| {
      NodeError::FileOperationError {
        message: format!("Failed to read tar.gz file: {}", e),
      }
    })?;

    // Use blocking task for CPU-intensive decompression
    let extract_path = extract_dir.clone();
    tokio::task::spawn_blocking(move || {
      use std::io::Cursor;
      
      // Decompress gzip
      let gz_decoder = flate2::read::GzDecoder::new(Cursor::new(tar_data));
      let mut tar_archive = tar::Archive::new(gz_decoder);
      
      // Extract to directory
      tar_archive.unpack(&extract_path).map_err(|e| {
        NodeError::FileOperationError {
          message: format!("Failed to extract tar archive: {}", e),
        }
      })
    })
    .await
    .map_err(|e| NodeError::ExecutionError {
      message: format!("Extraction task failed: {}", e),
    })??;

    Ok(())
  }

  /// Get main LaTeX file content from bytes (if it's plain text)
  fn extract_main_latex(&self, content: &[u8]) -> Result<Option<String>> {
    // Try to detect if content is plain text LaTeX (some papers provide this)
    match std::str::from_utf8(content) {
      Ok(text) if text.contains("\\documentclass") => Ok(Some(text.to_string())),
      _ => Ok(None), // It's a binary tar.gz file
    }
  }

  /// Extract and process LaTeX content from tar.gz archive
  async fn extract_and_process_latex(&self, content: &[u8], paper_info: &ArxivPaperInfo) -> Result<LatexContent> {
    let default_config = ArxivConfig::default();
    let config = self.config.as_ref().unwrap_or(&default_config);
    
    // Create temporary directory for extraction
    let temp_dir = std::env::temp_dir().join(format!("arxiv_{}", paper_info.paper_id.replace('/', "_")));
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|e| {
      NodeError::FileOperationError {
        message: format!("Failed to create temp directory: {}", e),
      }
    })?;

    // Extract tar.gz to temp directory
    let extracted_files = self.extract_tar_gz_to_temp(content, &temp_dir).await?;
    
    // Read all file contents
    let mut file_contents = HashMap::new();
    for file_path in &extracted_files {
      if let Ok(content) = tokio::fs::read_to_string(file_path).await {
        if let Some(relative_path) = file_path.strip_prefix(&temp_dir).ok() {
          file_contents.insert(relative_path.to_string_lossy().to_string(), content);
        }
      }
    }

    // Find main LaTeX file
    let main_file = self.find_main_latex_file(&file_contents)?;
    let main_content = file_contents.get(&main_file)
      .ok_or_else(|| NodeError::ExecutionError {
        message: format!("Main file {} not found in extracted contents", main_file),
      })?.clone();

    // Expand content if requested
    let expanded_content = if config.expand_content.unwrap_or(true) {
      let processor = LatexProcessor::new(config.max_include_depth.unwrap_or(10));
      Some(processor.expand_latex_content(&main_content, &file_contents, &temp_dir)?)
    } else {
      None
    };

    // Cleanup temp directory
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;

    Ok(LatexContent {
      main_content,
      expanded_content,
      main_file: Some(main_file),
      extracted_files: extracted_files.into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect(),
      file_contents,
    })
  }

  /// Extract tar.gz archive to temporary directory
  async fn extract_tar_gz_to_temp(&self, tar_data: &[u8], temp_dir: &Path) -> Result<Vec<PathBuf>> {
    use std::io::Cursor;
    
    // Use blocking task for CPU-intensive decompression
    let temp_path = temp_dir.to_path_buf();
    let tar_data = tar_data.to_vec();
    
    tokio::task::spawn_blocking(move || {
      // Decompress gzip
      let gz_decoder = flate2::read::GzDecoder::new(Cursor::new(tar_data));
      let mut tar_archive = tar::Archive::new(gz_decoder);
      
      // Extract to directory and collect file paths
      let mut extracted_files = Vec::new();
      
      for entry in tar_archive.entries().map_err(|e| {
        NodeError::FileOperationError {
          message: format!("Failed to read tar entries: {}", e),
        }
      })? {
        let mut entry = entry.map_err(|e| {
          NodeError::FileOperationError {
            message: format!("Failed to process tar entry: {}", e),
          }
        })?;
        
        let path = entry.path().map_err(|e| {
          NodeError::FileOperationError {
            message: format!("Failed to get entry path: {}", e),
          }
        })?;
        
        let full_path = temp_path.join(&path);
        
        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
          std::fs::create_dir_all(parent).map_err(|e| {
            NodeError::FileOperationError {
              message: format!("Failed to create parent directory: {}", e),
            }
          })?;
        }
        
        // Extract the file
        entry.unpack(&full_path).map_err(|e| {
          NodeError::FileOperationError {
            message: format!("Failed to extract file: {}", e),
          }
        })?;
        
        if full_path.is_file() {
          extracted_files.push(full_path);
        }
      }
      
      Ok(extracted_files)
    })
    .await
    .map_err(|e| NodeError::ExecutionError {
      message: format!("Extraction task failed: {}", e),
    })?
  }

  /// Find the main LaTeX file in extracted contents
  fn find_main_latex_file(&self, file_contents: &HashMap<String, String>) -> Result<String> {
    // Priority order for finding main file
    let main_file_candidates = vec![
      "main.tex",
      "paper.tex", 
      "manuscript.tex",
      "article.tex",
      "document.tex",
    ];

    // First, try known main file names
    for candidate in &main_file_candidates {
      if file_contents.contains_key(*candidate) {
        return Ok(candidate.to_string());
      }
    }

    // Look for files with \documentclass
    let mut documentclass_files = Vec::new();
    for (filename, content) in file_contents {
      if filename.ends_with(".tex") && content.contains("\\documentclass") {
        documentclass_files.push(filename.clone());
      }
    }

    // If exactly one file has \documentclass, use it
    if documentclass_files.len() == 1 {
      return Ok(documentclass_files[0].clone());
    }

    // If multiple files have \documentclass, prefer the shortest name (likely main)
    if !documentclass_files.is_empty() {
      documentclass_files.sort_by_key(|f| f.len());
      return Ok(documentclass_files[0].clone());
    }

    // Last resort: use the first .tex file
    for filename in file_contents.keys() {
      if filename.ends_with(".tex") {
        return Ok(filename.clone());
      }
    }

    Err(NodeError::ExecutionError {
      message: "No LaTeX files found in archive".to_string(),
    }.into())
  }
}

impl LatexProcessor {
  /// Create a new LaTeX processor with the specified maximum depth
  fn new(max_depth: u32) -> Self {
    Self {
      max_depth,
      processed_files: HashSet::new(),
      file_cache: HashMap::new(),
    }
  }

  /// Expand LaTeX content by resolving all includes, inputs, and subfiles
  fn expand_latex_content(
    mut self,
    main_content: &str,
    file_contents: &HashMap<String, String>,
    _base_dir: &Path,
  ) -> Result<String> {
    // Initialize file cache
    self.file_cache = file_contents.clone();
    
    // Start expansion from main content
    self.expand_content_recursive(main_content, "", 0)
  }

  /// Recursively expand LaTeX content
  fn expand_content_recursive(
    &mut self,
    content: &str,
    current_file: &str,
    depth: u32,
  ) -> Result<String> {
    if depth > self.max_depth {
      return Ok(format!(
        "% MAX INCLUDE DEPTH REACHED ({})\n{}",
        self.max_depth, content
      ));
    }

    // Mark current file as processed to avoid circular includes
    if !current_file.is_empty() {
      if self.processed_files.contains(current_file) {
        return Ok(format!(
          "% CIRCULAR INCLUDE DETECTED: {}\n% Original content omitted to prevent infinite recursion",
          current_file
        ));
      }
      self.processed_files.insert(current_file.to_string());
    }

    // Create combined regex pattern to find all include commands
    let combined_pattern = r"\\(?:input|include|subfile|InputIfFileExists|bibliography|addbibresource)\{([^}]+)\}";
    let regex = regex::Regex::new(combined_pattern).map_err(|e| {
      NodeError::ExecutionError {
        message: format!("Failed to compile combined regex: {}", e),
      }
    })?;

    let mut expanded_content = String::new();
    let mut current_pos = 0;

    // Find all matches and process them in order
    for capture in regex.captures_iter(content) {
      let full_match = capture.get(0).unwrap();
      let file_ref = capture.get(1).unwrap().as_str();
      
      let match_start = full_match.start();
      let match_end = full_match.end();

      // Add content before the match
      expanded_content.push_str(&content[current_pos..match_start]);

      // Determine command type from the full match
      let command = full_match.as_str();
      let command_type = if command.starts_with("\\input") {
        "input"
      } else if command.starts_with("\\include") {
        "include"
      } else if command.starts_with("\\subfile") {
        "subfile"
      } else if command.starts_with("\\InputIfFileExists") {
        "inputiffileexists"
      } else if command.starts_with("\\bibliography") {
        "bibliography"
      } else if command.starts_with("\\addbibresource") {
        "addbibresource"
      } else {
        "unknown"
      };

      // Process the included file
      let included_content = self.process_include(file_ref, command_type, depth + 1)?;
      expanded_content.push_str(&included_content);

      current_pos = match_end;
    }

    // Add remaining content
    expanded_content.push_str(&content[current_pos..]);

    // Remove current file from processed set after expansion
    if !current_file.is_empty() {
      self.processed_files.remove(current_file);
    }

    Ok(expanded_content)
  }

  /// Process a single include directive
  fn process_include(
    &mut self,
    file_ref: &str,
    command_type: &str,
    depth: u32,
  ) -> Result<String> {
    // Normalize file reference
    let mut file_path = file_ref.to_string();
    
    // Add .tex extension if not present for certain commands
    if matches!(command_type, "input" | "include" | "subfile") && !file_path.ends_with(".tex") {
      file_path.push_str(".tex");
    }

    // Handle bibliography files
    if matches!(command_type, "bibliography" | "addbibresource") {
      if !file_path.ends_with(".bib") {
        file_path.push_str(".bib");
      }
      // For bibliography files, just add a comment
      return Ok(format!("% Bibliography file: {}\n", file_path));
    }

    // Try to find the file in our cache
    let file_content = if let Some(content) = self.file_cache.get(&file_path) {
      content.clone()
    } else {
      // Try common variations
      let variations = vec![
        file_path.clone(),
        format!("{}.tex", file_ref),
        format!("{}/{}.tex", file_ref, file_ref), // Common pattern: dir/dir.tex
      ];

      let mut found_content = None;
      for variation in &variations {
        if let Some(content) = self.file_cache.get(variation) {
          found_content = Some(content.clone());
          break;
        }
      }

      found_content.unwrap_or_else(|| {
        format!("% FILE NOT FOUND: {} (tried variations: {:?})\n", file_path, variations)
      })
    };

    // Add a comment header for the included file
    let mut result = format!("\n% === BEGIN INCLUDED FILE: {} ===\n", file_path);
    
    // Recursively expand the included content
    let expanded = self.expand_content_recursive(&file_content, &file_path, depth)?;
    result.push_str(&expanded);
    
    result.push_str(&format!("\n% === END INCLUDED FILE: {} ===\n", file_path));

    Ok(result)
  }
}

#[async_trait]
impl AsyncNode for ArxivNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Resolve URL template variables
    let resolved_url = self.resolve_url(shared)?;
    
    // Parse arXiv URL to extract paper information
    let paper_info = self.parse_arxiv_url(&resolved_url)?;
    
    // Build source download URL
    let source_url = self.build_source_url(&paper_info);
    
    Ok(json!({
      "paper_info": {
        "paper_id": paper_info.paper_id,
        "version": paper_info.version
      },
      "source_url": source_url,
      "original_url": resolved_url
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let default_config = ArxivConfig::default();
    let config = self.config.as_ref().unwrap_or(&default_config);
    let source_url = prep_result["source_url"].as_str()
      .ok_or_else(|| NodeError::ExecutionError {
        message: "No source URL in prep result".to_string(),
      })?;

    // Create HTTP client with timeout
    let timeout_duration = std::time::Duration::from_secs(
      config.timeout_seconds.unwrap_or(60)
    );
    let client = Client::builder()
      .timeout(timeout_duration)
      .user_agent(config.user_agent.as_ref().unwrap_or(&"AgentFlow/1.0".to_string()))
      .build()
      .map_err(|e| NodeError::HttpError {
        message: format!("Failed to create HTTP client: {}", e),
      })?;

    // Download LaTeX source
    let response = client
      .get(source_url)
      .send()
      .await
      .map_err(|e| NodeError::HttpError {
        message: format!("Failed to download arXiv source: {}", e),
      })?;

    if !response.status().is_success() {
      let status = response.status();
      return Err(NodeError::HttpError {
        message: format!("Download failed with status {}", status),
      }.into());
    }

    // Get content as bytes (could be tar.gz or plain text)
    let content_bytes = response.bytes().await.map_err(|e| NodeError::HttpError {
      message: format!("Failed to read response body: {}", e),
    })?;

    // Try to extract main LaTeX content if it's plain text
    let simple_latex_content = self.extract_main_latex(&content_bytes)?;

    // Extract paper info for processing
    let paper_info = ArxivPaperInfo {
      paper_id: prep_result["paper_info"]["paper_id"].as_str().unwrap_or("unknown").to_string(),
      version: prep_result["paper_info"]["version"].as_str().map(|s| s.to_string()),
    };

    // Process LaTeX content from archive
    let latex_processing_result = if simple_latex_content.is_none() {
      // It's a tar.gz archive, extract and process
      match self.extract_and_process_latex(&content_bytes, &paper_info).await {
        Ok(result) => Some(result),
        Err(e) => {
          eprintln!("Warning: Failed to process LaTeX archive: {}", e);
          None
        }
      }
    } else {
      None
    };

    Ok(json!({
      "content_bytes": STANDARD.encode(&content_bytes),
      "simple_latex_content": simple_latex_content,
      "latex_processing": latex_processing_result.as_ref().map(|lp| json!({
        "main_file": lp.main_file,
        "main_content": lp.main_content,
        "expanded_content": lp.expanded_content,
        "extracted_files_count": lp.extracted_files.len(),
        "file_contents_keys": lp.file_contents.keys().collect::<Vec<_>>(),
      })),
      "content_size": content_bytes.len(),
      "paper_info": prep_result["paper_info"],
      "source_url": source_url,
      "original_url": prep_result["original_url"]
    }))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Extract paper info
    let paper_info = ArxivPaperInfo {
      paper_id: exec_result["paper_info"]["paper_id"].as_str().unwrap_or("unknown").to_string(),
      version: exec_result["paper_info"]["version"].as_str().map(|s| s.to_string()),
    };

    // Decode content bytes
    let content_bytes = STANDARD.decode(
      exec_result["content_bytes"].as_str().unwrap_or("")
    ).map_err(|e| NodeError::ExecutionError {
      message: format!("Failed to decode content bytes: {}", e),
    })?;

    // Save to directory if specified
    let saved_path = self.save_to_directory(&content_bytes, &paper_info).await?;

    // Extract LaTeX processing information
    let latex_info = if let Some(processing) = exec_result.get("latex_processing") {
      json!({
        "main_file": processing.get("main_file"),
        "main_content": processing.get("main_content"),
        "expanded_content": processing.get("expanded_content"),
        "extracted_files_count": processing.get("extracted_files_count"),
        "has_expanded_content": processing.get("expanded_content").is_some(),
      })
    } else {
      json!({
        "simple_latex_content": exec_result.get("simple_latex_content"),
        "is_simple_tex": exec_result.get("simple_latex_content").is_some(),
      })
    };

    // Prepare result data
    let result_data = json!({
      "paper_id": paper_info.paper_id,
      "version": paper_info.version,
      "source_url": exec_result["source_url"],
      "original_url": exec_result["original_url"],
      "content_size": exec_result["content_size"],
      "latex_info": latex_info,
      "saved_path": saved_path,
      "node_name": self.name,
      "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    // Store result in shared state if key is specified
    if let Some(output_key) = &self.output_key {
      shared.insert(output_key.clone(), result_data.clone());
    }

    // Store in default output key
    shared.insert(format!("{}_output", self.name), result_data);

    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(format!("arxiv_{}", self.name))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::SharedState;

  #[tokio::test]
  async fn test_arxiv_node_creation() {
    let node = ArxivNode::new("test_paper", "https://arxiv.org/abs/2312.07104")
      .with_output_key("paper_source")
      .with_output_directory("./arxiv_output");

    assert_eq!(node.name, "test_paper");
    assert!(node.arxiv_url.contains("2312.07104"));
    assert_eq!(node.output_key, Some("paper_source".to_string()));
    assert_eq!(node.output_directory, Some("./arxiv_output".to_string()));
  }

  #[tokio::test]
  async fn test_url_resolution() {
    let shared = SharedState::new();
    shared.insert("paper_id".to_string(), json!("2312.07104"));
    shared.insert("version".to_string(), json!("v2"));

    let node = ArxivNode::new("template_test", "https://arxiv.org/abs/{{paper_id}}{{version}}");
    let resolved = node.resolve_url(&shared).unwrap();
    assert_eq!(resolved, "https://arxiv.org/abs/2312.07104v2");
  }

  #[test]
  fn test_arxiv_url_parsing() {
    let node = ArxivNode::new("test", "https://arxiv.org/abs/2312.07104v2");
    
    // Test full URL with version
    let info = node.parse_arxiv_url("https://arxiv.org/abs/2312.07104v2").unwrap();
    assert_eq!(info.paper_id, "2312.07104");
    assert_eq!(info.version, Some("2".to_string()));
    
    // Test URL without version
    let info = node.parse_arxiv_url("https://arxiv.org/abs/2312.07104").unwrap();
    assert_eq!(info.paper_id, "2312.07104");
    assert_eq!(info.version, None);
    
    // Test PDF URL
    let info = node.parse_arxiv_url("https://arxiv.org/pdf/2312.07104.pdf").unwrap();
    assert_eq!(info.paper_id, "2312.07104");
    assert_eq!(info.version, None);
    
    // Test bare paper ID
    let info = node.parse_arxiv_url("2312.07104v1").unwrap();
    assert_eq!(info.paper_id, "2312.07104");
    assert_eq!(info.version, Some("1".to_string()));
  }

  #[test]
  fn test_source_url_building() {
    let node = ArxivNode::new("test", "");
    
    // Test with version
    let info = ArxivPaperInfo {
      paper_id: "2312.07104".to_string(),
      version: Some("2".to_string()),
    };
    let url = node.build_source_url(&info);
    assert_eq!(url, "https://arxiv.org/src/2312.07104v2");
    
    // Test without version
    let info = ArxivPaperInfo {
      paper_id: "2312.07104".to_string(),
      version: None,
    };
    let url = node.build_source_url(&info);
    assert_eq!(url, "https://arxiv.org/src/2312.07104");
  }

  #[test]
  fn test_latex_content_detection() {
    let node = ArxivNode::new("test", "");
    
    // Test LaTeX content
    let latex_content = b"\\documentclass{article}\n\\begin{document}\nHello\n\\end{document}";
    let result = node.extract_main_latex(latex_content).unwrap();
    assert!(result.is_some());
    assert!(result.unwrap().contains("\\documentclass"));
    
    // Test binary content
    let binary_content = b"\x1f\x8b\x08\x00\x00\x00\x00\x00";
    let result = node.extract_main_latex(binary_content).unwrap();
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn test_node_id_generation() {
    let node = ArxivNode::new("my_paper", "2312.07104");
    assert_eq!(node.get_node_id(), Some("arxiv_my_paper".to_string()));
  }

  #[test]
  fn test_default_config() {
    let config = ArxivConfig::default();
    assert_eq!(config.timeout_seconds, Some(60));
    assert_eq!(config.save_latex, Some(false));
    assert_eq!(config.extract_files, Some(false));
    assert_eq!(config.expand_content, Some(true));
    assert_eq!(config.max_include_depth, Some(10));
    assert!(config.user_agent.is_some());
  }

  #[test]
  fn test_latex_processor_creation() {
    let processor = LatexProcessor::new(5);
    assert_eq!(processor.max_depth, 5);
    assert!(processor.processed_files.is_empty());
    assert!(processor.file_cache.is_empty());
  }

  #[test]
  fn test_main_file_detection() {
    let node = ArxivNode::new("test", "");
    
    // Test with main.tex
    let mut files = HashMap::new();
    files.insert("main.tex".to_string(), "\\documentclass{article}".to_string());
    files.insert("intro.tex".to_string(), "\\section{Introduction}".to_string());
    
    let main_file = node.find_main_latex_file(&files).unwrap();
    assert_eq!(main_file, "main.tex");
    
    // Test with single documentclass file
    let mut files = HashMap::new();
    files.insert("paper.tex".to_string(), "\\documentclass{article}".to_string());
    files.insert("intro.tex".to_string(), "\\section{Introduction}".to_string());
    
    let main_file = node.find_main_latex_file(&files).unwrap();
    assert_eq!(main_file, "paper.tex");
    
    // Test with multiple documentclass files (should pick shortest name)
    let mut files = HashMap::new();
    files.insert("very_long_filename.tex".to_string(), "\\documentclass{article}".to_string());
    files.insert("short.tex".to_string(), "\\documentclass{article}".to_string());
    
    let main_file = node.find_main_latex_file(&files).unwrap();
    assert_eq!(main_file, "short.tex");
  }

  #[test]
  fn test_latex_content_expansion() {
    let processor = LatexProcessor::new(10);
    let mut files = HashMap::new();
    
    files.insert("main.tex".to_string(), 
      "\\documentclass{article}\n\\begin{document}\n\\input{intro}\n\\end{document}".to_string());
    files.insert("intro.tex".to_string(), 
      "\\section{Introduction}\nThis is the introduction.".to_string());
    
    let main_content = files.get("main.tex").unwrap();
    let temp_dir = std::env::temp_dir();
    
    let expanded = processor.expand_latex_content(main_content, &files, &temp_dir).unwrap();
    
    assert!(expanded.contains("\\documentclass{article}"));
    assert!(expanded.contains("BEGIN INCLUDED FILE: intro.tex"));
    assert!(expanded.contains("This is the introduction."));
    assert!(expanded.contains("END INCLUDED FILE: intro.tex"));
  }

  #[test]
  fn test_circular_include_detection() {
    let processor = LatexProcessor::new(10);
    let mut files = HashMap::new();
    
    // Create circular include: main -> file1 -> file2 -> file1
    files.insert("main.tex".to_string(), 
      "\\documentclass{article}\n\\input{file1}".to_string());
    files.insert("file1.tex".to_string(), 
      "\\section{File 1}\n\\input{file2}".to_string());
    files.insert("file2.tex".to_string(), 
      "\\section{File 2}\n\\input{file1}".to_string());
    
    let main_content = files.get("main.tex").unwrap();
    let temp_dir = std::env::temp_dir();
    
    let expanded = processor.expand_latex_content(main_content, &files, &temp_dir).unwrap();
    
    assert!(expanded.contains("CIRCULAR INCLUDE DETECTED"));
    assert!(expanded.contains("\\section{File 1}"));
    assert!(expanded.contains("\\section{File 2}"));
  }

  #[test]
  fn test_max_depth_limit() {
    let processor = LatexProcessor::new(2); // Very low depth limit
    let mut files = HashMap::new();
    
    files.insert("main.tex".to_string(), 
      "\\documentclass{article}\n\\input{level1}".to_string());
    files.insert("level1.tex".to_string(), 
      "\\section{Level 1}\n\\input{level2}".to_string());
    files.insert("level2.tex".to_string(), 
      "\\section{Level 2}\n\\input{level3}".to_string());
    files.insert("level3.tex".to_string(), 
      "\\section{Level 3}".to_string());
    
    let main_content = files.get("main.tex").unwrap();
    let temp_dir = std::env::temp_dir();
    
    let expanded = processor.expand_latex_content(main_content, &files, &temp_dir).unwrap();
    
    assert!(expanded.contains("MAX INCLUDE DEPTH REACHED"));
    assert!(expanded.contains("\\section{Level 1}"));
    assert!(expanded.contains("\\section{Level 2}"));
    // Level 3 should not be fully expanded due to depth limit
  }

  #[test]
  fn test_bibliography_handling() {
    let processor = LatexProcessor::new(10);
    let mut files = HashMap::new();
    
    files.insert("main.tex".to_string(), 
      "\\documentclass{article}\n\\bibliography{refs}\n\\addbibresource{more_refs}".to_string());
    
    let main_content = files.get("main.tex").unwrap();
    let temp_dir = std::env::temp_dir();
    
    let expanded = processor.expand_latex_content(main_content, &files, &temp_dir).unwrap();
    
    assert!(expanded.contains("% Bibliography file: refs.bib"));
    assert!(expanded.contains("% Bibliography file: more_refs.bib"));
  }
}