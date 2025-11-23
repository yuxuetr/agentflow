//! Trace collector - implements EventListener to collect workflow traces

use crate::storage::TraceStorage;
use crate::types::*;
use agentflow_core::events::{EventListener, WorkflowEvent};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trace collector configuration
#[derive(Debug, Clone)]
pub struct TraceConfig {
  /// Whether to capture input/output data (may contain sensitive information)
  pub capture_io: bool,

  /// Whether to capture LLM prompts
  pub capture_prompts: bool,

  /// Maximum size for input/output data in bytes (prevents huge logs)
  pub max_io_size_bytes: usize,

  /// Whether to use async storage (recommended)
  pub async_storage: bool,

  /// Behavior when storage fails
  pub on_storage_error: StorageErrorPolicy,
}

impl Default for TraceConfig {
  fn default() -> Self {
    Self {
      capture_io: true,
      capture_prompts: true,
      max_io_size_bytes: 1024 * 1024, // 1MB
      async_storage: true,
      on_storage_error: StorageErrorPolicy::LogError,
    }
  }
}

impl TraceConfig {
  /// Production configuration (more restrictive)
  pub fn production() -> Self {
    Self {
      capture_io: false, // Don't capture sensitive data
      capture_prompts: false,
      max_io_size_bytes: 0,
      async_storage: true,
      on_storage_error: StorageErrorPolicy::LogError,
    }
  }

  /// Development configuration (full tracing)
  pub fn development() -> Self {
    Self {
      capture_io: true,
      capture_prompts: true,
      max_io_size_bytes: 10 * 1024 * 1024, // 10MB
      async_storage: true,
      on_storage_error: StorageErrorPolicy::Ignore,
    }
  }
}

/// Storage error handling policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageErrorPolicy {
  /// Ignore storage errors silently
  Ignore,

  /// Log error but continue execution
  LogError,

  /// Fail workflow if storage fails (not recommended for production)
  FailWorkflow,
}

/// Trace collector - collects execution traces by listening to workflow events
pub struct TraceCollector {
  /// Storage backend
  storage: Arc<dyn TraceStorage>,

  /// Configuration
  config: TraceConfig,

  /// Currently running traces (in-memory)
  current_traces: Arc<RwLock<HashMap<String, ExecutionTrace>>>,

  /// Pending LLM prompts (workflow_id, node_id) -> LLMTrace
  pending_llm: Arc<RwLock<HashMap<(String, String), LLMTrace>>>,
}

impl TraceCollector {
  /// Create a new trace collector
  pub fn new(storage: Arc<dyn TraceStorage>, config: TraceConfig) -> Self {
    Self {
      storage,
      config,
      current_traces: Arc::new(RwLock::new(HashMap::new())),
      pending_llm: Arc::new(RwLock::new(HashMap::new())),
    }
  }

  /// Get a trace by workflow ID
  pub async fn get_trace(
    &self,
    workflow_id: &str,
  ) -> Result<Option<ExecutionTrace>, anyhow::Error> {
    // Check in-memory first (running workflows)
    {
      let traces = self.current_traces.read().await;
      if let Some(trace) = traces.get(workflow_id) {
        return Ok(Some(trace.clone()));
      }
    }

    // Check storage (completed workflows)
    self.storage.get_trace(workflow_id).await
  }

  /// Query traces with filters
  pub async fn query_traces(
    &self,
    query: crate::storage::TraceQuery,
  ) -> Result<Vec<ExecutionTrace>, anyhow::Error> {
    self.storage.query_traces(query).await
  }

  /// List all currently running workflows
  pub async fn list_running(&self) -> Vec<ExecutionTrace> {
    let traces = self.current_traces.read().await;
    traces.values().cloned().collect()
  }

  /// Process an event asynchronously
  async fn process_event(
    storage: Arc<dyn TraceStorage>,
    traces: Arc<RwLock<HashMap<String, ExecutionTrace>>>,
    pending_llm: Arc<RwLock<HashMap<(String, String), LLMTrace>>>,
    config: TraceConfig,
    event: WorkflowEvent,
  ) -> Result<(), anyhow::Error> {
    match event {
      WorkflowEvent::WorkflowStarted {
        workflow_id,
        timestamp: _,
      } => {
        let mut trace = ExecutionTrace::new(workflow_id.clone());
        trace.metadata.environment = std::env::var("AGENTFLOW_ENV")
          .unwrap_or_else(|_| "development".to_string());

        traces.write().await.insert(workflow_id, trace);
      }

      WorkflowEvent::NodeStarted {
        workflow_id,
        node_id,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(trace) = traces_guard.get_mut(&workflow_id) {
          // Extract node_type from node_id (format: "type:id" or just "id")
          let node_type = node_id
            .split(':')
            .next()
            .unwrap_or("Unknown")
            .to_string();

          let node_trace = NodeTrace::new(node_id, node_type);
          trace.nodes.push(node_trace);
        }
      }

      WorkflowEvent::NodeCompleted {
        workflow_id,
        node_id,
        duration,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(trace) = traces_guard.get_mut(&workflow_id) {
          if let Some(node) = trace.nodes.iter_mut().rev().find(|n| n.node_id == node_id)
          {
            node.complete();
            node.duration_ms = Some(duration.as_millis() as u64);
          }
        }
      }

      WorkflowEvent::NodeFailed {
        workflow_id,
        node_id,
        error,
        duration,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(trace) = traces_guard.get_mut(&workflow_id) {
          if let Some(node) = trace.nodes.iter_mut().rev().find(|n| n.node_id == node_id)
          {
            node.fail(error);
            node.duration_ms = Some(duration.as_millis() as u64);
          }
        }
      }

      WorkflowEvent::NodeSkipped {
        workflow_id,
        node_id,
        reason,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(trace) = traces_guard.get_mut(&workflow_id) {
          if let Some(node) = trace.nodes.iter_mut().rev().find(|n| n.node_id == node_id)
          {
            node.status = NodeStatus::Skipped;
            node.error = Some(format!("Skipped: {}", reason));
          }
        }
      }

      WorkflowEvent::LLMPromptSent {
        workflow_id,
        node_id,
        model,
        provider,
        system_prompt,
        user_prompt,
        temperature,
        max_tokens,
        timestamp: _,
      } => {
        // Create LLM trace (will be filled in when response received)
        let llm_trace = LLMTrace {
          model,
          provider,
          system_prompt,
          user_prompt,
          response: String::new(), // Will be filled later
          temperature,
          max_tokens,
          usage: None, // Will be filled later
          latency_ms: 0, // Will be calculated later
        };

        // Store in pending LLM map
        pending_llm
          .write()
          .await
          .insert((workflow_id, node_id), llm_trace);
      }

      WorkflowEvent::LLMResponseReceived {
        workflow_id,
        node_id,
        model: _,
        response,
        usage,
        duration,
        timestamp: _,
      } => {
        // Get and remove pending LLM trace
        let llm_trace_opt = pending_llm
          .write()
          .await
          .remove(&(workflow_id.clone(), node_id.clone()));

        if let Some(mut llm_trace) = llm_trace_opt {
          // Fill in response details
          llm_trace.response = response;
          llm_trace.latency_ms = duration.as_millis() as u64;
          llm_trace.usage = usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            estimated_cost_usd: None, // Could calculate based on model pricing
          });

          // Attach to node trace
          let mut traces_guard = traces.write().await;
          if let Some(trace) = traces_guard.get_mut(&workflow_id) {
            if let Some(node) =
              trace.nodes.iter_mut().rev().find(|n| n.node_id == node_id)
            {
              node.llm_details = Some(llm_trace);
            }
          }
        }
      }

      WorkflowEvent::WorkflowCompleted {
        workflow_id,
        duration: _,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(mut trace) = traces_guard.remove(&workflow_id) {
          trace.completed_at = Some(chrono::Utc::now());
          trace.status = TraceStatus::Completed;

          // Save to storage
          if let Err(e) = storage.save_trace(&trace).await {
            Self::handle_storage_error(&config, e);
          }
        }
      }

      WorkflowEvent::WorkflowFailed {
        workflow_id,
        error,
        duration: _,
        timestamp: _,
      } => {
        let mut traces_guard = traces.write().await;
        if let Some(mut trace) = traces_guard.remove(&workflow_id) {
          trace.completed_at = Some(chrono::Utc::now());
          trace.status = TraceStatus::Failed { error };

          // Save to storage
          if let Err(e) = storage.save_trace(&trace).await {
            Self::handle_storage_error(&config, e);
          }
        }
      }

      // Other events can be ignored for now
      _ => {}
    }

    Ok(())
  }

  /// Handle storage errors according to policy
  fn handle_storage_error(config: &TraceConfig, error: anyhow::Error) {
    match config.on_storage_error {
      StorageErrorPolicy::Ignore => {
        // Do nothing
      }
      StorageErrorPolicy::LogError => {
        eprintln!("Trace storage error: {}", error);
      }
      StorageErrorPolicy::FailWorkflow => {
        panic!("Trace storage failed: {}", error);
      }
    }
  }

  /// Sanitize value to remove sensitive data and limit size
  fn sanitize_value(
    value: &mut serde_json::Value,
    max_size: usize,
  ) -> Result<(), anyhow::Error> {
    // Remove sensitive keys
    if let serde_json::Value::Object(map) = value {
      let sensitive_keys = ["api_key", "password", "token", "secret", "credential"];
      for key in sensitive_keys {
        if map.contains_key(key) {
          map.insert(
            key.to_string(),
            serde_json::Value::String("[REDACTED]".to_string()),
          );
        }
      }
    }

    // Limit size
    let json_str = serde_json::to_string(value)?;
    if json_str.len() > max_size {
      *value = serde_json::Value::String(format!(
        "[TRUNCATED: {} bytes]",
        json_str.len()
      ));
    }

    Ok(())
  }
}

impl EventListener for TraceCollector {
  fn on_event(&self, event: &WorkflowEvent) {
    // Clone what we need for async task
    let storage = self.storage.clone();
    let traces = self.current_traces.clone();
    let pending_llm = self.pending_llm.clone();
    let config = self.config.clone();
    let event = event.clone();

    // Spawn async task to process event (non-blocking)
    if self.config.async_storage {
      tokio::spawn(async move {
        if let Err(e) =
          Self::process_event(storage, traces, pending_llm, config.clone(), event).await
        {
          Self::handle_storage_error(&config, e);
        }
      });
    } else {
      // Blocking mode (for testing or special cases)
      let rt = tokio::runtime::Handle::current();
      rt.block_on(async {
        if let Err(e) =
          Self::process_event(storage, traces, pending_llm, config.clone(), event).await
        {
          Self::handle_storage_error(&config, e);
        }
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::file::FileTraceStorage;
  use std::time::Duration as StdDuration;
  use tempfile::tempdir;

  #[tokio::test]
  async fn test_trace_collector_workflow_lifecycle() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let config = TraceConfig::development();
    let collector = TraceCollector::new(storage.clone(), config);

    let workflow_id = "test-wf-1".to_string();

    // Start workflow
    collector.on_event(&WorkflowEvent::WorkflowStarted {
      workflow_id: workflow_id.clone(),
      timestamp: std::time::Instant::now(),
    });

    // Give async task time to process
    tokio::time::sleep(StdDuration::from_millis(50)).await;

    // Check trace exists
    let trace = collector.get_trace(&workflow_id).await.unwrap();
    assert!(trace.is_some());
    assert!(trace.unwrap().is_running());

    // Complete workflow
    collector.on_event(&WorkflowEvent::WorkflowCompleted {
      workflow_id: workflow_id.clone(),
      duration: StdDuration::from_secs(5),
      timestamp: std::time::Instant::now(),
    });

    tokio::time::sleep(StdDuration::from_millis(100)).await;

    // Check trace is completed and stored
    let trace = storage.get_trace(&workflow_id).await.unwrap();
    assert!(trace.is_some());
    assert!(trace.unwrap().is_completed());
  }

  #[tokio::test]
  async fn test_trace_collector_node_tracking() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let collector = TraceCollector::new(storage, TraceConfig::development());

    let workflow_id = "test-wf-2".to_string();
    let node_id = "node1".to_string();

    collector.on_event(&WorkflowEvent::WorkflowStarted {
      workflow_id: workflow_id.clone(),
      timestamp: std::time::Instant::now(),
    });

    tokio::time::sleep(StdDuration::from_millis(50)).await;

    collector.on_event(&WorkflowEvent::NodeStarted {
      workflow_id: workflow_id.clone(),
      node_id: node_id.clone(),
      timestamp: std::time::Instant::now(),
    });

    tokio::time::sleep(StdDuration::from_millis(50)).await;

    collector.on_event(&WorkflowEvent::NodeCompleted {
      workflow_id: workflow_id.clone(),
      node_id: node_id.clone(),
      duration: StdDuration::from_millis(100),
      timestamp: std::time::Instant::now(),
    });

    tokio::time::sleep(StdDuration::from_millis(50)).await;

    let trace = collector.get_trace(&workflow_id).await.unwrap().unwrap();
    assert_eq!(trace.nodes.len(), 1);
    assert_eq!(trace.nodes[0].node_id, node_id);
    assert_eq!(trace.nodes[0].status, NodeStatus::Completed);
  }
}
