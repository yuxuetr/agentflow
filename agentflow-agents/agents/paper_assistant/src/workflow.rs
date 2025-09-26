//! Paper Assistant Workflow Implementation
//!
//! This module defines the workflow for processing arXiv papers with Chinese translation,
//! summarization, mind mapping, and poster generation.

use agentflow_core::{AsyncFlow, AsyncNode, SharedState, AgentFlowError};
use agentflow_nodes::{ArxivNode, LlmNode, MarkMapNode, TextToImageNode};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::config::PaperAssistantConfig;
use crate::utils::{extract_paper_sections, create_section_markdown};

/// Paper Assistant Workflow orchestrates the complete paper processing pipeline
#[derive(Debug)]
pub struct PaperAssistantWorkflow {
  /// ArXiv paper fetching node
  arxiv_node: ArxivNode,
  /// LLM node for Chinese summary generation
  summary_node: LlmNode,
  /// LLM node for Chinese translation
  translation_node: LlmNode,
  /// LLM node for section content extraction
  section_extraction_node: LlmNode,
  /// Text-to-image node for poster generation
  poster_node: TextToImageNode,
  /// Configuration
  config: PaperAssistantConfig,
}

impl PaperAssistantWorkflow {
  /// Create a new workflow with the given configuration
  pub fn new(config: &PaperAssistantConfig) -> Result<Self> {
    // Create ArXiv node
    let arxiv_node = ArxivNode::new("arxiv_fetch", "{{arxiv_url}}")
      .with_output_key("arxiv_fetch_output")
      .with_output_directory(&config.output_directory);

    // Create Chinese summary LLM node
    let summary_node = LlmNode::new("chinese_summary", &config.qwen_turbo_model)
      .with_prompt(&config.chinese_summary_prompt)
      .with_temperature(config.temperature.unwrap_or(0.3))
      .with_max_tokens(config.max_tokens.unwrap_or(4000))
      .with_output_key("chinese_summary_output")
      .with_input_keys(vec!["paper_content".to_string()]);

    // Create Chinese translation LLM node
    let translation_node = LlmNode::new("chinese_translation", &config.qwen_turbo_model)
      .with_prompt(&config.chinese_translation_prompt)
      .with_temperature(config.temperature.unwrap_or(0.3))
      .with_max_tokens(config.max_tokens.unwrap_or(8000))
      .with_output_key("chinese_translation_output")
      .with_input_keys(vec!["paper_content".to_string()]);

    // Create section extraction LLM node
    let section_extraction_node = LlmNode::new("section_extraction", &config.qwen_turbo_model)
      .with_prompt(&config.section_extraction_prompt)
      .with_temperature(config.temperature.unwrap_or(0.2))
      .with_max_tokens(config.max_tokens.unwrap_or(6000))
      .with_output_key("sections_output")
      .with_input_keys(vec!["paper_content".to_string()]);

    // Create poster generation node
    let poster_node = TextToImageNode::new("poster_generation", &config.qwen_image_model)
      .with_prompt(&config.poster_generation_prompt)
      .with_output_key("poster_image_output")
      .with_input_keys(vec!["chinese_summary".to_string(), "paper_title".to_string()])
      .with_size("1024x1024");

    Ok(Self {
      arxiv_node,
      summary_node,
      translation_node,
      section_extraction_node,
      poster_node,
      config: config.clone(),
    })
  }

  /// Execute the complete paper processing workflow
  pub async fn execute(&mut self, shared_state: &SharedState) -> Result<Value> {
    log::info!("Starting paper assistant workflow execution");

    // Step 1: Fetch paper from arXiv
    log::info!("Step 1: Fetching paper from arXiv");
    self.arxiv_node.run_async(shared_state).await
      .map_err(|e| anyhow::anyhow!("ArXiv fetch failed: {}", e))?;

    // Extract paper content for subsequent processing
    let arxiv_output = shared_state.get("arxiv_fetch_output")
      .ok_or_else(|| anyhow::anyhow!("ArXiv output not found"))?;

    // Get the best available content (expanded LaTeX or simple content)
    let paper_content = self.extract_paper_content(&arxiv_output)?;
    shared_state.insert("paper_content".to_string(), json!(paper_content));

    // Extract paper title for poster generation
    let paper_title = self.extract_paper_title(&paper_content);
    shared_state.insert("paper_title".to_string(), json!(paper_title));

    // Step 2: Generate Chinese summary
    log::info!("Step 2: Generating Chinese summary");
    self.summary_node.run_async(shared_state).await
      .map_err(|e| anyhow::anyhow!("Chinese summary generation failed: {}", e))?;

    // Step 3: Generate Chinese translation
    log::info!("Step 3: Generating Chinese translation");
    self.translation_node.run_async(shared_state).await
      .map_err(|e| anyhow::anyhow!("Chinese translation failed: {}", e))?;

    // Step 4: Extract paper sections for mind mapping
    log::info!("Step 4: Extracting paper sections");
    self.section_extraction_node.run_async(shared_state).await
      .map_err(|e| anyhow::anyhow!("Section extraction failed: {}", e))?;

    // Step 5: Generate mind maps for each section
    log::info!("Step 5: Generating mind maps for sections");
    self.generate_section_mind_maps(shared_state).await?;

    // Step 6: Generate poster image
    log::info!("Step 6: Generating poster image");
    
    // Prepare summary for poster generation
    let summary_output = match shared_state.get("chinese_summary_output") {
      Some(output) => match output.get("response") {
        Some(response) => response.as_str().unwrap_or("无摘要可用").to_string(),
        None => "无摘要可用".to_string(),
      },
      None => "无摘要可用".to_string(),
    };
    
    shared_state.insert("chinese_summary".to_string(), json!(summary_output));

    self.poster_node.run_async(shared_state).await
      .map_err(|e| anyhow::anyhow!("Poster generation failed: {}", e))?;

    log::info!("Paper assistant workflow completed successfully");

    Ok(json!({
      "status": "completed",
      "workflow": "paper_assistant",
      "timestamp": chrono::Utc::now().to_rfc3339()
    }))
  }

  /// Extract the best available paper content from ArXiv output
  fn extract_paper_content(&self, arxiv_output: &Value) -> Result<String> {
    // Try to get expanded LaTeX content first (most comprehensive)
    if let Some(latex_info) = arxiv_output.get("latex_info") {
      if let Some(expanded_content) = latex_info.get("expanded_content") {
        if let Some(content) = expanded_content.as_str() {
          if !content.trim().is_empty() {
            return Ok(content.to_string());
          }
        }
      }
      
      // Fall back to main content
      if let Some(main_content) = latex_info.get("main_content") {
        if let Some(content) = main_content.as_str() {
          if !content.trim().is_empty() {
            return Ok(content.to_string());
          }
        }
      }
    }

    // Fall back to simple LaTeX content
    if let Some(simple_content) = arxiv_output.get("simple_latex_content") {
      if let Some(content) = simple_content.as_str() {
        if !content.trim().is_empty() {
          return Ok(content.to_string());
        }
      }
    }

    Err(anyhow::anyhow!("No usable paper content found in ArXiv output"))
  }

  /// Extract paper title from LaTeX content
  fn extract_paper_title(&self, paper_content: &str) -> String {
    // Look for \title{...} in LaTeX content
    if let Some(start) = paper_content.find("\\title{") {
      let title_start = start + 7; // Length of "\title{"
      if let Some(end) = paper_content[title_start..].find('}') {
        let title = &paper_content[title_start..title_start + end];
        // Clean up LaTeX commands and return
        return self.clean_latex_text(title);
      }
    }

    // Fall back to first line or default
    paper_content.lines()
      .next()
      .map(|line| self.clean_latex_text(line))
      .filter(|line| !line.trim().is_empty())
      .unwrap_or_else(|| "未知论文标题".to_string())
  }

  /// Clean LaTeX text by removing common commands
  fn clean_latex_text(&self, text: &str) -> String {
    text.replace("\\textbf{", "")
        .replace("\\textit{", "")
        .replace("\\emph{", "")
        .replace("\\section{", "")
        .replace("\\subsection{", "")
        .replace("\\subsubsection{", "")
        .replace('}', "")
        .replace('\\', "")
        .trim()
        .to_string()
  }

  /// Generate mind maps for each extracted section
  async fn generate_section_mind_maps(&self, shared_state: &SharedState) -> Result<()> {
    let sections_output = shared_state.get("section_extraction_output")
      .ok_or_else(|| anyhow::anyhow!("Sections output not found"))?;

    let sections_text = sections_output.as_str()
      .ok_or_else(|| anyhow::anyhow!("No sections text in output"))?;

    // Parse sections from the extraction output
    let sections = extract_paper_sections(sections_text)?;
    
    log::info!("Found {} sections for mind mapping", sections.len());

    for (i, section) in sections.iter().enumerate() {
      log::info!("Generating mind map for section {}: {}", i + 1, section.title);

      // Create markdown content for this section
      let section_markdown = create_section_markdown(&section.title, &section.content);
      
      // Create a MarkMap node for this section
      let mut markmap_node = MarkMapNode::new(
        &format!("mind_map_section_{}", i + 1),
        &section_markdown,
      )
      .with_output_key(format!("mind_map_section_{}_output", i + 1))
      .with_file_output(format!(
        "{}/mind_map_section_{}_{}.html",
        self.config.output_directory,
        i + 1,
        section.title.chars().take(20).collect::<String>()
          .replace(' ', "_")
          .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
      ));

      // Set section metadata in shared state
      shared_state.insert(format!("section_{}_title", i + 1), json!(section.title));
      shared_state.insert(format!("section_{}_number", i + 1), json!(section.number));

      // Execute the MarkMap node
      match markmap_node.run_async(shared_state).await {
        Ok(_) => {
          log::info!("Successfully generated mind map for section {}", i + 1);
          // Note: Section metadata was already set before executing the node
        },
        Err(e) => {
          log::warn!("Failed to generate mind map for section {}: {}", i + 1, e);
          // Continue with other sections even if one fails
        }
      }
    }

    Ok(())
  }
}

/// Represents a paper section for mind mapping
#[derive(Debug, Clone)]
pub struct PaperSection {
  pub title: String,
  pub number: Option<String>,
  pub content: String,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::PaperAssistantConfig;

  #[test]
  fn test_workflow_creation() {
    let config = PaperAssistantConfig::default();
    let workflow = PaperAssistantWorkflow::new(&config);
    assert!(workflow.is_ok());
  }

  #[test]
  fn test_paper_title_extraction() {
    let config = PaperAssistantConfig::default();
    let workflow = PaperAssistantWorkflow::new(&config).unwrap();
    
    let latex_content = r#"\documentclass{article}
\title{A Great Paper About Machine Learning}
\author{John Doe}
\begin{document}"#;
    
    let title = workflow.extract_paper_title(latex_content);
    assert_eq!(title, "A Great Paper About Machine Learning");
  }

  #[test]
  fn test_latex_text_cleaning() {
    let config = PaperAssistantConfig::default();
    let workflow = PaperAssistantWorkflow::new(&config).unwrap();
    
    let dirty_text = r#"\textbf{Bold Text} and \textit{Italic Text}"#;
    let clean_text = workflow.clean_latex_text(dirty_text);
    assert_eq!(clean_text, "Bold Text and Italic Text");
  }

  #[test]
  fn test_paper_section_creation() {
    let section = PaperSection {
      title: "Introduction".to_string(),
      number: Some("1".to_string()),
      content: "This is the introduction section.".to_string(),
    };
    
    assert_eq!(section.title, "Introduction");
    assert_eq!(section.number, Some("1".to_string()));
    assert!(section.content.contains("introduction"));
  }
}