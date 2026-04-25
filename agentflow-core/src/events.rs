//! Lightweight event system for workflow observability
//!
//! This module provides a minimal event definition system that allows users to
//! optionally observe workflow execution without forcing any specific logging or
//! metrics implementation.
//!
//! ## Design Philosophy
//!
//! - **Zero-cost abstraction**: If you don't use events, zero overhead
//! - **No dependencies**: Pure Rust, no logging/metrics libraries
//! - **User choice**: Users decide how to handle events (logs, metrics, traces, etc.)
//!
//! ## Example
//!
//! ```rust
//! use agentflow_core::events::{WorkflowEvent, EventListener};
//!
//! struct MyListener;
//!
//! impl EventListener for MyListener {
//!     fn on_event(&self, event: &WorkflowEvent) {
//!         match event {
//!             WorkflowEvent::NodeCompleted { node_id, duration, .. } => {
//!                 println!("Node {} completed in {:?}", node_id, duration);
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

use std::fmt;
use std::time::{Duration, Instant};

/// Workflow execution events
///
/// These events represent significant points in workflow execution.
/// Users can listen to these events to implement their own logging,
/// metrics, or tracing.
#[derive(Debug, Clone)]
pub enum WorkflowEvent {
  /// Workflow execution started
  WorkflowStarted {
    workflow_id: String,
    timestamp: Instant,
  },

  /// Workflow execution completed successfully
  WorkflowCompleted {
    workflow_id: String,
    duration: Duration,
    timestamp: Instant,
  },

  /// Workflow execution failed
  WorkflowFailed {
    workflow_id: String,
    error: String,
    duration: Duration,
    timestamp: Instant,
  },

  /// Node execution started
  NodeStarted {
    workflow_id: String,
    node_id: String,
    timestamp: Instant,
  },

  /// Node execution completed successfully
  NodeCompleted {
    workflow_id: String,
    node_id: String,
    duration: Duration,
    timestamp: Instant,
  },

  /// Node execution failed
  NodeFailed {
    workflow_id: String,
    node_id: String,
    error: String,
    duration: Duration,
    timestamp: Instant,
  },

  /// Node was skipped (e.g., due to condition)
  NodeSkipped {
    workflow_id: String,
    node_id: String,
    reason: String,
    timestamp: Instant,
  },

  /// Checkpoint saved
  CheckpointSaved {
    workflow_id: String,
    checkpoint_id: String,
    timestamp: Instant,
  },

  /// Checkpoint restored
  CheckpointRestored {
    workflow_id: String,
    checkpoint_id: String,
    timestamp: Instant,
  },

  /// Retry attempt
  RetryAttempt {
    workflow_id: String,
    node_id: String,
    attempt: u32,
    max_attempts: u32,
    timestamp: Instant,
  },

  /// Resource limit warning
  ResourceWarning {
    workflow_id: String,
    resource_type: String,
    usage: f64,
    limit: f64,
    timestamp: Instant,
  },

  /// LLM prompt sent (for detailed tracing)
  LLMPromptSent {
    workflow_id: String,
    node_id: String,
    model: String,
    provider: String,
    system_prompt: Option<String>,
    user_prompt: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    timestamp: Instant,
  },

  /// LLM response received (for detailed tracing)
  LLMResponseReceived {
    workflow_id: String,
    node_id: String,
    model: String,
    response: String,
    usage: Option<TokenUsage>,
    duration: Duration,
    timestamp: Instant,
  },
}

/// Token usage statistics for LLM calls
#[derive(Debug, Clone)]
pub struct TokenUsage {
  pub prompt_tokens: u32,
  pub completion_tokens: u32,
  pub total_tokens: u32,
}

impl WorkflowEvent {
  /// Get the workflow ID associated with this event
  pub fn workflow_id(&self) -> &str {
    match self {
      Self::WorkflowStarted { workflow_id, .. }
      | Self::WorkflowCompleted { workflow_id, .. }
      | Self::WorkflowFailed { workflow_id, .. }
      | Self::NodeStarted { workflow_id, .. }
      | Self::NodeCompleted { workflow_id, .. }
      | Self::NodeFailed { workflow_id, .. }
      | Self::NodeSkipped { workflow_id, .. }
      | Self::CheckpointSaved { workflow_id, .. }
      | Self::CheckpointRestored { workflow_id, .. }
      | Self::RetryAttempt { workflow_id, .. }
      | Self::ResourceWarning { workflow_id, .. }
      | Self::LLMPromptSent { workflow_id, .. }
      | Self::LLMResponseReceived { workflow_id, .. } => workflow_id,
    }
  }

  /// Get the timestamp of this event
  pub fn timestamp(&self) -> Instant {
    match self {
      Self::WorkflowStarted { timestamp, .. }
      | Self::WorkflowCompleted { timestamp, .. }
      | Self::WorkflowFailed { timestamp, .. }
      | Self::NodeStarted { timestamp, .. }
      | Self::NodeCompleted { timestamp, .. }
      | Self::NodeFailed { timestamp, .. }
      | Self::NodeSkipped { timestamp, .. }
      | Self::CheckpointSaved { timestamp, .. }
      | Self::CheckpointRestored { timestamp, .. }
      | Self::RetryAttempt { timestamp, .. }
      | Self::ResourceWarning { timestamp, .. }
      | Self::LLMPromptSent { timestamp, .. }
      | Self::LLMResponseReceived { timestamp, .. } => *timestamp,
    }
  }

  /// Get a human-readable event type name
  pub fn event_type(&self) -> &'static str {
    match self {
      Self::WorkflowStarted { .. } => "workflow.started",
      Self::WorkflowCompleted { .. } => "workflow.completed",
      Self::WorkflowFailed { .. } => "workflow.failed",
      Self::NodeStarted { .. } => "node.started",
      Self::NodeCompleted { .. } => "node.completed",
      Self::NodeFailed { .. } => "node.failed",
      Self::NodeSkipped { .. } => "node.skipped",
      Self::CheckpointSaved { .. } => "checkpoint.saved",
      Self::CheckpointRestored { .. } => "checkpoint.restored",
      Self::RetryAttempt { .. } => "retry.attempt",
      Self::ResourceWarning { .. } => "resource.warning",
      Self::LLMPromptSent { .. } => "llm.prompt.sent",
      Self::LLMResponseReceived { .. } => "llm.response.received",
    }
  }
}

impl fmt::Display for WorkflowEvent {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::WorkflowStarted { workflow_id, .. } => {
        write!(f, "Workflow '{}' started", workflow_id)
      }
      Self::WorkflowCompleted {
        workflow_id,
        duration,
        ..
      } => {
        write!(f, "Workflow '{}' completed in {:?}", workflow_id, duration)
      }
      Self::WorkflowFailed {
        workflow_id, error, ..
      } => {
        write!(f, "Workflow '{}' failed: {}", workflow_id, error)
      }
      Self::NodeStarted { node_id, .. } => {
        write!(f, "Node '{}' started", node_id)
      }
      Self::NodeCompleted {
        node_id, duration, ..
      } => {
        write!(f, "Node '{}' completed in {:?}", node_id, duration)
      }
      Self::NodeFailed { node_id, error, .. } => {
        write!(f, "Node '{}' failed: {}", node_id, error)
      }
      Self::NodeSkipped {
        node_id, reason, ..
      } => {
        write!(f, "Node '{}' skipped: {}", node_id, reason)
      }
      Self::CheckpointSaved { checkpoint_id, .. } => {
        write!(f, "Checkpoint '{}' saved", checkpoint_id)
      }
      Self::CheckpointRestored { checkpoint_id, .. } => {
        write!(f, "Checkpoint '{}' restored", checkpoint_id)
      }
      Self::RetryAttempt {
        node_id,
        attempt,
        max_attempts,
        ..
      } => {
        write!(
          f,
          "Retry attempt {}/{} for node '{}'",
          attempt, max_attempts, node_id
        )
      }
      Self::ResourceWarning {
        resource_type,
        usage,
        limit,
        ..
      } => {
        write!(
          f,
          "Resource warning: {} usage {:.1}% (limit: {})",
          resource_type,
          usage * 100.0,
          limit
        )
      }
      Self::LLMPromptSent {
        node_id,
        model,
        provider,
        ..
      } => {
        write!(
          f,
          "LLM prompt sent to {} ({}) for node '{}'",
          model, provider, node_id
        )
      }
      Self::LLMResponseReceived {
        node_id,
        model,
        duration,
        ..
      } => {
        write!(
          f,
          "LLM response received from {} for node '{}' in {:?}",
          model, node_id, duration
        )
      }
    }
  }
}

/// Event listener trait
///
/// Implement this trait to receive workflow events. You can use this to:
/// - Log events to stdout, files, or external systems
/// - Collect metrics (Prometheus, StatsD, etc.)
/// - Send traces (OpenTelemetry, Jaeger, etc.)
/// - Trigger alerts or webhooks
///
/// ## Example
///
/// ```rust
/// use agentflow_core::events::{WorkflowEvent, EventListener};
///
/// struct ConsoleLogger;
///
/// impl EventListener for ConsoleLogger {
///     fn on_event(&self, event: &WorkflowEvent) {
///         println!("[{}] {}", event.event_type(), event);
///     }
/// }
/// ```
pub trait EventListener: Send + Sync {
  /// Called when a workflow event occurs
  fn on_event(&self, event: &WorkflowEvent);

  /// Called when multiple events occur (batch processing)
  ///
  /// Default implementation calls `on_event` for each event.
  /// Override this for more efficient batch processing.
  fn on_events(&self, events: &[WorkflowEvent]) {
    for event in events {
      self.on_event(event);
    }
  }
}

/// No-op event listener (does nothing)
///
/// This is the default listener when no listener is configured.
/// It has zero runtime overhead.
pub struct NoOpListener;

impl EventListener for NoOpListener {
  #[inline]
  fn on_event(&self, _event: &WorkflowEvent) {
    // Intentionally empty - compiler will optimize this away
  }
}

/// Console event listener (prints to stdout)
///
/// Simple listener that prints events to standard output.
/// Useful for debugging and development.
///
/// ## Example
///
/// ```rust
/// use agentflow_core::events::{ConsoleListener, EventListener, WorkflowEvent};
/// use std::time::Instant;
///
/// let listener = ConsoleListener;
/// listener.on_event(&WorkflowEvent::WorkflowStarted {
///     workflow_id: "demo".to_string(),
///     timestamp: Instant::now(),
/// });
/// ```
pub struct ConsoleListener;

impl EventListener for ConsoleListener {
  fn on_event(&self, event: &WorkflowEvent) {
    println!("[{}] {}", event.event_type(), event);
  }
}

/// Multi-listener combinator
///
/// Forwards events to multiple listeners.
///
/// ## Example
///
/// ```rust
/// use agentflow_core::events::{ConsoleListener, MultiListener};
///
/// let listener = MultiListener::new(vec![
///     Box::new(ConsoleListener),
///     // Box::new(MyMetricsListener),
///     // Box::new(MyTracingListener),
/// ]);
/// ```
pub struct MultiListener {
  listeners: Vec<Box<dyn EventListener>>,
}

impl MultiListener {
  /// Create a new multi-listener
  pub fn new(listeners: Vec<Box<dyn EventListener>>) -> Self {
    Self { listeners }
  }

  /// Add a listener
  pub fn add(&mut self, listener: Box<dyn EventListener>) {
    self.listeners.push(listener);
  }
}

impl EventListener for MultiListener {
  fn on_event(&self, event: &WorkflowEvent) {
    for listener in &self.listeners {
      listener.on_event(event);
    }
  }

  fn on_events(&self, events: &[WorkflowEvent]) {
    for listener in &self.listeners {
      listener.on_events(events);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_event_workflow_id() {
    let event = WorkflowEvent::WorkflowStarted {
      workflow_id: "test-wf".to_string(),
      timestamp: Instant::now(),
    };
    assert_eq!(event.workflow_id(), "test-wf");
  }

  #[test]
  fn test_event_type() {
    let event = WorkflowEvent::NodeCompleted {
      workflow_id: "wf".into(),
      node_id: "node1".into(),
      duration: Duration::from_secs(1),
      timestamp: Instant::now(),
    };
    assert_eq!(event.event_type(), "node.completed");
  }

  #[test]
  fn test_noop_listener() {
    let listener = NoOpListener;
    let event = WorkflowEvent::WorkflowStarted {
      workflow_id: "test".into(),
      timestamp: Instant::now(),
    };
    listener.on_event(&event); // Should do nothing
  }

  #[test]
  fn test_multi_listener() {
    let listener = MultiListener::new(vec![Box::new(NoOpListener), Box::new(ConsoleListener)]);

    let event = WorkflowEvent::NodeStarted {
      workflow_id: "wf".into(),
      node_id: "node1".into(),
      timestamp: Instant::now(),
    };

    listener.on_event(&event);
  }

  #[test]
  fn test_event_display() {
    let event = WorkflowEvent::WorkflowCompleted {
      workflow_id: "test-wf".into(),
      duration: Duration::from_secs(5),
      timestamp: Instant::now(),
    };

    let display = format!("{}", event);
    assert!(display.contains("test-wf"));
    assert!(display.contains("completed"));
  }
}
