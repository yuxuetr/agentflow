use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Batch node for parallel processing of multiple items
#[derive(Clone)]
pub struct BatchNode {
  pub name: String,
  pub items_key: String, // Key in shared state containing array of items to process
  pub batch_size: usize,
  pub max_concurrent: usize,
  /// The inner node executed once per batch item. Skipped in [`Debug`]
  /// output because `AsyncNode` is a trait object (no `Debug` bound).
  pub child_node: Option<Arc<dyn AsyncNode>>,
}

impl std::fmt::Debug for BatchNode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("BatchNode")
      .field("name", &self.name)
      .field("items_key", &self.items_key)
      .field("batch_size", &self.batch_size)
      .field("max_concurrent", &self.max_concurrent)
      .field(
        "child_node",
        &self.child_node.as_ref().map(|_| "<AsyncNode>"),
      )
      .finish()
  }
}

impl BatchNode {
  pub fn new(name: &str, items_key: &str) -> Self {
    Self {
      name: name.to_string(),
      items_key: items_key.to_string(),
      batch_size: 10,
      max_concurrent: 4,
      child_node: None,
    }
  }

  pub fn with_batch_size(mut self, size: usize) -> Self {
    self.batch_size = size;
    self
  }

  pub fn with_max_concurrent(mut self, max: usize) -> Self {
    self.max_concurrent = max;
    self
  }

  pub fn with_child_node(mut self, node: Arc<dyn AsyncNode>) -> Self {
    self.child_node = Some(node);
    self
  }

  async fn process_batch(&self, items: &[FlowValue]) -> Result<Vec<FlowValue>, AgentFlowError> {
    let mut results = Vec::new();

    if let Some(child_node) = &self.child_node {
      let mut tasks = JoinSet::new();

      for item in items.iter() {
        if tasks.len() >= self.max_concurrent
          && let Some(task_result) = tasks.join_next().await
        {
          match task_result {
            Ok(result) => results.push(result?),
            Err(e) => {
              return Err(AgentFlowError::AsyncExecutionError {
                message: format!("Batch processing task failed: {}", e),
              });
            }
          }
        }

        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("item".to_string(), item.clone());

        let child_clone = Arc::clone(child_node);

        tasks.spawn(async move { child_clone.execute(&inputs).await });
      }

      while let Some(task_result) = tasks.join_next().await {
        match task_result {
          Ok(result) => results.push(result?),
          Err(e) => {
            return Err(AgentFlowError::AsyncExecutionError {
              message: format!("Batch processing task failed: {}", e),
            });
          }
        }
      }
    } else {
      results = items
        .iter()
        .map(|i| {
          let mut res = HashMap::new();
          res.insert("output".to_string(), i.clone());
          res
        })
        .collect();
    }

    let flattened_results = results
      .into_iter()
      .map(|h| h.get("output").unwrap().clone())
      .collect();

    Ok(flattened_results)
  }
}

#[async_trait]
impl AsyncNode for BatchNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let items = match inputs.get(&self.items_key) {
      Some(FlowValue::Json(Value::Array(arr))) => arr
        .iter()
        .map(|v| FlowValue::Json(v.clone()))
        .collect::<Vec<_>>(),
      _ => {
        return Err(AgentFlowError::NodeInputError {
          message: format!("Input '{}' is missing or not an array", self.items_key),
        });
      }
    };

    println!("🔧 Batch Node '{}' prepared:", self.name);
    println!("   Items key: {}", self.items_key);
    println!("   Item count: {}", items.len());
    println!("   Batch size: {}", self.batch_size);
    println!("   Max concurrent: {}", self.max_concurrent);

    let mut all_results = Vec::new();
    for batch in items.chunks(self.batch_size) {
      let batch_results = self.process_batch(batch).await?;
      all_results.extend(batch_results);
    }

    println!(
      "✅ Batch processing complete. {} results",
      all_results.len()
    );

    // Flatten the Vec<FlowValue> into a plain JSON array — the custom
    // FlowValue Serialize impl would otherwise wrap each entry in
    // `{type, value}`, which is the trace envelope shape, not the data
    // shape downstream nodes consume.
    let array: Vec<Value> = all_results
      .into_iter()
      .map(|v| match v {
        FlowValue::Json(inner) => inner,
        other => serde_json::to_value(other).unwrap_or(Value::Null),
      })
      .collect();
    let mut outputs = HashMap::new();
    outputs.insert("results".to_string(), FlowValue::Json(Value::Array(array)));

    Ok(outputs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  struct MockChildNode;

  #[async_trait]
  impl AsyncNode for MockChildNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      let item = inputs.get("item").unwrap();
      let mut result = HashMap::new();
      result.insert("output".to_string(), item.clone());
      Ok(result)
    }
  }

  #[tokio::test]
  async fn test_batch_node() {
    let child_node = Arc::new(MockChildNode);
    let batch_node = BatchNode::new("test_batch", "my_items").with_child_node(child_node);

    let mut inputs = AsyncNodeInputs::new();
    let items = vec![json!(1), json!(2), json!(3)];
    inputs.insert("my_items".to_string(), FlowValue::Json(json!(items)));

    let result = batch_node.execute(&inputs).await.unwrap();
    let results = result.get("results").unwrap();
    if let FlowValue::Json(Value::Array(arr)) = results {
      assert_eq!(arr.len(), 3);
      assert_eq!(arr[0], json!(1));
      assert_eq!(arr[1], json!(2));
      assert_eq!(arr[2], json!(3));
    }
  }
}
