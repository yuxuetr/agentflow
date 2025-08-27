use crate::{AsyncNode, SharedState, NodeError};
use agentflow_core::{AgentFlowError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinSet;

/// Batch node for parallel processing of multiple items
#[derive(Debug, Clone)]
pub struct BatchNode {
    pub name: String,
    pub items_key: String, // Key in shared state containing array of items to process
    pub batch_size: usize,
    pub max_concurrent: usize,
    pub child_node: Option<Arc<dyn AsyncNode>>, // Node to execute for each item
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

    async fn process_batch(&self, items: &[Value], shared: &SharedState) -> Result<Vec<Value>, AgentFlowError> {
        let mut results = Vec::new();
        
        if let Some(child_node) = &self.child_node {
            let mut tasks = JoinSet::new();
            
            // Process items with concurrency limit
            for (index, item) in items.iter().enumerate() {
                if tasks.len() >= self.max_concurrent {
                    // Wait for one task to complete before starting new ones
                    if let Some(task_result) = tasks.join_next().await {
                        match task_result {
                            Ok(result) => results.push(result),
                            Err(e) => {
                                return Err(AgentFlowError::AsyncExecutionError {
                                    message: format!("Batch processing task failed: {}", e),
                                });
                            }
                        }
                    }
                }

                // Set current item in shared state for child node to access
                let item_key = format!("batch_item_{}", index);
                shared.insert(item_key.clone(), item.clone());
                shared.insert("current_batch_item".to_string(), item.clone());
                shared.insert("current_batch_index".to_string(), Value::Number(serde_json::Number::from(index)));

                let child_clone = Arc::clone(child_node);
                let shared_clone = shared.clone();
                
                tasks.spawn(async move {
                    // Execute child node pipeline
                    let prep_result = child_clone.prep_async(&shared_clone).await?;
                    let exec_result = child_clone.exec_async(prep_result.clone()).await?;
                    child_clone.post_async(&shared_clone, prep_result, exec_result.clone()).await?;
                    Ok::<Value, AgentFlowError>(exec_result)
                });
            }

            // Collect remaining results
            while let Some(task_result) = tasks.join_next().await {
                match task_result {
                    Ok(result) => results.push(result),
                    Err(e) => {
                        return Err(AgentFlowError::AsyncExecutionError {
                            message: format!("Batch processing task failed: {}", e),
                        });
                    }
                }
            }
        } else {
            // No child node - just return the items as-is
            results = items.to_vec();
        }

        Ok(results)
    }
}

#[async_trait]
impl AsyncNode for BatchNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Get items array from shared state
        let items = shared.get(&self.items_key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        println!("🔧 Batch Node '{}' prepared:", self.name);
        println!("   Items key: {}", self.items_key);
        println!("   Item count: {}", items.len());
        println!("   Batch size: {}", self.batch_size);
        println!("   Max concurrent: {}", self.max_concurrent);

        Ok(serde_json::json!({
            "items": items,
            "batch_size": self.batch_size,
            "max_concurrent": self.max_concurrent
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let items = prep_result["items"].as_array()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
                message: "No items array found in prep result".to_string(),
            })?;

        let batch_size = prep_result["batch_size"].as_u64().unwrap_or(self.batch_size as u64) as usize;

        println!("🔄 Processing {} items in batches of {}", items.len(), batch_size);

        let mut all_results = Vec::new();
        let shared = SharedState::default(); // Create isolated shared state for batch processing

        // Process items in batches
        for (batch_index, batch) in items.chunks(batch_size).enumerate() {
            println!("   Processing batch {} ({} items)", batch_index + 1, batch.len());
            
            let batch_results = self.process_batch(batch, &shared).await?;
            all_results.extend(batch_results);
        }

        println!("✅ Batch processing complete. {} results", all_results.len());

        Ok(serde_json::json!({
            "results": all_results,
            "processed_count": all_results.len(),
            "batch_count": (items.len() + batch_size - 1) / batch_size
        }))
    }

    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        // Store batch results in shared state
        let output_key = format!("{}_results", self.name);
        shared.insert(output_key.clone(), exec_result["results"].clone());

        // Store metadata
        let metadata_key = format!("{}_metadata", self.name);
        shared.insert(metadata_key, serde_json::json!({
            "processed_count": exec_result["processed_count"],
            "batch_count": exec_result["batch_count"]
        }));

        println!("💾 Stored batch results in shared state as: {}", output_key);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}