// PDF Research Paper Analyzer
// An AgentFlow application using agentflow-core and agentflow-llm
// Features: PDF parsing, summarization, key insights, translation, mind maps

use agentflow_core::{AsyncFlow, AsyncNode, SharedState, AgentFlowError};
use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use reqwest;
use tokio;

/// PDF Research Paper Analyzer
/// 
/// This application demonstrates how to build a comprehensive research paper
/// analysis system using AgentFlow's core workflow orchestration and LLM capabilities.
/// 
/// Features:
/// - PDF upload and text extraction via StepFun Document Parser API
/// - Intelligent summarization of research papers
/// - Key insights and metadata extraction
/// - Multi-language translation support
/// - Mind map generation for visual representation
/// - Structured JSON output with comprehensive analysis
pub struct PDFAnalyzer {
  pub stepfun_api_key: String,
  pub target_language: String,
  pub analysis_depth: AnalysisDepth,
  pub generate_mind_map: bool,
  pub model: String,
}

#[derive(Debug, Clone)]
pub enum AnalysisDepth {
  Summary,      // Generate summary only
  Insights,     // Extract key insights only  
  Comprehensive, // Full analysis with summary + insights + mind map
  WithTranslation, // Everything + translation
}

impl PDFAnalyzer {
  pub fn new(stepfun_api_key: String) -> Self {
    Self {
      stepfun_api_key,
      target_language: "en".to_string(),
      analysis_depth: AnalysisDepth::Comprehensive,
      generate_mind_map: true,
      model: "step-2-16k".to_string(),
    }
  }

  /// Get model capacity based on model name
  fn get_model_capacity(&self) -> usize {
    match self.model.as_str() {
      // DashScope models with 1M token context
      m if m.contains("qwen-turbo") || m.contains("qwen-plus-latest") || m.contains("qwen-long") => {
        800_000  // ~800k chars for 1M token models, leaving room for prompt/response
      }
      // StepFun models
      m if m.contains("256k") => 200_000,   // 256k token models
      m if m.contains("32k") => 80_000,     // 32k token models
      // Claude models
      m if m.contains("claude") => 180_000,  // ~200k token models
      // OpenAI models  
      m if m.contains("gpt-4o") => 120_000,  // ~128k token models
      // Conservative default
      _ => 30_000
    }
  }

  /// Split content into semantic chunks based on document structure
  fn chunk_by_sections(&self, content: &str) -> Result<Vec<String>, String> {
    let max_chunk_size = self.get_model_capacity();
    let mut chunks = Vec::new();
    
    // Split on common research paper section headers
    let section_patterns = [
      "\n# ", "\n## ", "\n### ",  // Markdown headers
      "\nAbstract", "\nIntroduction", "\nMethods", "\nResults", 
      "\nDiscussion", "\nConclusion", "\nReferences",
      "\n1.", "\n2.", "\n3.", "\n4.", "\n5.",  // Numbered sections
    ];
    
    let mut current_chunk = String::new();
    let lines: Vec<&str> = content.lines().collect();
    
    for line in lines {
      // Check if this line starts a new section
      let is_section_start = section_patterns.iter().any(|pattern| {
        line.starts_with(&pattern[1..]) // Remove leading \n for comparison
      });
      
      // If adding this line would exceed capacity and we have content, start new chunk
      if current_chunk.len() + line.len() > max_chunk_size && !current_chunk.is_empty() {
        chunks.push(current_chunk.trim().to_string());
        current_chunk = String::new();
      }
      
      // If this is a section start and we have content, consider starting new chunk
      if is_section_start && !current_chunk.is_empty() && current_chunk.len() > max_chunk_size / 4 {
        chunks.push(current_chunk.trim().to_string());
        current_chunk = String::new();
      }
      
      current_chunk.push_str(line);
      current_chunk.push('\n');
    }
    
    // Add final chunk
    if !current_chunk.trim().is_empty() {
      chunks.push(current_chunk.trim().to_string());
    }
    
    // Ensure no chunk is empty and handle very large single sections
    let mut final_chunks = Vec::new();
    for chunk in chunks {
      if chunk.len() <= max_chunk_size {
        final_chunks.push(chunk);
      } else {
        // Split very large sections by character limit with overlap
        let overlap = 1000; // 1k character overlap for context
        let mut start = 0;
        
        while start < chunk.len() {
          let end = std::cmp::min(start + max_chunk_size, chunk.len());
          let chunk_slice = &chunk[start..end];
          final_chunks.push(chunk_slice.to_string());
          
          if end == chunk.len() { break; }
          start = end - overlap; // Overlap for context preservation
        }
      }
    }
    
    Ok(final_chunks)
  }

  pub fn target_language(mut self, language: &str) -> Self {
    self.target_language = language.to_string();
    self
  }

  pub fn analysis_depth(mut self, depth: AnalysisDepth) -> Self {
    self.analysis_depth = depth;
    self
  }

  pub fn model(mut self, model: &str) -> Self {
    self.model = model.to_string();
    self
  }

  pub fn generate_mind_map(mut self, enable: bool) -> Self {
    self.generate_mind_map = enable;
    self
  }

  /// Analyze a single PDF research paper with adaptive strategy
  pub async fn analyze_paper<P: AsRef<Path>>(&self, pdf_path: P) -> Result<AnalysisResult, String> {
    // First extract content to determine strategy
    let content = self.extract_pdf_content(pdf_path.as_ref()).await?;
    let model_capacity = self.get_model_capacity();
    
    if content.len() <= model_capacity {
      // Single-request analysis - optimal for most papers
      self.analyze_single_request(pdf_path, content).await
    } else {
      // Multi-request with semantic chunking
      println!("📄 Large document detected ({} chars), using chunked analysis with model {}", 
               content.len(), self.model);
      self.analyze_with_chunking(pdf_path, content).await
    }
  }

  /// Extract PDF content using StepFun API
  async fn extract_pdf_content<P: AsRef<Path>>(&self, pdf_path: P) -> Result<String, String> {
    let client = reqwest::Client::new();

    // Check if file exists first
    if !pdf_path.as_ref().exists() {
      return Err(format!("PDF file does not exist: {}", pdf_path.as_ref().display()));
    }

    let file_data = tokio::fs::read(&pdf_path).await
      .map_err(|e| format!("Failed to read PDF file: {}", e))?;

    println!("📄 Uploading PDF: {}", pdf_path.as_ref().display());
    println!("📋 File size: {} bytes", file_data.len());

    let form = reqwest::multipart::Form::new()
      .part("file", reqwest::multipart::Part::bytes(file_data)
        .file_name(pdf_path.as_ref().file_name().unwrap().to_string_lossy().to_string())
        .mime_str("application/pdf").unwrap())
      .text("purpose", "file-extract");

    let upload_response = client
      .post("https://api.stepfun.com/v1/files")
      .header("Authorization", format!("Bearer {}", self.stepfun_api_key))
      .multipart(form)
      .send()
      .await
      .map_err(|e| format!("PDF upload failed: {}", e))?;

    let status = upload_response.status();
    if !status.is_success() {
      let error_text = upload_response.text().await.unwrap_or_default();
      return Err(format!("PDF upload failed with status {}: {}", status, error_text));
    }

    let upload_text = upload_response.text().await
      .map_err(|e| format!("Failed to read upload response: {}", e))?;

    let upload_result: Value = serde_json::from_str(&upload_text)
      .map_err(|e| format!("Failed to parse upload response: {}. Raw response: {}", e, upload_text))?;

    let file_id = upload_result["id"].as_str().ok_or_else(|| 
      format!("No file ID in upload response. Response: {}", upload_result))?;

    // Wait for processing and retrieve content
    println!("⏳ Processing PDF content extraction...");
    
    let mut attempts = 0;
    let max_attempts = 20;
    
    loop {
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
      
      let content_response = client
        .get(&format!("https://api.stepfun.com/v1/files/{}/content", file_id))
        .header("Authorization", format!("Bearer {}", self.stepfun_api_key))
        .send()
        .await
        .map_err(|e| format!("Content retrieval failed: {}", e))?;

      let status = content_response.status();
      
      if status.as_u16() == 202 {
        attempts += 1;
        if attempts >= max_attempts {
          return Err("PDF processing timeout".to_string());
        }
        continue;
      }

      if !status.is_success() {
        let error_text = content_response.text().await.unwrap_or_default();
        return Err(format!("Content retrieval failed with status {}: {}", status, error_text));
      }

      let response_text = content_response.text().await
        .map_err(|e| format!("Failed to read response text: {}", e))?;

      let content_result: Value = if response_text.trim().starts_with('{') || response_text.trim().starts_with('[') {
        serde_json::from_str(&response_text)
          .map_err(|e| format!("Failed to parse JSON response: {}. Raw response: {}", e, response_text))?
      } else {
        println!("📄 Treating as plain text content");
        json!({
          "content": response_text,
          "token_count": response_text.len() / 4
        })
      };

      println!("✅ PDF content extracted successfully");
      
      let final_content = content_result["content"].as_str().unwrap_or("").to_string();
      let token_count = content_result["token_count"].as_u64().unwrap_or(0);
      
      println!("📋 Extracted {} characters, ~{} tokens", final_content.len(), token_count);
      
      return Ok(final_content);
    }
  }

  /// Single-request analysis for documents that fit in model context
  async fn analyze_single_request<P: AsRef<Path>>(&self, pdf_path: P, _content: String) -> Result<AnalysisResult, String> {
    // Use existing workflow logic but with pre-extracted content
    // Initialize AgentFlow LLM
    std::env::set_var("STEP_API_KEY", &self.stepfun_api_key);
    AgentFlow::init().await.map_err(|e| e.to_string())?;

    // Create workflow with analysis nodes
    let pdf_parser = PDFParserNode::new(
      pdf_path.as_ref().to_path_buf(),
      self.stepfun_api_key.clone()
    );
    let mut flow = AsyncFlow::new(Box::new(pdf_parser));

    // Step 2: Summary Generation Node (always included)
    let summarizer = SummaryNode::new(self.model.clone());
    flow.add_node("summarizer".to_string(), Box::new(summarizer));

    // Create shared state and add markers for conditional nodes
    let shared_state = SharedState::new();
    let has_insights = matches!(self.analysis_depth, AnalysisDepth::Insights | AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation);
    let has_mindmap = self.generate_mind_map && matches!(self.analysis_depth, AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation);
    let has_translation = matches!(self.analysis_depth, AnalysisDepth::WithTranslation) && self.target_language != "en";
    
    shared_state.insert("has_insights".to_string(), serde_json::Value::Bool(has_insights));
    shared_state.insert("has_mindmap".to_string(), serde_json::Value::Bool(has_mindmap));
    shared_state.insert("has_translation".to_string(), serde_json::Value::Bool(has_translation));

    // Step 3: Key Insights Extraction Node (conditional)
    if has_insights {
      let insights_extractor = InsightsNode::new(self.model.clone());
      flow.add_node("insights_extractor".to_string(), Box::new(insights_extractor));
    }

    // Step 4: Mind Map Generation Node (conditional)
    if has_mindmap {
      let mind_mapper = MindMapNode::new(self.model.clone());
      flow.add_node("mind_mapper".to_string(), Box::new(mind_mapper));
    }

    // Step 5: Translation Node (conditional)
    if has_translation {
      let translator = TranslationNode::new(self.model.clone(), self.target_language.clone());
      flow.add_node("translator".to_string(), Box::new(translator));
    }

    // Step 6: Results Compilation Node
    let compiler = ResultsCompilerNode::new(self.analysis_depth.clone());
    flow.add_node("compiler".to_string(), Box::new(compiler));

    // Execute workflow with the configured shared state
    let _execution_result = flow.run_async(&shared_state).await.map_err(|e| e.to_string())?;

    // Extract final results
    let final_result = shared_state.get("final_analysis")
      .ok_or("Analysis result not found".to_string())?
      .clone();
    
    let analysis_result = final_result
      .as_object()
      .ok_or("Invalid analysis result format".to_string())?;

    Ok(AnalysisResult::from_json(analysis_result.clone()))
  }

  /// Multi-request analysis with semantic chunking for large documents
  async fn analyze_with_chunking<P: AsRef<Path>>(&self, _pdf_path: P, content: String) -> Result<AnalysisResult, String> {
    // For now, fallback to single request with truncation
    // TODO: Implement full chunking strategy
    println!("🔧 Chunked analysis not fully implemented yet, using truncated single analysis");
    
    let model_capacity = self.get_model_capacity();
    let truncated_content = if content.len() > model_capacity {
      content[..model_capacity].to_string()
    } else {
      content
    };
    
    self.analyze_single_request(_pdf_path, truncated_content).await
  }

  /// Batch process multiple PDF papers
  pub async fn analyze_batch<P: AsRef<Path>>(&self, pdf_directory: P) -> Result<BatchAnalysisResult, String> {
    let mut results = Vec::new();
    let mut errors = Vec::new();

    // Find all PDF files in directory
    let pdf_files = self.discover_pdf_files(&pdf_directory).await.map_err(|e| e.to_string())?;
    
    println!("Found {} PDF files to process", pdf_files.len());

    // Process files with concurrency limit (3 concurrent)
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut handles = Vec::new();

    let pdf_files_len = pdf_files.len();
    for (index, pdf_path) in pdf_files.iter().enumerate() {
      let analyzer = self.clone();
      let pdf_path = pdf_path.clone();
      let permit = semaphore.clone();

      let handle = tokio::spawn(async move {
        let _permit = permit.acquire().await.unwrap();
        println!("Processing file {} of {}: {}", index + 1, pdf_files_len, pdf_path.display());
        
        match analyzer.analyze_paper(&pdf_path).await {
          Ok(result) => (pdf_path, Ok(result)),
          Err(e) => (pdf_path, Err(e)),
        }
      });
      handles.push(handle);
    }

    // Collect results
    for handle in handles {
      let (pdf_path, result) = handle.await.map_err(|e| e.to_string())?;
      match result {
        Ok(analysis) => results.push((pdf_path.clone(), analysis)),
        Err(e) => errors.push((pdf_path.clone(), e)),
      }
    }

    Ok(BatchAnalysisResult {
      successful_analyses: results,
      failed_analyses: errors,
      total_processed: pdf_files_len,
    })
  }

  async fn discover_pdf_files<P: AsRef<Path>>(&self, directory: P) -> Result<Vec<std::path::PathBuf>, String> {
    use tokio::fs;

    let mut pdf_files = Vec::new();
    let mut dir = fs::read_dir(directory).await.map_err(|e| e.to_string())?;
    
    while let Some(entry) = dir.next_entry().await.map_err(|e| e.to_string())? {
      let path = entry.path();
      if path.extension().and_then(|s| s.to_str()) == Some("pdf") {
        pdf_files.push(path);
      }
    }

    Ok(pdf_files)
  }
}

impl Clone for PDFAnalyzer {
  fn clone(&self) -> Self {
    Self {
      stepfun_api_key: self.stepfun_api_key.clone(),
      target_language: self.target_language.clone(),
      analysis_depth: self.analysis_depth.clone(),
      generate_mind_map: self.generate_mind_map,
      model: self.model.clone(),
    }
  }
}

/// PDF Parser Node - Upload PDF to StepFun and extract text content
struct PDFParserNode {
  pdf_path: std::path::PathBuf,
  api_key: String,
}

impl PDFParserNode {
  fn new(pdf_path: std::path::PathBuf, api_key: String) -> Self {
    Self { pdf_path, api_key }
  }
}

#[async_trait]
impl AsyncNode for PDFParserNode {
  async fn prep_async(&self, _shared: &SharedState) -> Result<Value, AgentFlowError> {
    Ok(json!({
      "pdf_path": self.pdf_path.to_string_lossy(),
      "api_key": self.api_key
    }))
  }

  async fn exec_async(&self, _prep_result: Value) -> Result<Value, AgentFlowError> {
    let client = reqwest::Client::new();

    // Step 1: Upload PDF file
    println!("📄 Uploading PDF: {}", self.pdf_path.display());
    
    // Check if file exists first
    if !self.pdf_path.exists() {
      return Err(AgentFlowError::AsyncExecutionError {
        message: format!("PDF file does not exist: {}", self.pdf_path.display())
      });
    }

    let file_data = tokio::fs::read(&self.pdf_path).await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Failed to read PDF file: {}", e) 
      })?;

    println!("📋 File size: {} bytes", file_data.len());

    let form = reqwest::multipart::Form::new()
      .part("file", reqwest::multipart::Part::bytes(file_data)
        .file_name(self.pdf_path.file_name().unwrap().to_string_lossy().to_string())
        .mime_str("application/pdf").unwrap())
      .text("purpose", "file-extract");

    let upload_response = client
      .post("https://api.stepfun.com/v1/files")
      .header("Authorization", format!("Bearer {}", self.api_key))
      .multipart(form)
      .send()
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("PDF upload failed: {}", e) 
      })?;

    let status = upload_response.status();
    if !status.is_success() {
      let error_text = upload_response.text().await.unwrap_or_default();
      return Err(AgentFlowError::AsyncExecutionError {
        message: format!("PDF upload failed with status {}: {}", status, error_text)
      });
    }

    // Get upload response as text first for debugging
    let upload_text = upload_response.text().await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Failed to read upload response: {}", e) 
      })?;


    let upload_result: Value = serde_json::from_str(&upload_text)
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Failed to parse upload response: {}. Raw response: {}", e, upload_text) 
      })?;

    let file_id = upload_result["id"].as_str().ok_or_else(|| 
      AgentFlowError::AsyncExecutionError { 
        message: format!("No file ID in upload response. Response: {}", upload_result) 
      })?;

    // Step 2: Wait for processing and retrieve content
    println!("⏳ Processing PDF content extraction...");
    
    let mut attempts = 0;
    let max_attempts = 20;
    
    loop {
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
      
      let content_response = client
        .get(&format!("https://api.stepfun.com/v1/files/{}/content", file_id))
        .header("Authorization", format!("Bearer {}", self.api_key))
        .send()
        .await
        .map_err(|e| AgentFlowError::AsyncExecutionError { 
          message: format!("Content retrieval failed: {}", e) 
        })?;

      let status = content_response.status();
      
      if status.as_u16() == 202 {
        // Still processing
        attempts += 1;
        if attempts >= max_attempts {
          return Err(AgentFlowError::AsyncExecutionError {
            message: "PDF processing timeout".to_string()
          });
        }
        continue;
      }

      if !status.is_success() {
        let error_text = content_response.text().await.unwrap_or_default();
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("Content retrieval failed with status {}: {}", status, error_text)
        });
      }

      // Get response text first to handle different content types
      let response_text = content_response.text().await
        .map_err(|e| AgentFlowError::AsyncExecutionError { 
          message: format!("Failed to read response text: {}", e) 
        })?;


      // Try to parse as JSON, but handle plain text responses too
      let content_result: Value = if response_text.trim().starts_with('{') || response_text.trim().starts_with('[') {
        serde_json::from_str(&response_text)
          .map_err(|e| AgentFlowError::AsyncExecutionError { 
            message: format!("Failed to parse JSON response: {}. Raw response: {}", e, response_text) 
          })?
      } else {
        // If it's plain text, wrap it in a JSON object
        println!("📄 Treating as plain text content");
        json!({
          "content": response_text,
          "token_count": response_text.len() / 4  // Rough estimate
        })
      };

      println!("✅ PDF content extracted successfully");
      
      let final_content = content_result["content"].as_str().unwrap_or("").to_string();
      let token_count = content_result["token_count"].as_u64().unwrap_or(0);
      
      println!("📋 Extracted {} characters, ~{} tokens", final_content.len(), token_count);
      
      return Ok(json!({
        "file_id": file_id,
        "content": final_content,
        "token_count": token_count,
        "filename": self.pdf_path.file_name().unwrap().to_string_lossy()
      }));
    }
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

/// Summary Generation Node - Create comprehensive research paper summary
struct SummaryNode {
  model: String,
}

impl SummaryNode {
  fn new(model: String) -> Self {
    Self { model }
  }

  /// Get model capacity for summary generation
  fn get_model_capacity_for_summary(&self) -> usize {
    match self.model.as_str() {
      m if m.contains("qwen-turbo") || m.contains("qwen-plus-latest") || m.contains("qwen-long") => 800_000,
      m if m.contains("256k") => 200_000,
      m if m.contains("32k") => 80_000,
      m if m.contains("claude") => 180_000,
      m if m.contains("gpt-4o") => 120_000,
      _ => 30_000
    }
  }
}

#[async_trait]
impl AsyncNode for SummaryNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    let content = shared.get("pdf_content").ok_or_else(|| 
      AgentFlowError::AsyncExecutionError { message: "PDF content not available".to_string() })?;
    
    Ok(json!({
      "content": content,
      "model": self.model
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let content = prep_result["content"].as_str().unwrap();
    
    // Calculate reasonable truncation based on model context window
    let max_content_chars = self.get_model_capacity_for_summary();
    
    let truncated_content = if content.len() > max_content_chars {
      println!("⚠️  Content too long ({}), truncating to {} characters for model {}", 
               content.len(), max_content_chars, self.model);
      &content[..max_content_chars]
    } else {
      content
    };
    
    println!("📝 Generating research paper summary...");

    let summary_prompt = format!(r#"
请分析这篇研究论文，并按以下结构提供全面的中文摘要：

# 研究论文摘要

## 标题和作者
[提取论文标题和作者信息]

## 摘要总结  
[用2-3句话总结摘要]

## 研究问题
[这篇论文解决了什么问题？]

## 研究方法
[简要描述使用的研究方法]

## 主要发现
[主要结果和发现，编号列表]

## 结论
[作者的结论和意义]

## 重要性
[为什么这项研究很重要？]

## 局限性
[作者提到的任何局限性]

Research Paper Content:
{}
"#, truncated_content);

    let response = AgentFlow::model(&self.model)
      .prompt(&summary_prompt)
      .temperature(0.3)
      .max_tokens(2000)  // Increased for complete summary
      .execute()
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Summary generation failed: {}", e) 
      })?;

    println!("✅ Summary generated successfully");

    Ok(json!({
      "summary": response,
      "model_used": self.model
    }))
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("📝 SummaryNode: Storing summary in shared state");
    shared.insert("summary".to_string(), exec_result);
    
    // Determine next node based on workflow configuration
    if shared.get("has_insights").and_then(|v| v.as_bool()).unwrap_or(false) {
      Ok(Some("insights_extractor".to_string()))
    } else {
      Ok(Some("compiler".to_string()))
    }
  }

  fn get_node_id(&self) -> Option<String> {
    Some("summary_generator".to_string())
  }
}

/// Key Insights Extraction Node - Extract structured metadata and insights
struct InsightsNode {
  model: String,
}

impl InsightsNode {
  fn new(model: String) -> Self {
    Self { model }
  }

  /// Get model capacity for insights extraction (75% of full capacity)
  fn get_model_capacity_for_insights(&self) -> usize {
    let full_capacity = match self.model.as_str() {
      m if m.contains("qwen-turbo") || m.contains("qwen-plus-latest") || m.contains("qwen-long") => 800_000,
      m if m.contains("256k") => 200_000,
      m if m.contains("32k") => 80_000,
      m if m.contains("claude") => 180_000,
      m if m.contains("gpt-4o") => 120_000,
      _ => 30_000
    };
    (full_capacity as f64 * 0.75) as usize
  }
}

#[async_trait]
impl AsyncNode for InsightsNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    let content = shared.get("pdf_content").ok_or_else(|| 
      AgentFlowError::AsyncExecutionError { message: "PDF content not available".to_string() })?;
    
    Ok(json!({
      "content": content,
      "model": self.model
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let content = prep_result["content"].as_str().unwrap();
    
    // Calculate reasonable truncation based on model context window  
    let max_content_chars = self.get_model_capacity_for_insights();
    
    let truncated_content = if content.len() > max_content_chars {
      println!("⚠️  Content too long for insights extraction ({}), truncating to {} characters for model {}", 
               content.len(), max_content_chars, self.model);
      &content[..max_content_chars]
    } else {
      content
    };
    
    println!("🔍 Extracting key insights and metadata...");

    let insights_prompt = format!(r#"
分析这篇研究论文，并按以下JSON格式提取关键洞察：

{{
  "title": "论文确切标题",
  "authors": ["作者列表"],
  "publication_year": "发表年份（如有）",
  "field_of_study": "主要研究领域",
  "research_type": "理论/实证/实验/综述/评论",
  "methodology": ["使用的方法列表"],
  "key_contributions": ["主要贡献"],
  "novel_concepts": ["引入的新概念"],
  "datasets_used": ["提到的数据集"],
  "evaluation_metrics": ["用于评估的指标"],
  "future_work": ["建议的未来研究方向"],
  "citations_mentioned": "参考文献数量",
  "research_gap": "填补了什么空白",
  "impact_potential": "high/medium/low",
  "reproducibility": "high/medium/low/unclear"
}}

Research Paper Content:
{}
"#, truncated_content);

    let response = AgentFlow::model(&self.model)
      .prompt(&insights_prompt)
      .temperature(0.2)
      .max_tokens(1500)  // Increased for complete insights JSON
      .execute()
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Insights extraction failed: {}", e) 
      })?;

    println!("✅ Key insights extracted successfully");

    // Try to parse as JSON to validate structure
    let insights_json: Value = serde_json::from_str(&response)
      .unwrap_or_else(|_| json!({"raw_response": response}));

    Ok(json!({
      "insights": insights_json,
      "model_used": self.model
    }))
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("🔍 InsightsNode: Storing insights in shared state");
    shared.insert("insights".to_string(), exec_result);
    
    // Determine next node
    if shared.get("has_mindmap").and_then(|v| v.as_bool()).unwrap_or(false) {
      Ok(Some("mind_mapper".to_string()))
    } else if shared.get("has_translation").and_then(|v| v.as_bool()).unwrap_or(false) {
      Ok(Some("translator".to_string()))
    } else {
      Ok(Some("compiler".to_string()))
    }
  }

  fn get_node_id(&self) -> Option<String> {
    Some("insights_extractor".to_string())
  }
}

/// Mind Map Generation Node - Create Mermaid mind map visualization
struct MindMapNode {
  model: String,
}

impl MindMapNode {
  fn new(model: String) -> Self {
    Self { model }
  }
}

#[async_trait]
impl AsyncNode for MindMapNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    let insights = shared.get("insights").ok_or_else(|| 
      AgentFlowError::AsyncExecutionError { message: "Insights not available".to_string() })?;
    
    Ok(json!({
      "insights": insights,
      "model": self.model
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let insights = &prep_result["insights"];
    
    println!("🧠 Generating mind map visualization...");

    let mindmap_prompt = format!(r#"
Create a Mermaid mind map diagram for this research paper based on the extracted insights.
Focus on the main concepts, methodology, findings, and relationships.

Use this structure:
```mermaid
mindmap
  root)Research Paper Title(
    Problem
      [specific problem areas]
    Method  
      [methodological approaches]
    Findings
      [key results]
    Impact
      [significance and applications]
```

Paper Insights:
{}
"#, insights);

    let response = AgentFlow::model(&self.model)
      .prompt(&mindmap_prompt)
      .temperature(0.4)
      .max_tokens(1000)
      .execute()
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Mind map generation failed: {}", e) 
      })?;

    println!("✅ Mind map generated successfully");

    Ok(json!({
      "mind_map": response,
      "model_used": self.model
    }))
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("🧠 MindMapNode: Storing mind map in shared state");
    shared.insert("mind_map".to_string(), exec_result);
    
    // Determine next node
    if shared.get("has_translation").and_then(|v| v.as_bool()).unwrap_or(false) {
      Ok(Some("translator".to_string()))
    } else {
      Ok(Some("compiler".to_string()))
    }
  }

  fn get_node_id(&self) -> Option<String> {
    Some("mind_map_generator".to_string())
  }
}

/// Translation Node - Translate summary to target language
struct TranslationNode {
  model: String,
  target_language: String,
}

impl TranslationNode {
  fn new(model: String, target_language: String) -> Self {
    Self { model, target_language }
  }
}

#[async_trait]
impl AsyncNode for TranslationNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    let summary = shared.get("summary").ok_or_else(|| 
      AgentFlowError::AsyncExecutionError { message: "Summary not available".to_string() })?;
    
    Ok(json!({
      "summary": summary,
      "target_language": self.target_language,
      "model": self.model
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let summary = prep_result["summary"]["summary"].as_str().unwrap();
    
    println!("🌍 Translating summary to {}...", self.target_language);

    let translation_prompt = format!(r#"
Please translate this research paper summary to {}.
Maintain all technical terms and academic formatting. If technical terms don't have direct translations, keep them in English with brief explanations.

Original Summary:
{}
"#, self.target_language, summary);

    let response = AgentFlow::model(&self.model)
      .prompt(&translation_prompt)
      .temperature(0.1)
      .max_tokens(2500)
      .execute()
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError { 
        message: format!("Translation failed: {}", e) 
      })?;

    println!("✅ Translation completed successfully");

    Ok(json!({
      "translated_summary": response,
      "target_language": self.target_language,
      "model_used": self.model
    }))
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("🌍 TranslationNode: Storing translation in shared state");
    shared.insert("translation".to_string(), exec_result);
    // Always go to compiler after translation
    Ok(Some("compiler".to_string()))
  }

  fn get_node_id(&self) -> Option<String> {
    Some("translator".to_string())
  }
}

/// Results Compiler Node - Compile all analysis results into final output
struct ResultsCompilerNode {
  analysis_depth: AnalysisDepth,
}

impl ResultsCompilerNode {
  fn new(analysis_depth: AnalysisDepth) -> Self {
    Self { analysis_depth }
  }
}

#[async_trait]
impl AsyncNode for ResultsCompilerNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    let pdf_metadata = shared.get("pdf_metadata").map(|v| v.clone()).unwrap_or_else(|| json!({}));
    let summary = shared.get("summary").map(|v| v.clone()).unwrap_or_else(|| json!({}));
    let insights = shared.get("insights").map(|v| v.clone()).unwrap_or_else(|| json!({}));
    let mind_map = shared.get("mind_map").map(|v| v.clone()).unwrap_or_else(|| json!({}));
    let translation = shared.get("translation").map(|v| v.clone()).unwrap_or_else(|| json!({}));
    
    Ok(json!({
      "pdf_metadata": pdf_metadata,
      "summary": summary,
      "insights": insights,
      "mind_map": mind_map,
      "translation": translation,
      "analysis_depth": format!("{:?}", self.analysis_depth)
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    println!("📊 Compiling final analysis results...");

    let now = chrono::Utc::now();
    let mut final_result = json!({
      "analysis_metadata": {
        "pdf_filename": prep_result["pdf_metadata"]["filename"],
        "pdf_size_bytes": prep_result["pdf_metadata"]["token_count"],
        "analysis_type": prep_result["analysis_depth"],
        "generated_at": now.to_rfc3339(),
        "processing_successful": true
      }
    });

    // Always include summary
    if let Some(summary) = prep_result["summary"]["summary"].as_str() {
      final_result["summary"] = json!(summary);
    }

    // Include insights if available
    if !prep_result["insights"]["insights"].is_null() {
      final_result["key_insights"] = prep_result["insights"]["insights"].clone();
    }

    // Include mind map if available
    if !prep_result["mind_map"]["mind_map"].is_null() {
      final_result["mind_map"] = prep_result["mind_map"]["mind_map"].clone();
    }

    // Include translation if available
    if !prep_result["translation"]["translated_summary"].is_null() {
      final_result["translated_summary"] = prep_result["translation"]["translated_summary"].clone();
      final_result["target_language"] = prep_result["translation"]["target_language"].clone();
    }

    final_result["processing_stats"] = json!({
      "summary_generated": !prep_result["summary"]["summary"].is_null(),
      "insights_extracted": !prep_result["insights"]["insights"].is_null(),
      "mind_map_created": !prep_result["mind_map"]["mind_map"].is_null(),
      "translation_completed": !prep_result["translation"]["translated_summary"].is_null()
    });

    println!("✅ Analysis compilation completed successfully");

    Ok(final_result)
  }

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, AgentFlowError> {
    println!("📊 ResultsCompilerNode: Storing final analysis in shared state");
    shared.insert("final_analysis".to_string(), exec_result);
    // End of workflow - return None to stop execution
    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some("results_compiler".to_string())
  }
}

/// Analysis Result Structure
#[derive(Debug, Clone)]
pub struct AnalysisResult {
  pub summary: Option<String>,
  pub key_insights: Option<Value>,
  pub mind_map: Option<String>,
  pub translated_summary: Option<String>,
  pub target_language: Option<String>,
  pub processing_stats: HashMap<String, bool>,
  pub metadata: HashMap<String, Value>,
}

impl AnalysisResult {
  fn from_json(value: serde_json::Map<String, Value>) -> Self {
    let mut processing_stats = HashMap::new();
    let mut metadata = HashMap::new();

    // Extract processing stats
    if let Some(stats) = value.get("processing_stats").and_then(|v| v.as_object()) {
      for (k, v) in stats {
        if let Some(bool_val) = v.as_bool() {
          processing_stats.insert(k.clone(), bool_val);
        }
      }
    }

    // Extract metadata
    if let Some(meta) = value.get("analysis_metadata").and_then(|v| v.as_object()) {
      for (k, v) in meta {
        metadata.insert(k.clone(), v.clone());
      }
    }

    Self {
      summary: value.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
      key_insights: value.get("key_insights").cloned(),
      mind_map: value.get("mind_map").and_then(|v| v.as_str()).map(|s| s.to_string()),
      translated_summary: value.get("translated_summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
      target_language: value.get("target_language").and_then(|v| v.as_str()).map(|s| s.to_string()),
      processing_stats,
      metadata,
    }
  }

  /// Save analysis results to files
  pub async fn save_to_files<P: AsRef<Path>>(&self, output_dir: P) -> Result<(), String> {
    tokio::fs::create_dir_all(&output_dir).await.map_err(|e| e.to_string())?;

    // Save summary as markdown
    if let Some(summary) = &self.summary {
      let summary_path = output_dir.as_ref().join("summary.md");
      tokio::fs::write(summary_path, summary).await.map_err(|e| e.to_string())?;
    }

    // Save insights as JSON
    if let Some(insights) = &self.key_insights {
      let insights_path = output_dir.as_ref().join("key_insights.json");
      let insights_pretty = serde_json::to_string_pretty(insights).map_err(|e| e.to_string())?;
      tokio::fs::write(insights_path, insights_pretty).await.map_err(|e| e.to_string())?;
    }

    // Save mind map as mermaid file
    if let Some(mind_map) = &self.mind_map {
      let mindmap_path = output_dir.as_ref().join("mind_map.mermaid");
      tokio::fs::write(mindmap_path, mind_map).await.map_err(|e| e.to_string())?;
    }

    // Save translation if available
    if let Some(translation) = &self.translated_summary {
      let lang = self.target_language.as_deref().unwrap_or("unknown");
      let translation_path = output_dir.as_ref().join(format!("summary_{}.md", lang));
      tokio::fs::write(translation_path, translation).await.map_err(|e| e.to_string())?;
    }

    // Save complete analysis as JSON
    let complete_analysis = json!({
      "summary": self.summary,
      "key_insights": self.key_insights,
      "mind_map": self.mind_map,
      "translated_summary": self.translated_summary,
      "target_language": self.target_language,
      "processing_stats": self.processing_stats,
      "metadata": self.metadata
    });
    
    let analysis_path = output_dir.as_ref().join("complete_analysis.json");
    let analysis_pretty = serde_json::to_string_pretty(&complete_analysis).map_err(|e| e.to_string())?;
    tokio::fs::write(analysis_path, analysis_pretty).await.map_err(|e| e.to_string())?;

    println!("✅ Analysis results saved to: {}", output_dir.as_ref().display());
    Ok(())
  }
}

/// Batch Analysis Result Structure
#[derive(Debug)]
pub struct BatchAnalysisResult {
  pub successful_analyses: Vec<(std::path::PathBuf, AnalysisResult)>,
  pub failed_analyses: Vec<(std::path::PathBuf, String)>,
  pub total_processed: usize,
}

impl BatchAnalysisResult {
  /// Save batch results to directory
  pub async fn save_to_directory<P: AsRef<Path>>(&self, output_dir: P) -> Result<(), String> {
    tokio::fs::create_dir_all(&output_dir).await.map_err(|e| e.to_string())?;

    // Save individual results
    for (pdf_path, analysis) in &self.successful_analyses {
      let filename = pdf_path.file_stem().unwrap().to_string_lossy();
      let result_dir = output_dir.as_ref().join(&*filename);
      analysis.save_to_files(result_dir).await?;
    }

    // Save batch summary report
    let batch_report = json!({
      "batch_summary": {
        "total_processed": self.total_processed,
        "successful": self.successful_analyses.len(),
        "failed": self.failed_analyses.len(),
        "success_rate": (self.successful_analyses.len() as f64 / self.total_processed as f64 * 100.0).round()
      },
      "successful_files": self.successful_analyses.iter().map(|(path, _)| path.file_name().unwrap().to_string_lossy()).collect::<Vec<_>>(),
      "failed_files": self.failed_analyses.iter().map(|(path, error)| json!({
        "filename": path.file_name().unwrap().to_string_lossy(),
        "error": error
      })).collect::<Vec<_>>()
    });

    let report_path = output_dir.as_ref().join("batch_analysis_report.json");
    let report_pretty = serde_json::to_string_pretty(&batch_report).map_err(|e| e.to_string())?;
    tokio::fs::write(report_path, report_pretty).await.map_err(|e| e.to_string())?;

    println!("✅ Batch analysis results saved to: {}", output_dir.as_ref().display());
    Ok(())
  }
}

// Example usage and tests
#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_pdf_analyzer_creation() {
    let analyzer = PDFAnalyzer::new("test-api-key".to_string())
      .target_language("zh")
      .analysis_depth(AnalysisDepth::Comprehensive)
      .model("step-2-16k");

    assert_eq!(analyzer.target_language, "zh");
    assert!(matches!(analyzer.analysis_depth, AnalysisDepth::Comprehensive));
    assert_eq!(analyzer.model, "step-2-16k");
  }
}

/// Example usage demonstration
#[tokio::main] 
async fn main() -> Result<(), String> {
  // Example 1: Analyze single research paper with long-context model - Chinese output
  let analyzer = PDFAnalyzer::new(std::env::var("STEP_API_KEY").map_err(|e| e.to_string())?)
    .analysis_depth(AnalysisDepth::WithTranslation)  // Enable translation
    .target_language("zh")  // Chinese output
    .model("qwen-turbo-latest");  // Use 1M token context DashScope model

  println!("🚀 Starting PDF Research Paper Analysis");

  // Analyze single paper
  match analyzer.analyze_paper("./assets/2312.07104v2.pdf").await {
    Ok(result) => {
      println!("✅ Analysis completed successfully!");
      result.save_to_files("./analysis_output").await?;
    }
    Err(e) => {
      eprintln!("❌ Analysis failed: {}", e);
    }
  }

  // Example 2: Batch analysis
  // println!("\n🔄 Starting batch analysis...");
  // let batch_analyzer = PDFAnalyzer::new(std::env::var("STEP_API_KEY")?)
  //   .analysis_depth(AnalysisDepth::Summary)
  //   .model("step-2-mini"); // Faster model for batch processing

  // match batch_analyzer.analyze_batch("./research_papers/").await {
  //   Ok(batch_result) => {
  //     println!("✅ Batch analysis completed!");
  //     println!("📊 Processed: {} papers", batch_result.total_processed);
  //     println!("✅ Successful: {} papers", batch_result.successful_analyses.len());
  //     println!("❌ Failed: {} papers", batch_result.failed_analyses.len());
      
  //     batch_result.save_to_directory("./batch_analysis_output").await?;
  //   }
  //   Err(e) => {
  //     eprintln!("❌ Batch analysis failed: {}", e);
  //   }
  // }

  Ok(())
}
