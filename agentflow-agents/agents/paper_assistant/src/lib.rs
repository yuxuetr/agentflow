//! Paper Assistant - AI Agent for comprehensive arXiv paper processing
//!
//! This agent processes arXiv papers with the following capabilities:
//! 1. Fetches paper content from arXiv URLs using ArxivNode
//! 2. Generates Chinese summaries using qwen-turbo model
//! 3. Translates papers to Chinese using qwen-turbo model  
//! 4. Creates Chinese mind maps for subsections using MarkMapNode
//! 5. Generates poster images using qwen-image model

use agentflow_core::SharedState;
use agentflow_nodes::{ArxivNode, LlmNode, MarkMapNode, TextToImageNode};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

pub mod workflow;
pub mod config;
pub mod utils;

use workflow::PaperAssistantWorkflow;
pub use config::{PaperAssistantConfig, ConfigBuilder};

/// Main Paper Assistant struct
#[derive(Debug)]
pub struct PaperAssistant {
  config: PaperAssistantConfig,
  workflow: PaperAssistantWorkflow,
  shared_state: SharedState,
}

/// Result data from paper processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperProcessingResult {
  pub paper_id: String,
  pub original_url: String,
  pub chinese_summary: String,
  pub chinese_translation: String,
  pub mind_maps: Vec<MindMapResult>,
  pub poster_image_path: Option<String>,
  pub processing_time_ms: u64,
  pub timestamp: String,
}

/// Mind map result for a paper subsection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapResult {
  pub section_title: String,
  pub section_number: Option<String>,
  pub mind_map_html: String,
  pub mind_map_markdown: String,
}

impl PaperAssistant {
  /// Create a new Paper Assistant with default configuration
  pub fn new() -> Result<Self> {
    let config = PaperAssistantConfig::default();
    let workflow = PaperAssistantWorkflow::new(&config)?;
    let shared_state = SharedState::new();
    
    Ok(Self {
      config,
      workflow,
      shared_state,
    })
  }

  /// Create a new Paper Assistant with custom configuration
  pub fn with_config(config: PaperAssistantConfig) -> Result<Self> {
    let workflow = PaperAssistantWorkflow::new(&config)?;
    let shared_state = SharedState::new();
    
    Ok(Self {
      config,
      workflow,
      shared_state,
    })
  }

  /// Process a paper from an arXiv URL
  pub async fn process_paper(&mut self, arxiv_url: &str) -> Result<PaperProcessingResult> {
    let start_time = std::time::Instant::now();
    
    // Set the arXiv URL in shared state
    self.shared_state.insert("arxiv_url".to_string(), json!(arxiv_url));
    
    // Generate a unique processing ID
    let processing_id = Uuid::new_v4().to_string();
    self.shared_state.insert("processing_id".to_string(), json!(processing_id));
    
    log::info!("Starting paper processing for URL: {}", arxiv_url);
    
    // Execute the workflow
    let result = self.workflow.execute(&self.shared_state).await?;
    
    let processing_time = start_time.elapsed();
    
    // Extract results from shared state
    let paper_processing_result = self.extract_processing_result(
      arxiv_url,
      processing_time.as_millis() as u64,
    )?;
    
    log::info!("Paper processing completed in {}ms", processing_time.as_millis());
    
    Ok(paper_processing_result)
  }

  /// Extract and format the processing results from shared state
  fn extract_processing_result(
    &self, 
    original_url: &str,
    processing_time_ms: u64,
  ) -> Result<PaperProcessingResult> {
    // Extract ArXiv paper information
    let arxiv_output = self.shared_state.get("arxiv_fetch_output")
      .ok_or_else(|| anyhow::anyhow!("ArXiv fetch output not found"))?;
    
    let paper_id = arxiv_output["paper_id"].as_str()
      .unwrap_or("unknown")
      .to_string();

    // Extract Chinese summary
    let summary_output = self.shared_state.get("chinese_summary_output")
      .ok_or_else(|| anyhow::anyhow!("Chinese summary output not found"))?;
    
    let chinese_summary = summary_output["response"].as_str()
      .unwrap_or("Summary generation failed")
      .to_string();

    // Extract Chinese translation
    let translation_output = self.shared_state.get("chinese_translation_output")
      .ok_or_else(|| anyhow::anyhow!("Chinese translation output not found"))?;
    
    let chinese_translation = translation_output["response"].as_str()
      .unwrap_or("Translation failed")
      .to_string();

    // Extract mind maps
    let mind_maps = self.extract_mind_maps()?;

    // Extract poster image path
    let poster_image_path = self.shared_state.get("poster_image_output")
      .and_then(|output| {
        output.get("image_path")
          .and_then(|path| path.as_str())
          .map(|s| s.to_string())
      });

    Ok(PaperProcessingResult {
      paper_id,
      original_url: original_url.to_string(),
      chinese_summary,
      chinese_translation,
      mind_maps,
      poster_image_path,
      processing_time_ms,
      timestamp: chrono::Utc::now().to_rfc3339(),
    })
  }

  /// Extract mind map results from shared state
  fn extract_mind_maps(&self) -> Result<Vec<MindMapResult>> {
    let mut mind_maps = Vec::new();
    
    // Look for mind map outputs in shared state
    for (key, value) in self.shared_state.iter() {
      if key.contains("mind_map_") && key.ends_with("_output") {
        // Extract section information from the key
        // Expected format: "mind_map_section_N_output"
        let section_info = key.replace("mind_map_", "").replace("_output", "");
        
        let mind_map_result = MindMapResult {
          section_title: value["section_title"].as_str()
            .unwrap_or(&section_info)
            .to_string(),
          section_number: value["section_number"].as_str()
            .map(|s| s.to_string()),
          mind_map_html: value["html"].as_str()
            .unwrap_or("")
            .to_string(),
          mind_map_markdown: value["original_markdown"].as_str()
            .unwrap_or("")
            .to_string(),
        };
        
        mind_maps.push(mind_map_result);
      }
    }
    
    // Sort mind maps by section number if available
    mind_maps.sort_by(|a, b| {
      match (&a.section_number, &b.section_number) {
        (Some(a_num), Some(b_num)) => a_num.cmp(b_num),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.section_title.cmp(&b.section_title),
      }
    });
    
    Ok(mind_maps)
  }

  /// Save processing results to files
  pub async fn save_results(
    &self, 
    result: &PaperProcessingResult, 
    output_dir: &str,
  ) -> Result<()> {
    // Create output directory
    tokio::fs::create_dir_all(output_dir).await?;
    
    let base_filename = format!("{}_paper_assistant", result.paper_id.replace('/', "_"));
    
    // Save summary as markdown
    let summary_path = format!("{}/{}_summary.md", output_dir, base_filename);
    let summary_content = format!(
      "# 论文摘要\n\n**论文ID:** {}\n**处理时间:** {}\n\n## 中文摘要\n\n{}\n",
      result.paper_id,
      result.timestamp,
      result.chinese_summary
    );
    tokio::fs::write(&summary_path, summary_content).await?;
    
    // Save translation as markdown
    let translation_path = format!("{}/{}_translation.md", output_dir, base_filename);
    let translation_content = format!(
      "# 论文中文翻译\n\n**论文ID:** {}\n**原始URL:** {}\n**处理时间:** {}\n\n## 翻译内容\n\n{}\n",
      result.paper_id,
      result.original_url,
      result.timestamp,
      result.chinese_translation
    );
    tokio::fs::write(&translation_path, translation_content).await?;
    
    // Save mind maps
    for (i, mind_map) in result.mind_maps.iter().enumerate() {
      let mind_map_html_path = format!(
        "{}/{}_mindmap_{:02}__{}.html", 
        output_dir, 
        base_filename, 
        i + 1,
        mind_map.section_title.chars().take(20).collect::<String>()
          .replace(' ', "_")
          .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
      );
      
      tokio::fs::write(&mind_map_html_path, &mind_map.mind_map_html).await?;
    }
    
    // Save complete results as JSON
    let json_path = format!("{}/{}_complete_results.json", output_dir, base_filename);
    let json_content = serde_json::to_string_pretty(result)?;
    tokio::fs::write(&json_path, json_content).await?;
    
    log::info!("Results saved to directory: {}", output_dir);
    
    Ok(())
  }

  /// Get the current configuration
  pub fn config(&self) -> &PaperAssistantConfig {
    &self.config
  }

  /// Get access to shared state for debugging
  pub fn shared_state(&self) -> &SharedState {
    &self.shared_state
  }
}

impl Default for PaperAssistant {
  fn default() -> Self {
    Self::new().expect("Failed to create default PaperAssistant")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_paper_assistant_creation() {
    let assistant = PaperAssistant::new();
    assert!(assistant.is_ok());
  }

  #[test]
  fn test_mind_map_result_creation() {
    let mind_map = MindMapResult {
      section_title: "Introduction".to_string(),
      section_number: Some("1".to_string()),
      mind_map_html: "<html>test</html>".to_string(),
      mind_map_markdown: "# Introduction".to_string(),
    };
    
    assert_eq!(mind_map.section_title, "Introduction");
    assert_eq!(mind_map.section_number, Some("1".to_string()));
  }

  #[test]
  fn test_processing_result_serialization() {
    let result = PaperProcessingResult {
      paper_id: "2312.07104".to_string(),
      original_url: "https://arxiv.org/abs/2312.07104".to_string(),
      chinese_summary: "测试摘要".to_string(),
      chinese_translation: "测试翻译".to_string(),
      mind_maps: vec![],
      poster_image_path: None,
      processing_time_ms: 1500,
      timestamp: "2025-01-01T00:00:00Z".to_string(),
    };
    
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("2312.07104"));
    assert!(json.contains("测试摘要"));
  }
}