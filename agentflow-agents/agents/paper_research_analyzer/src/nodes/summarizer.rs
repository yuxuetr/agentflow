//! Summary Generation Node - Create comprehensive research paper summary

use agentflow_agents::{AsyncNode, SharedState, AgentFlowError, AgentFlow};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct SummaryNode {
  model: String,
}

impl SummaryNode {
  pub fn new(model: String) -> Self {
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
      .max_tokens(2000)
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