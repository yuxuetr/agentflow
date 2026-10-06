//! Key Insights Extraction Node - Extract structured metadata and insights

use yanshi_agents::{AsyncNode, SharedState, YanshiError, Yanshi};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct InsightsNode {
  model: String,
}

impl InsightsNode {
  pub fn new(model: String) -> Self {
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
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, YanshiError> {
    let content = shared.get("pdf_content").ok_or_else(|| 
      YanshiError::AsyncExecutionError { message: "PDF content not available".to_string() })?;
    
    Ok(json!({
      "content": content,
      "model": self.model
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, YanshiError> {
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

    let response = Yanshi::model(&self.model)
      .prompt(&insights_prompt)
      .temperature(0.2)
      .max_tokens(1500)
      .execute()
      .await
      .map_err(|e| YanshiError::AsyncExecutionError { 
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

  async fn post_async(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>, YanshiError> {
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