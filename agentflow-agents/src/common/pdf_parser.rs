//! Common PDF parsing utilities using StepFun API

use serde_json::{json, Value};
use std::path::Path;

/// PDF parser using StepFun Document Parser API
pub struct StepFunPDFParser {
  api_key: String,
  client: reqwest::Client,
}

impl StepFunPDFParser {
  pub fn new(api_key: String) -> Self {
    Self {
      api_key,
      client: reqwest::Client::new(),
    }
  }

  /// Extract text content from PDF file
  pub async fn extract_content<P: AsRef<Path>>(
    &self, 
    pdf_path: P
  ) -> crate::AgentResult<PDFContent> {
    let path = pdf_path.as_ref();
    
    if !path.exists() {
      return Err(format!("PDF file does not exist: {}", path.display()).into());
    }

    let file_data = tokio::fs::read(path).await?;
    println!("📄 Uploading PDF: {}", path.display());
    println!("📋 File size: {} bytes", file_data.len());

    // Step 1: Upload PDF
    let form = reqwest::multipart::Form::new()
      .part("file", reqwest::multipart::Part::bytes(file_data)
        .file_name(path.file_name().unwrap().to_string_lossy().to_string())
        .mime_str("application/pdf")?)
      .text("purpose", "file-extract");

    let upload_response = self.client
      .post("https://api.stepfun.com/v1/files")
      .header("Authorization", format!("Bearer {}", self.api_key))
      .multipart(form)
      .send()
      .await?;

    if !upload_response.status().is_success() {
      let error_text = upload_response.text().await.unwrap_or_default();
      return Err(format!("PDF upload failed: {}", error_text).into());
    }

    let upload_text = upload_response.text().await?;
    let upload_result: Value = serde_json::from_str(&upload_text)?;
    
    let file_id = upload_result["id"]
      .as_str()
      .ok_or("No file ID in upload response")?;

    // Step 2: Wait for processing and retrieve content
    println!("⏳ Processing PDF content extraction...");
    
    let mut attempts = 0;
    let max_attempts = 20;
    
    loop {
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
      
      let content_response = self.client
        .get(&format!("https://api.stepfun.com/v1/files/{}/content", file_id))
        .header("Authorization", format!("Bearer {}", self.api_key))
        .send()
        .await?;

      let status = content_response.status();
      
      if status.as_u16() == 202 {
        attempts += 1;
        if attempts >= max_attempts {
          return Err("PDF processing timeout".into());
        }
        continue;
      }

      if !status.is_success() {
        let error_text = content_response.text().await.unwrap_or_default();
        return Err(format!("Content retrieval failed: {}", error_text).into());
      }

      let response_text = content_response.text().await?;
      
      let content_result: Value = if response_text.trim().starts_with('{') || response_text.trim().starts_with('[') {
        serde_json::from_str(&response_text)?
      } else {
        json!({
          "content": response_text,
          "token_count": response_text.len() / 4
        })
      };

      println!("✅ PDF content extracted successfully");
      
      let final_content = content_result["content"].as_str().unwrap_or("").to_string();
      let token_count = content_result["token_count"].as_u64().unwrap_or(0);
      
      println!("📋 Extracted {} characters, ~{} tokens", final_content.len(), token_count);
      
      return Ok(PDFContent {
        file_id: file_id.to_string(),
        content: final_content,
        token_count,
        filename: path.file_name().unwrap().to_string_lossy().to_string(),
      });
    }
  }
}

/// PDF content structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PDFContent {
  pub file_id: String,
  pub content: String,
  pub token_count: u64,
  pub filename: String,
}