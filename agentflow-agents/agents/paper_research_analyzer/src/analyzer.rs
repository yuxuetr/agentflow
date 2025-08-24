//! Paper Research Analyzer Core Implementation

use crate::config::{AnalyzerConfig, AnalysisDepth};
use agentflow_agents::{
  AgentApplication, FileAgent, AgentResult, AgentConfig,
  AsyncFlow, SharedState, AgentFlow,
  StepFunPDFParser, BatchProcessor, default_batch_processor
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::collections::HashMap;

/// PDF Research Paper Analyzer
pub struct PDFAnalyzer {
  config: AnalyzerConfig,
  pdf_parser: StepFunPDFParser,
  batch_processor: BatchProcessor,
}

impl PDFAnalyzer {
  pub fn new(stepfun_api_key: String) -> Self {
    let mut config = AnalyzerConfig::default();
    config.stepfun_api_key = stepfun_api_key.clone();
    
    Self {
      pdf_parser: StepFunPDFParser::new(stepfun_api_key),
      batch_processor: default_batch_processor(),
      config,
    }
  }

  /// Builder pattern methods
  pub fn target_language(mut self, language: &str) -> Self {
    self.config.target_language = language.to_string();
    self
  }

  pub fn analysis_depth(mut self, depth: AnalysisDepth) -> Self {
    self.config.analysis_depth = depth;
    self
  }

  pub fn model(mut self, model: &str) -> Self {
    self.config.model = model.to_string();
    self
  }

  pub fn generate_mind_map(mut self, enable: bool) -> Self {
    self.config.generate_mind_map = enable;
    self
  }

  /// Get model capacity based on model name
  fn get_model_capacity(&self) -> usize {
    match self.config.model.as_str() {
      m if m.contains("qwen-turbo") || m.contains("qwen-plus-latest") || m.contains("qwen-long") => 800_000,
      m if m.contains("256k") => 200_000,
      m if m.contains("32k") => 80_000,
      m if m.contains("claude") => 180_000,
      m if m.contains("gpt-4o") => 120_000,
      _ => 30_000
    }
  }

  /// Analyze a single PDF research paper
  pub async fn analyze_paper<P: AsRef<Path>>(&self, pdf_path: P) -> AgentResult<AnalysisResult> {
    // Initialize AgentFlow LLM
    std::env::set_var("STEP_API_KEY", &self.config.stepfun_api_key);
    AgentFlow::init().await?;

    // Extract PDF content first
    let pdf_content = self.pdf_parser.extract_content(&pdf_path).await?;
    
    // Create workflow with analysis nodes
    let pdf_parser = crate::nodes::PDFParserNode::new(
      pdf_path.as_ref().to_path_buf(),
      self.config.stepfun_api_key.clone(),
      pdf_content.clone()
    );
    let mut flow = AsyncFlow::new(Box::new(pdf_parser));

    // Add workflow nodes based on configuration
    self.setup_workflow_nodes(&mut flow).await?;

    // Create shared state and add configuration markers
    let shared_state = SharedState::new();
    self.configure_shared_state(&shared_state);

    // Execute workflow
    let _execution_result = flow.run_async(&shared_state).await?;

    // Extract final results
    let final_result = shared_state.get("final_analysis")
      .ok_or("Analysis result not found")?
      .clone();
    
    let analysis_result = final_result
      .as_object()
      .ok_or("Invalid analysis result format")?;

    Ok(AnalysisResult::from_json(analysis_result.clone()))
  }

  /// Setup workflow nodes based on configuration
  async fn setup_workflow_nodes(&self, flow: &mut AsyncFlow) -> AgentResult<()> {
    // Summary Generation Node (always included)
    let summarizer = crate::nodes::SummaryNode::new(self.config.model.clone());
    flow.add_node("summarizer".to_string(), Box::new(summarizer));

    let has_insights = matches!(
      self.config.analysis_depth, 
      AnalysisDepth::Insights | AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation
    );
    let has_mindmap = self.config.generate_mind_map && matches!(
      self.config.analysis_depth, 
      AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation
    );
    let has_translation = matches!(self.config.analysis_depth, AnalysisDepth::WithTranslation) 
      && self.config.target_language != "en";

    // Key Insights Extraction Node (conditional)
    if has_insights {
      let insights_extractor = crate::nodes::InsightsNode::new(self.config.model.clone());
      flow.add_node("insights_extractor".to_string(), Box::new(insights_extractor));
    }

    // Mind Map Generation Node (conditional)
    if has_mindmap {
      let mind_mapper = crate::nodes::MindMapNode::new(self.config.model.clone());
      flow.add_node("mind_mapper".to_string(), Box::new(mind_mapper));
      
      // Add MarkMap Visualizer Node for visual output
      let markmap_visualizer = crate::nodes::MarkMapVisualizerNode::new("png".to_string())
        .with_auto_open(false)
        .with_output_dir("./analysis_output");
      flow.add_node("markmap_visualizer".to_string(), Box::new(markmap_visualizer));
    }

    // Translation Node (conditional)
    if has_translation {
      let translator = crate::nodes::TranslationNode::new(
        self.config.model.clone(), 
        self.config.target_language.clone()
      );
      flow.add_node("translator".to_string(), Box::new(translator));
    }

    // Results Compilation Node
    let compiler = crate::nodes::ResultsCompilerNode::new(self.config.analysis_depth.clone());
    flow.add_node("compiler".to_string(), Box::new(compiler));

    Ok(())
  }

  /// Configure shared state with workflow markers
  fn configure_shared_state(&self, shared_state: &SharedState) {
    let has_insights = matches!(
      self.config.analysis_depth, 
      AnalysisDepth::Insights | AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation
    );
    let has_mindmap = self.config.generate_mind_map && matches!(
      self.config.analysis_depth, 
      AnalysisDepth::Comprehensive | AnalysisDepth::WithTranslation
    );
    let has_translation = matches!(self.config.analysis_depth, AnalysisDepth::WithTranslation) 
      && self.config.target_language != "en";
    let has_visual_mindmap = has_mindmap; // Enable visual mind map when mind map is enabled
    
    shared_state.insert("has_insights".to_string(), Value::Bool(has_insights));
    shared_state.insert("has_mindmap".to_string(), Value::Bool(has_mindmap));
    shared_state.insert("has_translation".to_string(), Value::Bool(has_translation));
    shared_state.insert("has_visual_mindmap".to_string(), Value::Bool(has_visual_mindmap));
  }

  /// Batch process multiple PDF papers
  pub async fn analyze_batch<P: AsRef<Path>>(&self, pdf_directory: P) -> AgentResult<BatchAnalysisResult> {
    use agentflow_agents::{discover_files_with_extensions};
    
    // Find all PDF files in directory
    let pdf_files = discover_files_with_extensions(&pdf_directory, &["pdf"]).await?;
    
    println!("Found {} PDF files to process", pdf_files.len());

    if pdf_files.is_empty() {
      return Ok(BatchAnalysisResult {
        successful_analyses: Vec::new(),
        failed_analyses: Vec::new(),
        total_processed: 0,
      });
    }

    // Process files with progress reporting
    let analyzer = self.clone();
    let results = self.batch_processor.process_with_progress(
      pdf_files,
      move |pdf_path| {
        let analyzer = analyzer.clone();
        async move {
          analyzer.analyze_paper(&pdf_path).await
        }
      },
      |completed, total| {
        println!("Progress: {}/{} files processed", completed, total);
      }
    ).await;

    // Separate successful and failed analyses
    let mut successful_analyses = Vec::new();
    let mut failed_analyses = Vec::new();

    for (pdf_path, result) in results {
      match result {
        Ok(analysis) => successful_analyses.push((pdf_path, analysis)),
        Err(e) => failed_analyses.push((pdf_path, e.to_string())),
      }
    }

    let total_processed = successful_analyses.len() + failed_analyses.len();
    Ok(BatchAnalysisResult {
      successful_analyses,
      failed_analyses,
      total_processed,
    })
  }
}

impl Clone for PDFAnalyzer {
  fn clone(&self) -> Self {
    Self {
      config: self.config.clone(),
      pdf_parser: StepFunPDFParser::new(self.config.stepfun_api_key.clone()),
      batch_processor: default_batch_processor(),
    }
  }
}

#[async_trait]
impl AgentApplication for PDFAnalyzer {
  type Config = AnalyzerConfig;
  type Result = AnalysisResult;

  async fn initialize(config: Self::Config) -> AgentResult<Self> {
    config.validate()?;
    
    let pdf_parser = StepFunPDFParser::new(config.stepfun_api_key.clone());
    let batch_processor = BatchProcessor::new(config.concurrency_limit);
    
    Ok(Self {
      config,
      pdf_parser,
      batch_processor,
    })
  }

  async fn execute(&self, input: &str) -> AgentResult<Self::Result> {
    self.analyze_paper(input).await
  }

  fn name(&self) -> &'static str {
    "paper-research-analyzer"
  }
}

#[async_trait]
impl FileAgent for PDFAnalyzer {
  async fn process_file<P: AsRef<Path> + Send + Sync>(&self, file_path: P) -> AgentResult<Self::Result> {
    self.analyze_paper(file_path).await
  }

  async fn process_directory<P: AsRef<Path> + Send + Sync>(&self, directory: P) -> AgentResult<Vec<(std::path::PathBuf, Self::Result)>> {
    let batch_result = self.analyze_batch(directory).await?;
    Ok(batch_result.successful_analyses)
  }

  fn supported_extensions(&self) -> Vec<&'static str> {
    vec!["pdf"]
  }
}

/// Analysis Result Structure
#[derive(Debug, Clone, Serialize, Deserialize)]
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
  pub fn from_json(value: serde_json::Map<String, Value>) -> Self {
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
  pub async fn save_to_files<P: AsRef<Path>>(&self, output_dir: P) -> AgentResult<()> {
    use agentflow_agents::{format_json_pretty, save_comprehensive_output};
    
    let mut outputs = Vec::new();
    
    // Summary as markdown
    if let Some(summary) = &self.summary {
      outputs.push(("summary".to_string(), summary.clone(), "md".to_string()));
    }

    // Insights as JSON
    if let Some(insights) = &self.key_insights {
      let insights_pretty = format_json_pretty(insights)?;
      outputs.push(("key_insights".to_string(), insights_pretty, "json".to_string()));
    }

    // Mind map as markdown (MarkMap format)
    if let Some(mind_map) = &self.mind_map {
      outputs.push(("mind_map".to_string(), mind_map.clone(), "md".to_string()));
    }

    // Translation
    if let Some(translation) = &self.translated_summary {
      let lang = self.target_language.as_deref().unwrap_or("unknown");
      outputs.push((format!("summary_{}", lang), translation.clone(), "md".to_string()));
    }

    // Complete analysis as JSON
    let complete_analysis = json!({
      "summary": self.summary,
      "key_insights": self.key_insights,
      "mind_map": self.mind_map,
      "translated_summary": self.translated_summary,
      "target_language": self.target_language,
      "processing_stats": self.processing_stats,
      "metadata": self.metadata
    });
    let analysis_pretty = format_json_pretty(&complete_analysis)?;
    outputs.push(("complete_analysis".to_string(), analysis_pretty, "json".to_string()));

    save_comprehensive_output(output_dir, "Analysis", &outputs).await?;
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
  pub async fn save_to_directory<P: AsRef<Path>>(&self, output_dir: P) -> AgentResult<()> {
    use agentflow_agents::{create_timestamped_output_dir, save_content, format_json_pretty};
    
    let final_output_dir = create_timestamped_output_dir(&output_dir, "batch_analysis").await?;

    // Save individual results
    for (pdf_path, analysis) in &self.successful_analyses {
      let filename = pdf_path.file_stem().unwrap().to_string_lossy();
      let result_dir = final_output_dir.join(&*filename);
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
      "successful_files": self.successful_analyses.iter()
        .map(|(path, _)| path.file_name().unwrap().to_string_lossy())
        .collect::<Vec<_>>(),
      "failed_files": self.failed_analyses.iter()
        .map(|(path, error)| json!({
          "filename": path.file_name().unwrap().to_string_lossy(),
          "error": error
        }))
        .collect::<Vec<_>>()
    });

    let report_pretty = format_json_pretty(&batch_report)?;
    let report_path = final_output_dir.join("batch_analysis_report.json");
    save_content(report_path, &report_pretty).await?;

    println!("✅ Batch analysis results saved to: {}", final_output_dir.display());
    Ok(())
  }
}