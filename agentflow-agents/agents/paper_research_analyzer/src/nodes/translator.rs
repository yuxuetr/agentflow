//! Translation Node - Translate summary to target language

use agentflow_agents::{AsyncNode, SharedState, AgentFlowError, AgentFlow};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct TranslationNode {
  model: String,
  target_language: String,
}

impl TranslationNode {
  pub fn new(model: String, target_language: String) -> Self {
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