//! Configuration for Paper Research Analyzer

use agentflow_agents::{AgentConfig, AgentResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerConfig {
  pub stepfun_api_key: String,
  pub target_language: String,
  pub analysis_depth: AnalysisDepth,
  pub generate_mind_map: bool,
  pub model: String,
  pub concurrency_limit: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AnalysisDepth {
  Summary,      // Generate summary only
  Insights,     // Extract key insights only  
  Comprehensive, // Full analysis with summary + insights + mind map
  WithTranslation, // Everything + translation
}

impl Default for AnalyzerConfig {
  fn default() -> Self {
    Self {
      stepfun_api_key: String::new(),
      target_language: "en".to_string(),
      analysis_depth: AnalysisDepth::Comprehensive,
      generate_mind_map: true,
      model: "step-2-16k".to_string(),
      concurrency_limit: 3,
    }
  }
}

impl AgentConfig for AnalyzerConfig {
  fn validate(&self) -> AgentResult<()> {
    if self.stepfun_api_key.is_empty() {
      return Err("StepFun API key is required".into());
    }
    
    if self.concurrency_limit == 0 {
      return Err("Concurrency limit must be greater than 0".into());
    }
    
    Ok(())
  }
}