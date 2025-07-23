# AgentFlow Use Cases

## Table of Contents

1. [Overview](#overview)
2. [Basic Workflow Patterns](#basic-workflow-patterns)
3. [Data Processing Pipelines](#data-processing-pipelines)
4. [API Integration Workflows](#api-integration-workflows)
5. [ML/AI Agent Workflows](#mlai-agent-workflows)
6. [Enterprise Integration Patterns](#enterprise-integration-patterns)
7. [Monitoring and Observability](#monitoring-and-observability)
8. [Advanced Robustness Patterns](#advanced-robustness-patterns)
9. [Production Deployment Scenarios](#production-deployment-scenarios)

## Overview

This document provides real-world implementation scenarios and sample code for AgentFlow, demonstrating how to build production-ready agent workflows across various domains. Each use case includes complete working examples, best practices, and performance considerations.

## Basic Workflow Patterns

### UC-1: Simple Sequential Processing

**Scenario**: Document processing pipeline with validation, transformation, and storage.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

// Document validation node
struct DocumentValidator {
    max_size_mb: u64,
    allowed_types: Vec<String>,
}

#[async_trait]
impl AsyncNode for DocumentValidator {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        // Get document metadata from shared state
        let doc_size = shared.get("document_size_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        let doc_type = shared.get("document_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("size_bytes".to_string(), Value::Number(doc_size.into()));
            map.insert("doc_type".to_string(), Value::String(doc_type));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let size_bytes = prep_result["size_bytes"].as_u64().unwrap_or(0);
        let doc_type = prep_result["doc_type"].as_str().unwrap_or("");
        
        // Validate document size
        if size_bytes > self.max_size_mb * 1024 * 1024 {
            return Err(agentflow_core::AgentFlowError::AsyncExecutionError {
                message: format!("Document too large: {} bytes", size_bytes),
            });
        }
        
        // Validate document type
        if !self.allowed_types.contains(&doc_type.to_string()) {
            return Err(agentflow_core::AgentFlowError::AsyncExecutionError {
                message: format!("Unsupported document type: {}", doc_type),
            });
        }
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("validation_status".to_string(), Value::String("passed".to_string()));
            map.insert("validated_at".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("validation_result".to_string(), exec);
        Ok(Some("transform_document".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("document_validator".to_string())
    }
}

// Document transformation node
struct DocumentTransformer {
    output_format: String,
}

#[async_trait]
impl AsyncNode for DocumentTransformer {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let content = shared.get("document_content")
            .unwrap_or(Value::String("".to_string()));
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("input_content".to_string(), content);
            map.insert("target_format".to_string(), Value::String(self.output_format.clone()));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let content = prep_result["input_content"].as_str().unwrap_or("");
        let target_format = prep_result["target_format"].as_str().unwrap_or("");
        
        // Simulate document transformation
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let transformed_content = match target_format {
            "markdown" => format!("# Transformed Document\n\n{}", content),
            "html" => format!("<html><body><h1>Transformed Document</h1><p>{}</p></body></html>", content),
            _ => content.to_string(),
        };
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("transformed_content".to_string(), Value::String(transformed_content));
            map.insert("output_format".to_string(), Value::String(target_format.to_string()));
            map.insert("transformation_timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("transformation_result".to_string(), exec);
        Ok(Some("store_document".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("document_transformer".to_string())
    }
}

// Document storage node
struct DocumentStorage {
    storage_backend: String,
}

#[async_trait]
impl AsyncNode for DocumentStorage {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let transformation_result = shared.get("transformation_result")
            .unwrap_or(Value::Null);
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("document_data".to_string(), transformation_result);
            map.insert("storage_backend".to_string(), Value::String(self.storage_backend.clone()));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let document_data = &prep_result["document_data"];
        let backend = prep_result["storage_backend"].as_str().unwrap_or("");
        
        // Simulate document storage
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let document_id = uuid::Uuid::new_v4().to_string();
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("document_id".to_string(), Value::String(document_id));
            map.insert("storage_backend".to_string(), Value::String(backend.to_string()));
            map.insert("storage_timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map.insert("storage_status".to_string(), Value::String("success".to_string()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("storage_result".to_string(), exec);
        Ok(None) // End of workflow
    }

    fn get_node_id(&self) -> Option<String> {
        Some("document_storage".to_string())
    }
}

// Usage example
#[tokio::main]
async fn main() -> Result<()> {
    // Create nodes
    let validator = Box::new(DocumentValidator {
        max_size_mb: 10,
        allowed_types: vec!["pdf".to_string(), "docx".to_string(), "txt".to_string()],
    });
    
    let transformer = Box::new(DocumentTransformer {
        output_format: "markdown".to_string(),
    });
    
    let storage = Box::new(DocumentStorage {
        storage_backend: "s3".to_string(),
    });
    
    // Create flow
    let mut flow = AsyncFlow::new(validator);
    flow.add_node("transform_document".to_string(), transformer);
    flow.add_node("store_document".to_string(), storage);
    
    // Set up shared state
    let shared = SharedState::new();
    shared.insert("document_size_bytes".to_string(), Value::Number(1048576.into())); // 1MB
    shared.insert("document_type".to_string(), Value::String("pdf".to_string()));
    shared.insert("document_content".to_string(), Value::String("Sample document content".to_string()));
    
    // Execute workflow
    match flow.run_async(&shared).await {
        Ok(result) => {
            println!("Workflow completed successfully: {:?}", result);
            
            // Check final results
            if let Some(storage_result) = shared.get("storage_result") {
                println!("Document stored: {}", storage_result);
            }
        }
        Err(e) => {
            println!("Workflow failed: {}", e);
        }
    }
    
    Ok(())
}
```

### UC-2: Parallel Data Processing

**Scenario**: Image processing pipeline with concurrent operations.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, MetricsCollector};
use std::sync::Arc;

// Image processing nodes
struct ImageResizer {
    target_width: u32,
    target_height: u32,
}

struct ImageWatermarker {
    watermark_text: String,
}

struct ImageOptimizer {
    quality: u8,
}

#[async_trait]
impl AsyncNode for ImageResizer {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let image_data = shared.get("image_data").unwrap_or(Value::Null);
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("input_image".to_string(), image_data);
            map.insert("target_width".to_string(), Value::Number(self.target_width.into()));
            map.insert("target_height".to_string(), Value::Number(self.target_height.into()));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        // Simulate image resizing
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("resized_image".to_string(), Value::String("resized_image_data".to_string()));
            map.insert("new_dimensions".to_string(), Value::String(format!("{}x{}", self.target_width, self.target_height)));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("resize_result".to_string(), exec);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some("image_resizer".to_string())
    }
}

#[async_trait]
impl AsyncNode for ImageWatermarker {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let image_data = shared.get("image_data").unwrap_or(Value::Null);
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("input_image".to_string(), image_data);
            map.insert("watermark_text".to_string(), Value::String(self.watermark_text.clone()));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        // Simulate watermarking
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("watermarked_image".to_string(), Value::String("watermarked_image_data".to_string()));
            map.insert("watermark_applied".to_string(), Value::String(self.watermark_text.clone()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("watermark_result".to_string(), exec);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some("image_watermarker".to_string())
    }
}

#[async_trait]
impl AsyncNode for ImageOptimizer {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let image_data = shared.get("image_data").unwrap_or(Value::Null);
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("input_image".to_string(), image_data);
            map.insert("quality".to_string(), Value::Number(self.quality.into()));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        // Simulate optimization
        tokio::time::sleep(Duration::from_millis(250)).await;
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("optimized_image".to_string(), Value::String("optimized_image_data".to_string()));
            map.insert("compression_ratio".to_string(), Value::Number(0.75.into()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("optimize_result".to_string(), exec);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some("image_optimizer".to_string())
    }
}

// Parallel processing example
async fn parallel_image_processing() -> Result<()> {
    // Create parallel processing nodes
    let nodes: Vec<Box<dyn AsyncNode>> = vec![
        Box::new(ImageResizer { target_width: 800, target_height: 600 }),
        Box::new(ImageWatermarker { watermark_text: "© 2024 AgentFlow".to_string() }),
        Box::new(ImageOptimizer { quality: 85 }),
    ];
    
    // Set up observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("image_processing_pipeline".to_string());
    
    // Set up shared state
    let shared = SharedState::new();
    shared.insert("image_data".to_string(), Value::String("original_image_data".to_string()));
    
    let start = std::time::Instant::now();
    
    // Execute parallel processing
    let result = flow.run_async(&shared).await?;
    
    let duration = start.elapsed();
    println!("Parallel processing completed in {:?}: {:?}", duration, result);
    
    // Check individual results
    println!("Resize result: {:?}", shared.get("resize_result"));
    println!("Watermark result: {:?}", shared.get("watermark_result"));
    println!("Optimize result: {:?}", shared.get("optimize_result"));
    
    // Check metrics
    let executions = metrics.get_metric("image_processing_pipeline.execution_count");
    let successes = metrics.get_metric("image_processing_pipeline.success_count");
    println!("Pipeline metrics - Executions: {:?}, Successes: {:?}", executions, successes);
    
    // Check individual node metrics
    println!("Resizer executions: {:?}", metrics.get_metric("image_resizer.execution_count"));
    println!("Watermarker executions: {:?}", metrics.get_metric("image_watermarker.execution_count"));
    println!("Optimizer executions: {:?}", metrics.get_metric("image_optimizer.execution_count"));
    
    Ok(())
}
```

## Data Processing Pipelines

### UC-3: ETL Pipeline with Batch Processing

**Scenario**: Extract, Transform, Load pipeline for processing large datasets in batches.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use std::collections::HashMap;

// ETL Pipeline nodes
struct DataExtractor {
    source_config: HashMap<String, String>,
}

struct DataTransformer {
    transformation_rules: Vec<String>,
}

struct DataLoader {
    destination_config: HashMap<String, String>,
    batch_size: usize,
}

#[async_trait]
impl AsyncNode for DataExtractor {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        // Get extraction parameters
        let start_date = shared.get("extraction_start_date")
            .and_then(|v| v.as_str())
            .unwrap_or("2024-01-01");
        let end_date = shared.get("extraction_end_date")
            .and_then(|v| v.as_str())
            .unwrap_or("2024-01-31");
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("start_date".to_string(), Value::String(start_date.to_string()));
            map.insert("end_date".to_string(), Value::String(end_date.to_string()));
            map.insert("source_type".to_string(), Value::String(
                self.source_config.get("type").unwrap_or(&"database".to_string()).clone()
            ));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let start_date = prep_result["start_date"].as_str().unwrap();
        let end_date = prep_result["end_date"].as_str().unwrap();
        let source_type = prep_result["source_type"].as_str().unwrap();
        
        // Simulate data extraction
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Generate sample data
        let mut records = Vec::new();
        for i in 1..=1000 {
            records.push(serde_json::json!({
                "id": i,
                "timestamp": format!("2024-01-{:02}T12:00:00Z", (i % 31) + 1),
                "value": i as f64 * 1.5,
                "category": match i % 3 {
                    0 => "A",
                    1 => "B",
                    _ => "C",
                },
                "status": "active"
            }));
        }
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("extracted_records".to_string(), Value::Array(records));
            map.insert("record_count".to_string(), Value::Number(1000.into()));
            map.insert("extraction_timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("extraction_result".to_string(), exec);
        Ok(Some("transform_data".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("data_extractor".to_string())
    }
}

#[async_trait]
impl AsyncNode for DataTransformer {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let extraction_result = shared.get("extraction_result")
            .unwrap_or(Value::Null);
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("input_data".to_string(), extraction_result);
            map.insert("transformation_rules".to_string(), Value::Array(
                self.transformation_rules.iter().map(|r| Value::String(r.clone())).collect()
            ));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let input_data = &prep_result["input_data"];
        let records = input_data["extracted_records"].as_array().unwrap();
        
        // Apply transformations
        let mut transformed_records = Vec::new();
        for record in records {
            let mut transformed = record.clone();
            
            // Apply transformation rules
            for rule in &self.transformation_rules {
                match rule.as_str() {
                    "normalize_values" => {
                        if let Some(value) = transformed["value"].as_f64() {
                            transformed["normalized_value"] = Value::Number(
                                serde_json::Number::from_f64(value / 100.0).unwrap()
                            );
                        }
                    }
                    "add_computed_fields" => {
                        let category = transformed["category"].as_str().unwrap_or("");
                        transformed["category_score"] = Value::Number(match category {
                            "A" => 10.into(),
                            "B" => 20.into(),
                            "C" => 30.into(),
                            _ => 0.into(),
                        });
                    }
                    "filter_active" => {
                        if transformed["status"].as_str() != Some("active") {
                            continue; // Skip inactive records
                        }
                    }
                    _ => {} // Unknown rule, skip
                }
            }
            
            transformed_records.push(transformed);
        }
        
        // Simulate processing time
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("transformed_records".to_string(), Value::Array(transformed_records.clone()));
            map.insert("processed_count".to_string(), Value::Number(transformed_records.len().into()));
            map.insert("transformation_timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("transformation_result".to_string(), exec);
        Ok(Some("load_data".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("data_transformer".to_string())
    }
}

#[async_trait]
impl AsyncNode for DataLoader {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let transformation_result = shared.get("transformation_result")
            .unwrap_or(Value::Null);
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("data_to_load".to_string(), transformation_result);
            map.insert("batch_size".to_string(), Value::Number(self.batch_size.into()));
            map.insert("destination".to_string(), Value::String(
                self.destination_config.get("type").unwrap_or(&"warehouse".to_string()).clone()
            ));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let data_to_load = &prep_result["data_to_load"];
        let records = data_to_load["transformed_records"].as_array().unwrap();
        let batch_size = prep_result["batch_size"].as_u64().unwrap() as usize;
        
        // Process records in batches
        let mut batch_results = Vec::new();
        let total_records = records.len();
        
        for (batch_idx, batch) in records.chunks(batch_size).enumerate() {
            // Simulate batch loading
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let batch_result = serde_json::json!({
                "batch_index": batch_idx,
                "records_loaded": batch.len(),
                "batch_timestamp": chrono::Utc::now().to_rfc3339(),
                "status": "success"
            });
            
            batch_results.push(batch_result);
        }
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("batch_results".to_string(), Value::Array(batch_results));
            map.insert("total_records_loaded".to_string(), Value::Number(total_records.into()));
            map.insert("total_batches".to_string(), Value::Number(((total_records + batch_size - 1) / batch_size).into()));
            map.insert("load_timestamp".to_string(), Value::String(chrono::Utc::now().to_rfc3339()));
            map
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("load_result".to_string(), exec);
        Ok(None) // End of ETL pipeline
    }

    fn get_node_id(&self) -> Option<String> {
        Some("data_loader".to_string())
    }
}

// ETL Pipeline usage
async fn run_etl_pipeline() -> Result<()> {
    // Create ETL nodes
    let extractor = Box::new(DataExtractor {
        source_config: {
            let mut config = HashMap::new();
            config.insert("type".to_string(), "postgresql".to_string());
            config.insert("host".to_string(), "localhost".to_string());
            config.insert("database".to_string(), "source_db".to_string());
            config
        },
    });
    
    let transformer = Box::new(DataTransformer {
        transformation_rules: vec![
            "normalize_values".to_string(),
            "add_computed_fields".to_string(),
            "filter_active".to_string(),
        ],
    });
    
    let loader = Box::new(DataLoader {
        destination_config: {
            let mut config = HashMap::new();
            config.insert("type".to_string(), "data_warehouse".to_string());
            config.insert("connection".to_string(), "redshift://cluster/db".to_string());
            config
        },
        batch_size: 100,
    });
    
    // Create ETL flow with observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new(extractor);
    flow.add_node("transform_data".to_string(), transformer);
    flow.add_node("load_data".to_string(), loader);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("etl_pipeline".to_string());
    
    // Set up extraction parameters
    let shared = SharedState::new();
    shared.insert("extraction_start_date".to_string(), Value::String("2024-01-01".to_string()));
    shared.insert("extraction_end_date".to_string(), Value::String("2024-01-31".to_string()));
    
    let start = std::time::Instant::now();
    
    // Execute ETL pipeline
    match flow.run_async(&shared).await {
        Ok(result) => {
            let duration = start.elapsed();
            println!("ETL pipeline completed in {:?}: {:?}", duration, result);
            
            // Print pipeline results
            if let Some(load_result) = shared.get("load_result") {
                let total_records = load_result["total_records_loaded"].as_u64().unwrap_or(0);
                let total_batches = load_result["total_batches"].as_u64().unwrap_or(0);
                println!("Successfully loaded {} records in {} batches", total_records, total_batches);
            }
            
            // Print metrics
            let pipeline_executions = metrics.get_metric("etl_pipeline.execution_count");
            let pipeline_duration = metrics.get_metric("etl_pipeline.duration_ms");
            println!("Pipeline metrics - Executions: {:?}, Duration: {:?}ms", pipeline_executions, pipeline_duration);
        }
        Err(e) => {
            println!("ETL pipeline failed: {}", e);
        }
    }
    
    Ok(())
}
```

## API Integration Workflows

### UC-4: Robust API Integration with Circuit Breaker

**Scenario**: Third-party API integration with comprehensive error handling and resilience patterns.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, CircuitBreaker, RateLimiter, TimeoutManager};
use std::time::Duration;

// API client node with robustness patterns
struct RobustApiClient {
    api_endpoint: String,
    circuit_breaker: CircuitBreaker,
    rate_limiter: RateLimiter,
    timeout_manager: TimeoutManager,
}

impl RobustApiClient {
    fn new(api_endpoint: String) -> Self {
        Self {
            api_endpoint,
            circuit_breaker: CircuitBreaker::new(
                "api_calls".to_string(),
                3, // failure threshold
                Duration::from_secs(30), // recovery timeout
            ),
            rate_limiter: RateLimiter::new(
                "api_rate_limit".to_string(),
                10, // 10 requests
                Duration::from_secs(60), // per minute
            ),
            timeout_manager: TimeoutManager::new(
                "api_timeouts".to_string(),
                Duration::from_secs(30), // default timeout
            ),
        }
    }
    
    async fn make_api_call(&self, request_data: &Value) -> Result<Value> {
        // Check rate limit first
        self.rate_limiter.acquire().await?;
        
        // Use circuit breaker for the API call
        self.circuit_breaker.call(async {
            // Use timeout manager for the actual HTTP call
            self.timeout_manager.execute_with_timeout("http_request", async {
                // Simulate HTTP API call
                tokio::time::sleep(Duration::from_millis(200)).await;
                
                // Simulate occasional failures for demonstration
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                std::time::SystemTime::now().hash(&mut hasher);
                let random_val = (hasher.finish() % 100) as f64 / 100.0;
                
                if random_val < 0.1 { // 10% failure rate
                    return Err(agentflow_core::AgentFlowError::AsyncExecutionError {
                        message: "API call failed".to_string(),
                    });
                }
                
                Ok(serde_json::json!({
                    "api_response": "success",
                    "data": request_data,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "response_time_ms": 200
                }))
            }).await
        }).await
    }
}

#[async_trait]
impl AsyncNode for RobustApiClient {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let request_payload = shared.get("api_request_payload")
            .unwrap_or(Value::Null);
        
        Ok(Value::Object({
            let mut map = serde_json::Map::new();
            map.insert("endpoint".to_string(), Value::String(self.api_endpoint.clone()));
            map.insert("payload".to_string(), request_payload);
            map.insert("circuit_breaker_state".to_string(), Value::String(format!("{:?}", self.circuit_breaker.get_state())));
            map
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let payload = &prep_result["payload"];
        
        // Make robust API call
        match self.make_api_call(payload).await {
            Ok(response) => Ok(response),
            Err(e) => {
                // Log error and potentially return fallback data
                println!("API call failed: {}. Circuit breaker state: {:?}", e, self.circuit_breaker.get_state());
                
                // Return fallback response
                Ok(serde_json::json!({
                    "api_response": "fallback",
                    "error": e.to_string(),
                    "fallback_data": "cached_or_default_response",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }))
            }
        }
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("api_response".to_string(), exec);
        
        // Route based on response type
        let response_type = shared.get("api_response")
            .and_then(|r| r.get("api_response"))
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        
        match response_type {
            "success" => Ok(Some("process_success_response".to_string())),
            "fallback" => Ok(Some("handle_fallback_response".to_string())),
            _ => Ok(None),
        }
    }

    fn get_node_id(&self) -> Option<String> {
        Some("robust_api_client".to_string())
    }
}

// Response processing nodes
struct SuccessResponseProcessor;
struct FallbackResponseHandler;

#[async_trait]
impl AsyncNode for SuccessResponseProcessor {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let api_response = shared.get("api_response").unwrap_or(Value::Null);
        Ok(api_response)
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let response_data = prep_result["data"].clone();
        let response_time = prep_result["response_time_ms"].as_u64().unwrap_or(0);
        
        // Process successful response
        Ok(serde_json::json!({
            "processed_data": response_data,
            "processing_status": "success",
            "api_performance": {
                "response_time_ms": response_time,
                "performance_tier": if response_time < 100 { "fast" } else if response_time < 500 { "normal" } else { "slow" }
            },
            "processed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("final_result".to_string(), exec);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some("success_response_processor".to_string())
    }
}

#[async_trait]
impl AsyncNode for FallbackResponseHandler {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let api_response = shared.get("api_response").unwrap_or(Value::Null);
        Ok(api_response)
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let fallback_data = prep_result["fallback_data"].as_str().unwrap_or("");
        let error_info = prep_result["error"].as_str().unwrap_or("");
        
        // Handle fallback scenario
        Ok(serde_json::json!({
            "fallback_result": fallback_data,
            "error_handled": true,
            "error_details": error_info,
            "fallback_strategy": "cached_response",
            "handled_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("final_result".to_string(), exec);
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        Some("fallback_response_handler".to_string())
    }
}

// Usage example with robustness patterns
async fn robust_api_integration_workflow() -> Result<()> {
    // Create robust API client
    let api_client = Box::new(RobustApiClient::new(
        "https://api.example.com/v1/data".to_string()
    ));
    
    let success_processor = Box::new(SuccessResponseProcessor);
    let fallback_handler = Box::new(FallbackResponseHandler);
    
    // Create flow with observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new(api_client);
    flow.add_node("process_success_response".to_string(), success_processor);
    flow.add_node("handle_fallback_response".to_string(), fallback_handler);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("robust_api_integration".to_string());
    
    // Set up request data
    let shared = SharedState::new();
    shared.insert("api_request_payload".to_string(), serde_json::json!({
        "user_id": "12345",
        "action": "get_user_data",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    
    // Execute multiple times to demonstrate robustness
    for i in 1..=5 {
        println!("\n--- API Call #{} ---", i);
        
        let start = std::time::Instant::now();
        match flow.run_async(&shared).await {
            Ok(result) => {
                let duration = start.elapsed();
                println!("API workflow completed in {:?}: {:?}", duration, result);
                
                if let Some(final_result) = shared.get("final_result") {
                    println!("Final result: {}", serde_json::to_string_pretty(&final_result).unwrap());
                }
            }
            Err(e) => {
                println!("API workflow failed: {}", e);
            }
        }
        
        // Brief pause between calls
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    // Print final metrics
    println!("\n--- Final Metrics ---");
    let total_executions = metrics.get_metric("robust_api_integration.execution_count").unwrap_or(0.0);
    let total_successes = metrics.get_metric("robust_api_integration.success_count").unwrap_or(0.0);
    let total_errors = metrics.get_metric("robust_api_integration.error_count").unwrap_or(0.0);
    let avg_duration = metrics.get_metric("robust_api_integration.duration_ms").unwrap_or(0.0) / total_executions.max(1.0);
    
    println!("Total executions: {}", total_executions);
    println!("Success rate: {:.1}%", (total_successes / total_executions.max(1.0)) * 100.0);
    println!("Error rate: {:.1}%", (total_errors / total_executions.max(1.0)) * 100.0);
    println!("Average duration: {:.1}ms", avg_duration);
    
    Ok(())
}
```

## ML/AI Agent Workflows

### UC-5: AI Agent Pipeline with Model Chaining

**Scenario**: Multi-stage AI processing with model orchestration and result aggregation.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};

// AI Model nodes
struct TextAnalysisModel {
    model_name: String,
    confidence_threshold: f64,
}

struct SentimentAnalysisModel {
    model_version: String,
}

struct EntityExtractionModel {
    entity_types: Vec<String>,
}

struct ResultAggregator {
    aggregation_strategy: String,
}

#[async_trait]
impl AsyncNode for TextAnalysisModel {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let input_text = shared.get("input_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        Ok(serde_json::json!({
            "model_name": self.model_name,
            "input_text": input_text,
            "text_length": input_text.len(),
            "confidence_threshold": self.confidence_threshold
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let input_text = prep_result["input_text"].as_str().unwrap_or("");
        
        // Simulate text analysis model inference
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        // Generate mock analysis results
        let word_count = input_text.split_whitespace().count();
        let complexity_score = (word_count as f64 * 0.1).min(1.0);
        let readability_score = 1.0 - complexity_score;
        
        Ok(serde_json::json!({
            "model_output": {
                "word_count": word_count,
                "complexity_score": complexity_score,
                "readability_score": readability_score,
                "language_detected": "en",
                "confidence": 0.95
            },
            "model_metadata": {
                "model_name": self.model_name,
                "inference_time_ms": 300,
                "model_version": "1.0.0"
            },
            "processed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("text_analysis_result".to_string(), exec);
        Ok(Some("sentiment_analysis".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some(format!("text_analysis_{}", self.model_name))
    }
}

#[async_trait]
impl AsyncNode for SentimentAnalysisModel {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let input_text = shared.get("input_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text_analysis = shared.get("text_analysis_result")
            .unwrap_or(Value::Null);
        
        Ok(serde_json::json!({
            "model_version": self.model_version,
            "input_text": input_text,
            "prior_analysis": text_analysis
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let input_text = prep_result["input_text"].as_str().unwrap_or("");
        
        // Simulate sentiment analysis
        tokio::time::sleep(Duration::from_millis(250)).await;
        
        // Simple sentiment scoring based on text characteristics
        let positive_words = ["good", "great", "excellent", "amazing", "wonderful", "fantastic"];
        let negative_words = ["bad", "terrible", "awful", "horrible", "disappointing", "poor"];
        
        let text_lower = input_text.to_lowercase();
        let positive_count = positive_words.iter()
            .map(|word| text_lower.matches(word).count())
            .sum::<usize>();
        let negative_count = negative_words.iter()
            .map(|word| text_lower.matches(word).count())
            .sum::<usize>();
        
        let sentiment_score = if positive_count > negative_count {
            0.7 + (positive_count as f64 * 0.1).min(0.3)
        } else if negative_count > positive_count {
            0.3 - (negative_count as f64 * 0.1).max(0.3)
        } else {
            0.5
        };
        
        let sentiment_label = match sentiment_score {
            s if s > 0.6 => "positive",
            s if s < 0.4 => "negative",
            _ => "neutral",
        };
        
        Ok(serde_json::json!({
            "model_output": {
                "sentiment_score": sentiment_score,
                "sentiment_label": sentiment_label,
                "confidence": 0.87,
                "positive_indicators": positive_count,
                "negative_indicators": negative_count
            },
            "model_metadata": {
                "model_version": self.model_version,
                "inference_time_ms": 250,
                "algorithm": "transformer_based"
            },
            "processed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("sentiment_analysis_result".to_string(), exec);
        Ok(Some("entity_extraction".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("sentiment_analysis_model".to_string())
    }
}

#[async_trait]
impl AsyncNode for EntityExtractionModel {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let input_text = shared.get("input_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        Ok(serde_json::json!({
            "input_text": input_text,
            "entity_types": self.entity_types,
            "extraction_mode": "comprehensive"
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let input_text = prep_result["input_text"].as_str().unwrap_or("");
        
        // Simulate entity extraction
        tokio::time::sleep(Duration::from_millis(400)).await;
        
        // Simple pattern-based entity extraction (mock)
        let mut entities = Vec::new();
        
        // Extract email-like patterns
        if input_text.contains("@") && input_text.contains(".") {
            entities.push(serde_json::json!({
                "type": "EMAIL",
                "value": "example@domain.com",
                "start": 0,
                "end": 19,
                "confidence": 0.92
            }));
        }
        
        // Extract number patterns
        let numbers: Vec<&str> = input_text.split_whitespace()
            .filter(|word| word.chars().all(|c| c.is_numeric()))
            .collect();
        for number in numbers {
            entities.push(serde_json::json!({
                "type": "NUMBER",
                "value": number,
                "start": 0,
                "end": number.len(),
                "confidence": 0.98
            }));
        }
        
        // Extract capitalized words as potential proper nouns
        let proper_nouns: Vec<&str> = input_text.split_whitespace()
            .filter(|word| word.chars().next().map_or(false, |c| c.is_uppercase()) && word.len() > 2)
            .collect();
        for noun in proper_nouns {
            entities.push(serde_json::json!({
                "type": "PERSON_OR_ORG",
                "value": noun,
                "start": 0,
                "end": noun.len(),
                "confidence": 0.75
            }));
        }
        
        Ok(serde_json::json!({
            "model_output": {
                "entities": entities,
                "entity_count": entities.len(),
                "coverage_score": (entities.len() as f64 / input_text.split_whitespace().count().max(1) as f64).min(1.0)
            },
            "model_metadata": {
                "supported_types": self.entity_types,
                "inference_time_ms": 400,
                "extraction_algorithm": "hybrid_pattern_ml"
            },
            "processed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("entity_extraction_result".to_string(), exec);
        Ok(Some("aggregate_results".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("entity_extraction_model".to_string())
    }
}

#[async_trait]
impl AsyncNode for ResultAggregator {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let text_analysis = shared.get("text_analysis_result").unwrap_or(Value::Null);
        let sentiment_analysis = shared.get("sentiment_analysis_result").unwrap_or(Value::Null);
        let entity_extraction = shared.get("entity_extraction_result").unwrap_or(Value::Null);
        
        Ok(serde_json::json!({
            "text_analysis": text_analysis,
            "sentiment_analysis": sentiment_analysis,
            "entity_extraction": entity_extraction,
            "aggregation_strategy": self.aggregation_strategy
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let text_analysis = &prep_result["text_analysis"];
        let sentiment_analysis = &prep_result["sentiment_analysis"];
        let entity_extraction = &prep_result["entity_extraction"];
        
        // Aggregate results from all models
        let overall_confidence = [
            text_analysis["model_output"]["confidence"].as_f64().unwrap_or(0.0),
            sentiment_analysis["model_output"]["confidence"].as_f64().unwrap_or(0.0),
            entity_extraction["model_output"]["coverage_score"].as_f64().unwrap_or(0.0),
        ].iter().sum::<f64>() / 3.0;
        
        let total_processing_time = [
            text_analysis["model_metadata"]["inference_time_ms"].as_u64().unwrap_or(0),
            sentiment_analysis["model_metadata"]["inference_time_ms"].as_u64().unwrap_or(0),
            entity_extraction["model_metadata"]["inference_time_ms"].as_u64().unwrap_or(0),
        ].iter().sum::<u64>();
        
        // Create comprehensive analysis summary
        Ok(serde_json::json!({
            "comprehensive_analysis": {
                "text_metrics": {
                    "word_count": text_analysis["model_output"]["word_count"],
                    "complexity_score": text_analysis["model_output"]["complexity_score"],
                    "readability_score": text_analysis["model_output"]["readability_score"]
                },
                "sentiment": {
                    "score": sentiment_analysis["model_output"]["sentiment_score"],
                    "label": sentiment_analysis["model_output"]["sentiment_label"],
                    "confidence": sentiment_analysis["model_output"]["confidence"]
                },
                "entities": {
                    "count": entity_extraction["model_output"]["entity_count"],
                    "types_found": entity_extraction["model_output"]["entities"],
                    "coverage": entity_extraction["model_output"]["coverage_score"]
                },
                "overall_insights": {
                    "processing_quality": if overall_confidence > 0.8 { "high" } else if overall_confidence > 0.6 { "medium" } else { "low" },
                    "confidence_score": overall_confidence,
                    "processing_complete": true
                }
            },
            "pipeline_metadata": {
                "total_processing_time_ms": total_processing_time,
                "models_used": 3,
                "pipeline_version": "2.1.0",
                "aggregation_strategy": self.aggregation_strategy
            },
            "completed_at": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("final_analysis".to_string(), exec);
        Ok(None) // End of AI pipeline
    }

    fn get_node_id(&self) -> Option<String> {
        Some("result_aggregator".to_string())
    }
}

// AI Pipeline usage
async fn ai_agent_pipeline() -> Result<()> {
    // Create AI model nodes
    let text_analyzer = Box::new(TextAnalysisModel {
        model_name: "bert_base_uncased".to_string(),
        confidence_threshold: 0.8,
    });
    
    let sentiment_analyzer = Box::new(SentimentAnalysisModel {
        model_version: "2.1.0".to_string(),
    });
    
    let entity_extractor = Box::new(EntityExtractionModel {
        entity_types: vec![
            "PERSON".to_string(),
            "ORGANIZATION".to_string(),
            "EMAIL".to_string(),
            "NUMBER".to_string(),
        ],
    });
    
    let aggregator = Box::new(ResultAggregator {
        aggregation_strategy: "weighted_confidence".to_string(),
    });
    
    // Create AI pipeline with observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut flow = AsyncFlow::new(text_analyzer);
    flow.add_node("sentiment_analysis".to_string(), sentiment_analyzer);
    flow.add_node("entity_extraction".to_string(), entity_extractor);
    flow.add_node("aggregate_results".to_string(), aggregator);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("ai_analysis_pipeline".to_string());
    
    // Test with different text samples
    let test_texts = vec![
        "Hello John! This is a great product with excellent features. Contact us at support@company.com for more information. Our phone number is 555-0123.",
        "This service is terrible and disappointing. I would not recommend it to anyone. The quality is poor and the support is awful.",
        "The weather today is nice. I will go for a walk in the park. There are many trees and flowers blooming.",
    ];
    
    for (i, text) in test_texts.iter().enumerate() {
        println!("\n--- AI Analysis #{} ---", i + 1);
        println!("Input text: {}", text);
        
        // Set up input text
        let shared = SharedState::new();
        shared.insert("input_text".to_string(), Value::String(text.to_string()));
        
        let start = std::time::Instant::now();
        
        match flow.run_async(&shared).await {
            Ok(result) => {
                let duration = start.elapsed();
                println!("AI pipeline completed in {:?}", duration);
                
                if let Some(final_analysis) = shared.get("final_analysis") {
                    println!("Analysis results:");
                    println!("{}", serde_json::to_string_pretty(&final_analysis).unwrap());
                }
            }
            Err(e) => {
                println!("AI pipeline failed: {}", e);
            }
        }
    }
    
    // Print pipeline performance metrics
    println!("\n--- Pipeline Performance Metrics ---");
    let total_executions = metrics.get_metric("ai_analysis_pipeline.execution_count").unwrap_or(0.0);
    let avg_duration = metrics.get_metric("ai_analysis_pipeline.duration_ms").unwrap_or(0.0) / total_executions.max(1.0);
    let success_rate = metrics.get_metric("ai_analysis_pipeline.success_count").unwrap_or(0.0) / total_executions.max(1.0) * 100.0;
    
    println!("Total pipeline executions: {}", total_executions);
    println!("Average processing time: {:.1}ms", avg_duration);
    println!("Success rate: {:.1}%", success_rate);
    
    // Individual model performance
    println!("\nIndividual model performance:");
    for model in ["text_analysis_bert_base_uncased", "sentiment_analysis_model", "entity_extraction_model", "result_aggregator"] {
        let executions = metrics.get_metric(&format!("{}.execution_count", model)).unwrap_or(0.0);
        let duration = metrics.get_metric(&format!("{}.duration_ms", model)).unwrap_or(0.0);
        if executions > 0.0 {
            println!("  {}: {:.1}ms avg", model, duration / executions);
        }
    }
    
    Ok(())
}
```

## Enterprise Integration Patterns

### UC-6: Enterprise Service Bus Integration

**Scenario**: Integration with enterprise systems using message queues, event sourcing, and distributed workflows.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result, MetricsCollector, AlertManager, AlertRule};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// Enterprise integration components
struct MessageQueueConsumer {
    queue_name: String,
    message_handler: String,
}

struct EventProcessor {
    event_type: String,
    processing_rules: Vec<String>,
}

struct ServiceOrchestrator {
    services: HashMap<String, String>,
    retry_config: RetryConfig,
}

struct AuditLogger {
    audit_rules: Vec<String>,
}

#[derive(Clone)]
struct RetryConfig {
    max_retries: u32,
    base_delay_ms: u64,
    backoff_multiplier: f64,
}

// Message queue consumer implementation
#[async_trait]
impl AsyncNode for MessageQueueConsumer {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        // Simulate message consumption from queue
        let message_batch = serde_json::json!({
            "batch_id": uuid::Uuid::new_v4().to_string(),
            "queue_name": self.queue_name,
            "messages": [
                {
                    "message_id": "msg_001",
                    "event_type": "order_created",
                    "payload": {
                        "order_id": "ORD-2024-001",
                        "customer_id": "CUST-12345",
                        "total_amount": 299.99,
                        "items": [
                            {"product_id": "PROD-001", "quantity": 2, "price": 149.99}
                        ]
                    },
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "correlation_id": "corr_001"
                },
                {
                    "message_id": "msg_002", 
                    "event_type": "payment_processed",
                    "payload": {
                        "payment_id": "PAY-2024-001",
                        "order_id": "ORD-2024-001",
                        "amount": 299.99,
                        "status": "success"
                    },
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "correlation_id": "corr_001"
                }
            ]
        });
        
        Ok(message_batch)
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let messages = prep_result["messages"].as_array().unwrap();
        let mut processed_messages = Vec::new();
        
        for message in messages {
            // Simulate message processing
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            let event_type = message["event_type"].as_str().unwrap_or("");
            let processing_result = match event_type {
                "order_created" => "route_to_fulfillment",
                "payment_processed" => "update_order_status", 
                _ => "unknown_event_handler",
            };
            
            processed_messages.push(serde_json::json!({
                "message_id": message["message_id"],
                "event_type": event_type,
                "processing_result": processing_result,
                "processed_at": chrono::Utc::now().to_rfc3339(),
                "correlation_id": message["correlation_id"]
            }));
        }
        
        Ok(serde_json::json!({
            "batch_id": prep_result["batch_id"],
            "processed_messages": processed_messages,
            "processing_summary": {
                "total_messages": messages.len(),
                "successful_processing": processed_messages.len(),
                "processing_time_ms": messages.len() * 50
            }
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("message_processing_result".to_string(), exec);
        Ok(Some("process_events".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some(format!("mq_consumer_{}", self.queue_name))
    }
}

#[async_trait]
impl AsyncNode for EventProcessor {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let processing_result = shared.get("message_processing_result").unwrap_or(Value::Null);
        
        Ok(serde_json::json!({
            "event_type_filter": self.event_type,
            "processing_rules": self.processing_rules,
            "input_data": processing_result
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let processed_messages = prep_result["input_data"]["processed_messages"].as_array().unwrap();
        let mut event_processing_results = Vec::new();
        
        for message in processed_messages {
            let event_type = message["event_type"].as_str().unwrap_or("");
            
            // Apply processing rules based on event type
            if event_type == self.event_type || self.event_type == "all" {
                let mut processing_steps = Vec::new();
                
                for rule in &self.processing_rules {
                    match rule.as_str() {
                        "validate_schema" => {
                            processing_steps.push(serde_json::json!({
                                "step": "schema_validation",
                                "status": "passed",
                                "details": "Message schema is valid"
                            }));
                        }
                        "enrich_data" => {
                            processing_steps.push(serde_json::json!({
                                "step": "data_enrichment", 
                                "status": "completed",
                                "details": "Added customer metadata and product details"
                            }));
                        }
                        "apply_business_rules" => {
                            processing_steps.push(serde_json::json!({
                                "step": "business_rules",
                                "status": "applied",
                                "details": "Discount rules and tax calculations applied"
                            }));
                        }
                        _ => {}
                    }
                }
                
                event_processing_results.push(serde_json::json!({
                    "message_id": message["message_id"],
                    "event_type": event_type,
                    "processing_steps": processing_steps,
                    "correlation_id": message["correlation_id"],
                    "processed_at": chrono::Utc::now().to_rfc3339()
                }));
            }
        }
        
        // Simulate processing time
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Ok(serde_json::json!({
            "event_processing_results": event_processing_results,
            "processing_summary": {
                "events_processed": event_processing_results.len(),
                "event_type_filter": self.event_type,
                "rules_applied": self.processing_rules.len()
            }
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("event_processing_result".to_string(), exec);
        Ok(Some("orchestrate_services".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some(format!("event_processor_{}", self.event_type))
    }
}

#[async_trait]
impl AsyncNode for ServiceOrchestrator {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let event_results = shared.get("event_processing_result").unwrap_or(Value::Null);
        
        Ok(serde_json::json!({
            "services_config": self.services,
            "retry_config": {
                "max_retries": self.retry_config.max_retries,
                "base_delay_ms": self.retry_config.base_delay_ms,
                "backoff_multiplier": self.retry_config.backoff_multiplier
            },
            "event_data": event_results
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let event_results = prep_result["event_data"]["event_processing_results"].as_array().unwrap();
        let mut service_orchestration_results = Vec::new();
        
        for event in event_results {
            let event_type = event["event_type"].as_str().unwrap_or("");
            let correlation_id = event["correlation_id"].as_str().unwrap_or("");
            
            // Determine which services to call based on event type
            let services_to_call = match event_type {
                "order_created" => vec!["inventory_service", "customer_service", "notification_service"],
                "payment_processed" => vec!["order_service", "accounting_service", "notification_service"],
                _ => vec!["audit_service"],
            };
            
            let mut service_call_results = Vec::new();
            
            for service_name in services_to_call {
                // Simulate service call with retry logic
                let mut attempt = 0;
                let mut call_successful = false;
                
                while attempt < self.retry_config.max_retries && !call_successful {
                    attempt += 1;
                    
                    // Simulate service call
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    
                    // Simulate occasional failures (10% failure rate)
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    (service_name, attempt, std::time::SystemTime::now()).hash(&mut hasher);
                    let random_val = (hasher.finish() % 100) as f64 / 100.0;
                    
                    if random_val < 0.9 { // 90% success rate
                        call_successful = true;
                        service_call_results.push(serde_json::json!({
                            "service_name": service_name,
                            "status": "success",
                            "attempt": attempt,
                            "response_time_ms": 100,
                            "response": format!("Service {} processed event successfully", service_name)
                        }));
                    } else if attempt < self.retry_config.max_retries {
                        // Wait before retry with exponential backoff
                        let delay = Duration::from_millis(
                            (self.retry_config.base_delay_ms as f64 * 
                             self.retry_config.backoff_multiplier.powi(attempt as i32 - 1)) as u64
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
                
                if !call_successful {
                    service_call_results.push(serde_json::json!({
                        "service_name": service_name,
                        "status": "failed",
                        "attempt": attempt,
                        "error": "Service call failed after retries"
                    }));
                }
            }
            
            service_orchestration_results.push(serde_json::json!({
                "event_id": event["message_id"],
                "event_type": event_type,
                "correlation_id": correlation_id,
                "service_calls": service_call_results,
                "orchestration_status": if service_call_results.iter().all(|r| r["status"] == "success") { "completed" } else { "partial_failure" }
            }));
        }
        
        Ok(serde_json::json!({
            "orchestration_results": service_orchestration_results,
            "orchestration_summary": {
                "total_events": event_results.len(),
                "successful_orchestrations": service_orchestration_results.iter()
                    .filter(|r| r["orchestration_status"] == "completed").count(),
                "partial_failures": service_orchestration_results.iter()
                    .filter(|r| r["orchestration_status"] == "partial_failure").count()
            }
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("orchestration_result".to_string(), exec);
        Ok(Some("audit_logging".to_string()))
    }

    fn get_node_id(&self) -> Option<String> {
        Some("service_orchestrator".to_string())
    }
}

#[async_trait]
impl AsyncNode for AuditLogger {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        let orchestration_result = shared.get("orchestration_result").unwrap_or(Value::Null);
        let message_result = shared.get("message_processing_result").unwrap_or(Value::Null);
        let event_result = shared.get("event_processing_result").unwrap_or(Value::Null);
        
        Ok(serde_json::json!({
            "audit_rules": self.audit_rules,
            "pipeline_data": {
                "message_processing": message_result,
                "event_processing": event_result,
                "service_orchestration": orchestration_result
            }
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let pipeline_data = &prep_result["pipeline_data"];
        let audit_rules = &self.audit_rules;
        
        let mut audit_entries = Vec::new();
        
        // Generate audit entries based on rules
        for rule in audit_rules {
            match rule.as_str() {
                "log_all_events" => {
                    if let Some(events) = pipeline_data["event_processing"]["event_processing_results"].as_array() {
                        for event in events {
                            audit_entries.push(serde_json::json!({
                                "audit_type": "event_processing",
                                "correlation_id": event["correlation_id"],
                                "event_type": event["event_type"],
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                "details": "Event processed successfully"
                            }));
                        }
                    }
                }
                "log_service_calls" => {
                    if let Some(orchestrations) = pipeline_data["service_orchestration"]["orchestration_results"].as_array() {
                        for orchestration in orchestrations {
                            if let Some(service_calls) = orchestration["service_calls"].as_array() {
                                for service_call in service_calls {
                                    audit_entries.push(serde_json::json!({
                                        "audit_type": "service_call",
                                        "correlation_id": orchestration["correlation_id"],
                                        "service_name": service_call["service_name"],
                                        "status": service_call["status"],
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                        "details": format!("Service call to {} {}", 
                                                         service_call["service_name"], 
                                                         service_call["status"])
                                    }));
                                }
                            }
                        }
                    }
                }
                "log_failures" => {
                    // Log any failures in the pipeline
                    if let Some(orchestrations) = pipeline_data["service_orchestration"]["orchestration_results"].as_array() {
                        for orchestration in orchestrations {
                            if orchestration["orchestration_status"] == "partial_failure" {
                                audit_entries.push(serde_json::json!({
                                    "audit_type": "failure",
                                    "correlation_id": orchestration["correlation_id"],
                                    "event_type": orchestration["event_type"],
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                    "details": "Service orchestration experienced partial failure",
                                    "severity": "warning"
                                }));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        
        // Simulate audit logging
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        Ok(serde_json::json!({
            "audit_entries": audit_entries,
            "audit_summary": {
                "total_entries": audit_entries.len(),
                "audit_timestamp": chrono::Utc::now().to_rfc3339(),
                "compliance_status": "compliant"
            }
        }))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("audit_result".to_string(), exec);
        Ok(None) // End of enterprise integration pipeline
    }

    fn get_node_id(&self) -> Option<String> {
        Some("audit_logger".to_string())
    }
}

// Enterprise integration workflow
async fn enterprise_integration_workflow() -> Result<()> {
    // Create enterprise integration components
    let mq_consumer = Box::new(MessageQueueConsumer {
        queue_name: "enterprise_events".to_string(),
        message_handler: "default_handler".to_string(),
    });
    
    let event_processor = Box::new(EventProcessor {
        event_type: "all".to_string(),
        processing_rules: vec![
            "validate_schema".to_string(),
            "enrich_data".to_string(), 
            "apply_business_rules".to_string(),
        ],
    });
    
    let service_orchestrator = Box::new(ServiceOrchestrator {
        services: {
            let mut services = HashMap::new();
            services.insert("inventory_service".to_string(), "http://inventory.internal:8080".to_string());
            services.insert("customer_service".to_string(), "http://customer.internal:8080".to_string());
            services.insert("notification_service".to_string(), "http://notification.internal:8080".to_string());
            services.insert("order_service".to_string(), "http://order.internal:8080".to_string());
            services.insert("accounting_service".to_string(), "http://accounting.internal:8080".to_string());
            services.insert("audit_service".to_string(), "http://audit.internal:8080".to_string());
            services
        },
        retry_config: RetryConfig {
            max_retries: 3,
            base_delay_ms: 100,
            backoff_multiplier: 2.0,
        },
    });
    
    let audit_logger = Box::new(AuditLogger {
        audit_rules: vec![
            "log_all_events".to_string(),
            "log_service_calls".to_string(),
            "log_failures".to_string(),
        ],
    });
    
    // Set up comprehensive observability
    let metrics = Arc::new(MetricsCollector::new());
    let mut alert_manager = AlertManager::new();
    
    // Configure enterprise alerting
    alert_manager.add_alert_rule(AlertRule {
        name: "high_service_failure_rate".to_string(),
        condition: "service_orchestrator.error_count".to_string(),
        threshold: 5.0,
        action: "page_on_call_engineer".to_string(),
    });
    
    alert_manager.add_alert_rule(AlertRule {
        name: "message_processing_latency".to_string(),
        condition: "enterprise_integration.duration_ms".to_string(),
        threshold: 5000.0, // 5 seconds
        action: "slack_notification".to_string(),
    });
    
    // Create enterprise integration flow
    let mut flow = AsyncFlow::new(mq_consumer);
    flow.add_node("process_events".to_string(), event_processor);
    flow.add_node("orchestrate_services".to_string(), service_orchestrator);
    flow.add_node("audit_logging".to_string(), audit_logger);
    flow.set_metrics_collector(metrics.clone());
    flow.set_flow_name("enterprise_integration".to_string());
    
    // Set up shared state (normally this would come from actual message queue)
    let shared = SharedState::new();
    
    println!("Starting enterprise integration workflow...\n");
    
    // Run the enterprise integration pipeline
    let start = std::time::Instant::now();
    match flow.run_async(&shared).await {
        Ok(result) => {
            let duration = start.elapsed();
            println!("Enterprise integration completed in {:?}", duration);
            
            // Display detailed results
            if let Some(audit_result) = shared.get("audit_result") {
                let audit_entries = audit_result["audit_entries"].as_array().unwrap();
                println!("\nAudit Summary:");
                println!("- Total audit entries: {}", audit_entries.len());
                println!("- Compliance status: {}", audit_result["audit_summary"]["compliance_status"]);
            }
            
            if let Some(orchestration_result) = shared.get("orchestration_result") {
                let summary = &orchestration_result["orchestration_summary"];
                println!("\nService Orchestration Summary:");
                println!("- Total events processed: {}", summary["total_events"]);
                println!("- Successful orchestrations: {}", summary["successful_orchestrations"]);
                println!("- Partial failures: {}", summary["partial_failures"]);
            }
        }
        Err(e) => {
            println!("Enterprise integration failed: {}", e);
        }
    }
    
    // Check alerts
    alert_manager.check_alerts(&metrics);
    let triggered_alerts = alert_manager.get_triggered_alerts();
    if !triggered_alerts.is_empty() {
        println!("\n🚨 Triggered Alerts:");
        for alert in triggered_alerts {
            println!("- {}", alert);
        }
    } else {
        println!("\n✅ No alerts triggered - system operating normally");
    }
    
    // Display comprehensive metrics
    println!("\n--- Enterprise Integration Metrics ---");
    let total_executions = metrics.get_metric("enterprise_integration.execution_count").unwrap_or(0.0);
    let success_rate = metrics.get_metric("enterprise_integration.success_count").unwrap_or(0.0) / total_executions.max(1.0) * 100.0;
    let avg_duration = metrics.get_metric("enterprise_integration.duration_ms").unwrap_or(0.0) / total_executions.max(1.0);
    
    println!("Pipeline Performance:");
    println!("- Total executions: {}", total_executions);
    println!("- Success rate: {:.1}%", success_rate);
    println!("- Average duration: {:.1}ms", avg_duration);
    
    // Component-level metrics
    println!("\nComponent Performance:");
    let components = ["mq_consumer_enterprise_events", "event_processor_all", "service_orchestrator", "audit_logger"];
    for component in components {
        let executions = metrics.get_metric(&format!("{}.execution_count", component)).unwrap_or(0.0);
        let duration = metrics.get_metric(&format!("{}.duration_ms", component)).unwrap_or(0.0);
        if executions > 0.0 {
            println!("- {}: {:.1}ms avg", component, duration / executions);
        }
    }
    
    Ok(())
}
```

## Production Deployment Scenarios

### UC-7: Production Monitoring and Health Checks

**Scenario**: Production deployment with comprehensive monitoring, health checks, and automated recovery.

```rust
use agentflow_core::{AsyncFlow, AsyncNode, MetricsCollector, AlertManager, AlertRule};

// Production monitoring example
async fn production_monitoring_example() -> Result<()> {
    // This would typically be integrated with your production monitoring stack
    println!("Production monitoring integration example");
    
    // Set up comprehensive monitoring
    let metrics = Arc::new(MetricsCollector::new());
    let mut alert_manager = AlertManager::new();
    
    // Production alert rules
    alert_manager.add_alert_rule(AlertRule {
        name: "critical_error_rate".to_string(),
        condition: "production_flow.error_count".to_string(),
        threshold: 10.0,
        action: "page_oncall".to_string(),
    });
    
    alert_manager.add_alert_rule(AlertRule {
        name: "high_latency".to_string(),
        condition: "production_flow.duration_ms".to_string(),
        threshold: 10000.0, // 10 seconds
        action: "scale_up".to_string(),
    });
    
    // Production health check endpoints would integrate here
    println!("✅ Production monitoring configured");
    
    Ok(())
}

// Entry point for all use cases
#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 AgentFlow Use Cases Demo\n");
    
    // Uncomment to run specific use cases:
    
    // Basic patterns
    // main().await?;
    
    // Parallel processing
    // parallel_image_processing().await?;
    
    // ETL pipeline
    // run_etl_pipeline().await?;
    
    // Robust API integration
    // robust_api_integration_workflow().await?;
    
    // AI agent pipeline
    // ai_agent_pipeline().await?;
    
    // Enterprise integration
    // enterprise_integration_workflow().await?;
    
    // Production monitoring
    production_monitoring_example().await?;
    
    println!("\n✅ Use cases demonstration completed!");
    Ok(())
}
```

## Summary

This use cases document demonstrates AgentFlow's versatility across various domains:

1. **Basic Workflow Patterns**: Sequential processing with validation and transformation
2. **Data Processing Pipelines**: Parallel processing and ETL workflows
3. **API Integration**: Robust API calls with circuit breakers and retry logic
4. **ML/AI Workflows**: Multi-model AI pipelines with result aggregation
5. **Enterprise Integration**: Service bus patterns with comprehensive auditing
6. **Production Deployment**: Monitoring, alerting, and health checks

Each use case showcases different aspects of AgentFlow's capabilities:
- **Async/await patterns** for high-performance execution
- **Robustness patterns** for production reliability
- **Observability integration** for comprehensive monitoring
- **Flexible composition** for complex workflow orchestration
- **Type safety** and **memory safety** through Rust's ownership model

These examples serve as both documentation and starting points for implementing production workflows with AgentFlow.

---

For more implementation details, see the [Design Document](design.md) and [Functional Specification](functional-spec.md).