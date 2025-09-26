//! Configuration for Paper Assistant
//!
//! This module defines configuration structures and default values for the
//! paper processing workflow.

use serde::{Deserialize, Serialize};
use anyhow;

/// Configuration for Paper Assistant workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperAssistantConfig {
  // Model configurations
  pub qwen_turbo_model: String,
  pub qwen_image_model: String,
  
  // LLM parameters
  pub temperature: Option<f32>,
  pub max_tokens: Option<u32>,
  
  // Output configuration
  pub output_directory: String,
  pub save_intermediate_files: bool,
  
  // Prompt templates
  pub chinese_summary_prompt: String,
  pub chinese_translation_prompt: String,
  pub section_extraction_prompt: String,
  pub poster_generation_prompt: String,
  
  // Processing options
  pub enable_mind_maps: bool,
  pub enable_poster_generation: bool,
  pub max_sections_for_mind_maps: Option<usize>,
  
  // ArXiv processing options
  pub extract_latex_files: bool,
  pub expand_latex_content: bool,
  pub arxiv_timeout_seconds: Option<u64>,
}

impl Default for PaperAssistantConfig {
  fn default() -> Self {
    Self {
      // Use Qwen models (DashScope API)
      qwen_turbo_model: "qwen-turbo".to_string(),
      qwen_image_model: "qwen-vl-plus".to_string(), // Use VL model for image generation
      
      // LLM parameters optimized for Chinese output
      temperature: Some(0.3),
      max_tokens: Some(4000),
      
      // Output configuration
      output_directory: "./paper_assistant_output".to_string(),
      save_intermediate_files: true,
      
      // Chinese summary prompt
      chinese_summary_prompt: r#"请仔细阅读以下学术论文内容，并生成一个详细的中文摘要。摘要应该包括：

1. 研究背景和动机
2. 主要研究方法
3. 关键创新点
4. 实验结果和发现
5. 结论和意义

论文内容：
{{paper_content}}

请生成专业、准确的中文摘要（约500-800字）："#.to_string(),

      // Chinese translation prompt
      chinese_translation_prompt: r#"请将以下学术论文翻译成中文。翻译要求：

1. 保持学术论文的专业性和准确性
2. 保留原文的段落结构和格式
3. 专业术语使用准确的中文表达
4. 保持逻辑清晰，语言流畅
5. 对于重要的英文术语，可以在中文后标注英文原文

论文原文：
{{paper_content}}

请提供完整的中文翻译："#.to_string(),

      // Section extraction prompt
      section_extraction_prompt: r#"请分析以下学术论文内容，提取出主要章节的结构和内容。对于每个章节，请提供：

1. 章节标题（中文翻译）
2. 章节编号（如果有）
3. 章节主要内容摘要（中文，约200字）

论文内容：
{{paper_content}}

请按照以下格式输出每个章节：

## 章节 [编号]：[中文标题]
### 内容摘要
[章节内容的中文摘要]

---"#.to_string(),

      // Poster generation prompt
      poster_generation_prompt: r#"Create an academic research poster design based on this Chinese research summary. Design requirements:

Title: {{paper_title}}
Summary: {{chinese_summary}}

Design a professional academic poster with:
1. Clear, readable layout with the paper title at the top
2. Main research highlights and key findings prominently displayed
3. Clean, modern academic design with appropriate color scheme
4. Visual elements that support the research content
5. Professional typography suitable for academic presentation

Style: Clean, modern academic poster design"#.to_string(),

      // Processing options
      enable_mind_maps: true,
      enable_poster_generation: true,
      max_sections_for_mind_maps: Some(10),
      
      // ArXiv options
      extract_latex_files: true,
      expand_latex_content: true,
      arxiv_timeout_seconds: Some(120),
    }
  }
}

impl PaperAssistantConfig {
  /// Create a new configuration with custom model names
  pub fn with_models(qwen_turbo: &str, qwen_image: &str) -> Self {
    let mut config = Self::default();
    config.qwen_turbo_model = qwen_turbo.to_string();
    config.qwen_image_model = qwen_image.to_string();
    config
  }

  /// Create a new configuration with custom output directory
  pub fn with_output_directory(output_dir: &str) -> Self {
    let mut config = Self::default();
    config.output_directory = output_dir.to_string();
    config
  }

  /// Create a configuration optimized for fast processing
  pub fn fast_processing() -> Self {
    let mut config = Self::default();
    config.max_tokens = Some(2000);
    config.temperature = Some(0.1);
    config.max_sections_for_mind_maps = Some(5);
    config.enable_poster_generation = false; // Skip image generation for speed
    config
  }

  /// Create a configuration optimized for comprehensive analysis
  pub fn comprehensive_analysis() -> Self {
    let mut config = Self::default();
    config.max_tokens = Some(8000);
    config.temperature = Some(0.3);
    config.max_sections_for_mind_maps = Some(15);
    config.enable_mind_maps = true;
    config.enable_poster_generation = true;
    config.save_intermediate_files = true;
    config
  }

  /// Validate the configuration
  pub fn validate(&self) -> Result<(), String> {
    if self.qwen_turbo_model.is_empty() {
      return Err("qwen_turbo_model cannot be empty".to_string());
    }

    if self.qwen_image_model.is_empty() && self.enable_poster_generation {
      return Err("qwen_image_model cannot be empty when poster generation is enabled".to_string());
    }

    if let Some(temp) = self.temperature {
      if temp < 0.0 || temp > 2.0 {
        return Err("temperature must be between 0.0 and 2.0".to_string());
      }
    }

    if let Some(max_tokens) = self.max_tokens {
      if max_tokens < 100 || max_tokens > 32000 {
        return Err("max_tokens must be between 100 and 32000".to_string());
      }
    }

    if self.output_directory.is_empty() {
      return Err("output_directory cannot be empty".to_string());
    }

    Ok(())
  }

  /// Load configuration from JSON file
  pub fn from_json_file(path: &str) -> anyhow::Result<Self> {
    let content = std::fs::read_to_string(path)?;
    let config: Self = serde_json::from_str(&content)?;
    config.validate().map_err(|e| anyhow::anyhow!("Config validation failed: {}", e))?;
    Ok(config)
  }

  /// Save configuration to JSON file
  pub fn to_json_file(&self, path: &str) -> anyhow::Result<()> {
    self.validate().map_err(|e| anyhow::anyhow!("Config validation failed: {}", e))?;
    let content = serde_json::to_string_pretty(self)?;
    std::fs::write(path, content)?;
    Ok(())
  }

  /// Update prompts with custom templates
  pub fn with_custom_prompts(
    mut self,
    summary_prompt: Option<String>,
    translation_prompt: Option<String>,
    section_prompt: Option<String>,
    poster_prompt: Option<String>,
  ) -> Self {
    if let Some(prompt) = summary_prompt {
      self.chinese_summary_prompt = prompt;
    }
    if let Some(prompt) = translation_prompt {
      self.chinese_translation_prompt = prompt;
    }
    if let Some(prompt) = section_prompt {
      self.section_extraction_prompt = prompt;
    }
    if let Some(prompt) = poster_prompt {
      self.poster_generation_prompt = prompt;
    }
    self
  }
}

/// Environment-based configuration builder
pub struct ConfigBuilder {
  config: PaperAssistantConfig,
}

impl ConfigBuilder {
  /// Create a new config builder with defaults
  pub fn new() -> Self {
    Self {
      config: PaperAssistantConfig::default(),
    }
  }

  /// Set models from environment variables
  pub fn from_env(mut self) -> Self {
    // Check for model overrides in environment
    if let Ok(turbo_model) = std::env::var("QWEN_TURBO_MODEL") {
      self.config.qwen_turbo_model = turbo_model;
    }
    
    if let Ok(image_model) = std::env::var("QWEN_IMAGE_MODEL") {
      self.config.qwen_image_model = image_model;
    }
    
    // Check for output directory override
    if let Ok(output_dir) = std::env::var("PAPER_ASSISTANT_OUTPUT_DIR") {
      self.config.output_directory = output_dir;
    }
    
    // Check for temperature override
    if let Ok(temp_str) = std::env::var("PAPER_ASSISTANT_TEMPERATURE") {
      if let Ok(temp) = temp_str.parse::<f32>() {
        self.config.temperature = Some(temp);
      }
    }
    
    // Check for max tokens override
    if let Ok(tokens_str) = std::env::var("PAPER_ASSISTANT_MAX_TOKENS") {
      if let Ok(tokens) = tokens_str.parse::<u32>() {
        self.config.max_tokens = Some(tokens);
      }
    }
    
    self
  }

  /// Build the configuration
  pub fn build(self) -> anyhow::Result<PaperAssistantConfig> {
    self.config.validate().map_err(|e| anyhow::anyhow!("Config validation failed: {}", e))?;
    Ok(self.config)
  }
}

impl Default for ConfigBuilder {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_default_config() {
    let config = PaperAssistantConfig::default();
    assert_eq!(config.qwen_turbo_model, "qwen-turbo");
    assert_eq!(config.qwen_image_model, "qwen-vl-plus");
    assert_eq!(config.temperature, Some(0.3));
    assert!(config.enable_mind_maps);
    assert!(config.enable_poster_generation);
  }

  #[test]
  fn test_config_validation() {
    let mut config = PaperAssistantConfig::default();
    
    // Valid config should pass
    assert!(config.validate().is_ok());
    
    // Invalid temperature should fail
    config.temperature = Some(-1.0);
    assert!(config.validate().is_err());
    
    config.temperature = Some(0.5);
    assert!(config.validate().is_ok());
    
    // Empty model should fail
    config.qwen_turbo_model = "".to_string();
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_config_builder() {
    let config = ConfigBuilder::new().build();
    assert!(config.is_ok());
  }

  #[test]
  fn test_fast_processing_config() {
    let config = PaperAssistantConfig::fast_processing();
    assert_eq!(config.max_tokens, Some(2000));
    assert_eq!(config.temperature, Some(0.1));
    assert_eq!(config.max_sections_for_mind_maps, Some(5));
    assert!(!config.enable_poster_generation);
  }

  #[test]
  fn test_comprehensive_analysis_config() {
    let config = PaperAssistantConfig::comprehensive_analysis();
    assert_eq!(config.max_tokens, Some(8000));
    assert_eq!(config.max_sections_for_mind_maps, Some(15));
    assert!(config.enable_mind_maps);
    assert!(config.enable_poster_generation);
    assert!(config.save_intermediate_files);
  }

  #[test]
  fn test_custom_models() {
    let config = PaperAssistantConfig::with_models("custom-turbo", "custom-image");
    assert_eq!(config.qwen_turbo_model, "custom-turbo");
    assert_eq!(config.qwen_image_model, "custom-image");
  }
}