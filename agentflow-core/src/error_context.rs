//! Error context and detailed tracking for debugging
//!
//! This module provides comprehensive error context tracking for workflow execution,
//! including error chains, node context, and execution history.

use crate::error::AgentFlowError;
use crate::value::FlowValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Comprehensive error context for debugging and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Unique identifier for the workflow run
    pub run_id: String,

    /// Name of the node that failed
    pub node_name: String,

    /// Type of the node (e.g., "http", "llm", "template")
    pub node_type: Option<String>,

    /// When the error occurred
    #[serde(with = "humantime_serde")]
    pub timestamp: SystemTime,

    /// Complete error chain from root cause to current error
    pub error_chain: Vec<ErrorInfo>,

    /// Node inputs at the time of failure (optionally captured)
    pub inputs: Option<HashMap<String, String>>,

    /// Duration of node execution before failure
    #[serde(with = "humantime_serde")]
    pub duration: Duration,

    /// List of successfully executed nodes before this failure
    pub execution_history: Vec<String>,

    /// Retry attempt number (if retries were attempted)
    pub retry_attempt: Option<u32>,

    /// Additional metadata for debugging
    pub metadata: HashMap<String, String>,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(run_id: String, node_name: String) -> Self {
        Self {
            run_id,
            node_name,
            node_type: None,
            timestamp: SystemTime::now(),
            error_chain: Vec::new(),
            inputs: None,
            duration: Duration::ZERO,
            execution_history: Vec::new(),
            retry_attempt: None,
            metadata: HashMap::new(),
        }
    }

    /// Builder for error context
    pub fn builder(run_id: impl Into<String>, node_name: impl Into<String>) -> ErrorContextBuilder {
        ErrorContextBuilder::new(run_id, node_name)
    }

    /// Add an error to the error chain
    pub fn add_error(&mut self, error: &AgentFlowError) {
        self.error_chain.push(ErrorInfo::from_error(error));
    }

    /// Set node inputs (sanitized for large values)
    pub fn set_inputs(&mut self, inputs: &HashMap<String, FlowValue>) {
        let sanitized: HashMap<String, String> = inputs
            .iter()
            .map(|(k, v)| {
                let value_str = match v {
                    FlowValue::Json(json) => {
                        let s = json.to_string();
                        if s.len() > 500 {
                            format!("{}... (truncated, {} bytes)", &s[..500], s.len())
                        } else {
                            s
                        }
                    }
                    FlowValue::File { path, mime_type } => {
                        let mime_str = mime_type.as_deref().unwrap_or("unknown");
                        format!("<file: {} ({})>", path.display(), mime_str)
                    }
                    FlowValue::Url { url, mime_type } => {
                        let mime_str = mime_type.as_deref().unwrap_or("unknown");
                        format!("<url: {} ({})>", url, mime_str)
                    }
                };
                (k.clone(), value_str)
            })
            .collect();
        self.inputs = Some(sanitized);
    }

    /// Get a human-readable error summary
    pub fn summary(&self) -> String {
        let root_error = self.error_chain
            .first()
            .map(|e| e.message.as_str())
            .unwrap_or("Unknown error");

        let retry_info = self.retry_attempt
            .map(|n| format!(" (attempt {})", n + 1))
            .unwrap_or_default();

        format!(
            "Node '{}' failed after {:?}{}: {}",
            self.node_name,
            self.duration,
            retry_info,
            root_error
        )
    }

    /// Get full error chain as a formatted string
    pub fn error_chain_str(&self) -> String {
        self.error_chain
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if i == 0 {
                    format!("Root cause: {}", e.message)
                } else {
                    format!("  → {}", e.message)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Generate a detailed report for logging/debugging
    pub fn detailed_report(&self) -> String {
        let mut report = String::new();

        report.push_str(&format!("╔══════════════════════════════════════════════════════════════╗\n"));
        report.push_str(&format!("║ ERROR CONTEXT REPORT                                         ║\n"));
        report.push_str(&format!("╠══════════════════════════════════════════════════════════════╣\n"));
        report.push_str(&format!("  Run ID: {}\n", self.run_id));
        report.push_str(&format!("  Failed Node: {} ({})\n",
            self.node_name,
            self.node_type.as_deref().unwrap_or("unknown")));
        report.push_str(&format!("  Timestamp: {:?}\n", self.timestamp));
        report.push_str(&format!("  Duration: {:?}\n", self.duration));

        if let Some(attempt) = self.retry_attempt {
            report.push_str(&format!("  Retry Attempt: {}\n", attempt + 1));
        }

        report.push_str(&format!("╠══════════════════════════════════════════════════════════════╣\n"));
        report.push_str(&format!("  ERROR CHAIN:\n"));
        for (i, error_info) in self.error_chain.iter().enumerate() {
            if i == 0 {
                report.push_str(&format!("    [Root] {}: {}\n", error_info.error_type, error_info.message));
            } else {
                report.push_str(&format!("      ↳ {}: {}\n", error_info.error_type, error_info.message));
            }
            if let Some(source) = &error_info.source {
                report.push_str(&format!("         Source: {}\n", source));
            }
        }

        if !self.execution_history.is_empty() {
            report.push_str(&format!("╠══════════════════════════════════════════════════════════════╣\n"));
            report.push_str(&format!("  EXECUTION HISTORY:\n"));
            for (i, node) in self.execution_history.iter().enumerate() {
                report.push_str(&format!("    {}. {}\n", i + 1, node));
            }
        }

        if let Some(inputs) = &self.inputs {
            if !inputs.is_empty() {
                report.push_str(&format!("╠══════════════════════════════════════════════════════════════╣\n"));
                report.push_str(&format!("  NODE INPUTS:\n"));
                for (key, value) in inputs.iter() {
                    report.push_str(&format!("    {}: {}\n", key, value));
                }
            }
        }

        if !self.metadata.is_empty() {
            report.push_str(&format!("╠══════════════════════════════════════════════════════════════╣\n"));
            report.push_str(&format!("  METADATA:\n"));
            for (key, value) in self.metadata.iter() {
                report.push_str(&format!("    {}: {}\n", key, value));
            }
        }

        report.push_str(&format!("╚══════════════════════════════════════════════════════════════╝\n"));

        report
    }
}

/// Builder for ErrorContext
pub struct ErrorContextBuilder {
    context: ErrorContext,
}

impl ErrorContextBuilder {
    pub fn new(run_id: impl Into<String>, node_name: impl Into<String>) -> Self {
        Self {
            context: ErrorContext::new(run_id.into(), node_name.into()),
        }
    }

    pub fn node_type(mut self, node_type: impl Into<String>) -> Self {
        self.context.node_type = Some(node_type.into());
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.context.duration = duration;
        self
    }

    pub fn execution_history(mut self, history: Vec<String>) -> Self {
        self.context.execution_history = history;
        self
    }

    pub fn retry_attempt(mut self, attempt: u32) -> Self {
        self.context.retry_attempt = Some(attempt);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.metadata.insert(key.into(), value.into());
        self
    }

    pub fn inputs(mut self, inputs: &HashMap<String, FlowValue>) -> Self {
        self.context.set_inputs(inputs);
        self
    }

    pub fn error(mut self, error: &AgentFlowError) -> Self {
        self.context.add_error(error);
        self
    }

    pub fn build(self) -> ErrorContext {
        self.context
    }
}

/// Information about a single error in the error chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Type/category of the error
    pub error_type: String,

    /// Error message
    pub message: String,

    /// Optional source/cause of the error
    pub source: Option<String>,
}

impl ErrorInfo {
    /// Create ErrorInfo from AgentFlowError
    pub fn from_error(error: &AgentFlowError) -> Self {
        let error_type = match error {
            AgentFlowError::ConfigurationError { .. } => "ConfigurationError",
            AgentFlowError::NodeExecutionFailed { .. } => "NodeExecutionFailed",
            AgentFlowError::NodeInputError { .. } => "NodeInputError",
            AgentFlowError::AsyncExecutionError { .. } => "AsyncExecutionError",
            AgentFlowError::FlowExecutionFailed { .. } => "FlowExecutionFailed",
            AgentFlowError::FlowDefinitionError { .. } => "FlowDefinitionError",
            AgentFlowError::SerializationError(_) => "SerializationError",
            AgentFlowError::TimeoutExceeded { .. } => "TimeoutExceeded",
            AgentFlowError::CircuitBreakerOpen { .. } => "CircuitBreakerOpen",
            AgentFlowError::RateLimitExceeded { .. } => "RateLimitExceeded",
            AgentFlowError::RetryExhausted { .. } => "RetryExhausted",
            _ => "Unknown",
        };

        Self {
            error_type: error_type.to_string(),
            message: error.to_string(),
            source: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_error_context_creation() {
        let context = ErrorContext::builder("run-123", "test_node")
            .node_type("http")
            .duration(Duration::from_millis(150))
            .build();

        assert_eq!(context.run_id, "run-123");
        assert_eq!(context.node_name, "test_node");
        assert_eq!(context.node_type, Some("http".to_string()));
        assert_eq!(context.duration, Duration::from_millis(150));
    }

    #[test]
    fn test_error_chain() {
        let mut context = ErrorContext::new("run-123".to_string(), "test_node".to_string());

        let error1 = AgentFlowError::NodeExecutionFailed {
            message: "First error".to_string(),
        };
        let error2 = AgentFlowError::AsyncExecutionError {
            message: "Second error".to_string(),
        };

        context.add_error(&error1);
        context.add_error(&error2);

        assert_eq!(context.error_chain.len(), 2);
        assert_eq!(context.error_chain[0].error_type, "NodeExecutionFailed");
        assert_eq!(context.error_chain[1].error_type, "AsyncExecutionError");
    }

    #[test]
    fn test_input_sanitization() {
        use std::path::PathBuf;

        let mut context = ErrorContext::new("run-123".to_string(), "test_node".to_string());

        let mut inputs = HashMap::new();
        inputs.insert("small".to_string(), FlowValue::Json(json!("test")));
        inputs.insert("file".to_string(), FlowValue::File {
            path: PathBuf::from("/path/to/file"),
            mime_type: Some("text/plain".to_string()),
        });

        // Very long JSON value
        let long_json = json!("x".repeat(1000));
        inputs.insert("large".to_string(), FlowValue::Json(long_json));

        context.set_inputs(&inputs);

        let sanitized = context.inputs.unwrap();
        assert!(sanitized["small"].contains("test"));
        assert!(sanitized["file"].contains("<file:"));
        assert!(sanitized["large"].contains("truncated"));
    }

    #[test]
    fn test_summary() {
        let context = ErrorContext::builder("run-123", "test_node")
            .duration(Duration::from_millis(250))
            .retry_attempt(1)
            .error(&AgentFlowError::TimeoutExceeded { duration_ms: 1000 })
            .build();

        let summary = context.summary();
        assert!(summary.contains("test_node"));
        assert!(summary.contains("attempt 2")); // retry_attempt is 0-indexed
        assert!(summary.contains("Timeout"));
    }

    #[test]
    fn test_detailed_report() {
        let mut inputs = HashMap::new();
        inputs.insert("key".to_string(), FlowValue::Json(json!("value")));

        let context = ErrorContext::builder("run-123", "test_node")
            .node_type("http")
            .duration(Duration::from_millis(100))
            .execution_history(vec!["node1".to_string(), "node2".to_string()])
            .inputs(&inputs)
            .error(&AgentFlowError::AsyncExecutionError {
                message: "Network error".to_string(),
            })
            .build();

        let report = context.detailed_report();
        assert!(report.contains("ERROR CONTEXT REPORT"));
        assert!(report.contains("test_node"));
        assert!(report.contains("http"));
        assert!(report.contains("EXECUTION HISTORY"));
        assert!(report.contains("NODE INPUTS"));
        assert!(report.contains("Network error"));
    }
}
