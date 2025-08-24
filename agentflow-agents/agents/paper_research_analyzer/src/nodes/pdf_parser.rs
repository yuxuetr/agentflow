//! PDF Parser Node - Upload PDF to StepFun and extract text content

use agentflow_agents::{AsyncNode, SharedState, AgentFlowError, PDFContent};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct PDFParserNode {
  pdf_path: PathBuf,
  api_key: String,
  cached_content: Option<PDFContent>,
}

impl PDFParserNode {
  pub fn new(pdf_path: PathBuf, api_key: String, cached_content: PDFContent) -> Self {
    Self { 
      pdf_path, 
      api_key,
      cached_content: Some(cached_content),
    }
  }
}

#[async_trait]
impl AsyncNode for PDFParserNode {
  async fn prep_async(&self, _shared: &SharedState) -> Result<Value, AgentFlowError> {
    Ok(json!({
      "pdf_path": self.pdf_path.to_string_lossy(),
      "api_key": self.api_key,
      "cached": self.cached_content.is_some()
    }))
  }

  async fn exec_async(&self, _prep_result: Value) -> Result<Value, AgentFlowError> {
    // Use cached content if available
    if let Some(content) = &self.cached_content {
      println!("✅ Using cached PDF content");
      return Ok(json!({
        "file_id": content.file_id,
        "content": content.content,
        "token_count": content.token_count,
        "filename": content.filename
      }));
    }

    // Fallback to API extraction (original logic would go here)
    Err(AgentFlowError::AsyncExecutionError {
      message: "PDF content not cached and API extraction not implemented in node".to_string()
    })
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("📄 PDFParserNode: Storing content and metadata in shared state");
    shared.insert("pdf_content".to_string(), exec_result["content"].clone());
    shared.insert("pdf_metadata".to_string(), exec_result);
    // Return next node ID
    Ok(Some("summarizer".to_string()))
  }

  fn get_node_id(&self) -> Option<String> {
    Some("pdf_parser".to_string())
  }
}