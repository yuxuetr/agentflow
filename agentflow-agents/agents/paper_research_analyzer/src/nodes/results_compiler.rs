//! Results Compiler Node - Compile all analysis results into final output

use crate::config::AnalysisDepth;
use agentflow_agents::{AsyncNode, SharedState, AgentFlowError};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct ResultsCompilerNode {
  analysis_depth: AnalysisDepth,
}

impl ResultsCompilerNode {
  pub fn new(analysis_depth: AnalysisDepth) -> Self {
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