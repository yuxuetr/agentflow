//! Mind Map Generation Node - Create MarkMap mind map visualization

use agentflow_agents::{AsyncNode, SharedState, AgentFlowError, AgentFlow};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct MindMapNode {
  model: String,
}

impl MindMapNode {
  pub fn new(model: String) -> Self {
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
基于提取的研究洞察，创建一个MarkMap思维导图（使用中文）。
重点关注主要概念、方法论、发现和关系。

使用以下MarkMap格式（分层markdown结构）:

# 研究论文标题

## 研究问题
- 具体问题领域1
- 具体问题领域2
- 研究背景

## 研究方法  
- 方法论1
- 方法论2
- 数据收集方式

## 主要发现
- 关键结果1
- 关键结果2
- 重要发现

## 研究意义与影响
- 理论贡献
- 实践应用
- 未来研究方向

## 局限性
- 研究局限1
- 研究局限2

请用中文创建具体的思维导图内容，基于以下论文洞察：
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