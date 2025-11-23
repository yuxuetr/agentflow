//! AgentFlow Tracing - Workflow execution tracing and logging
//!
//! This crate provides detailed execution tracing for AgentFlow workflows.
//! It captures node inputs/outputs, LLM interactions, execution metrics,
//! and provides queryable logs for debugging and analysis.
//!
//! ## Design Philosophy
//!
//! - **Non-invasive**: Integrates via EventListener trait from agentflow-core
//! - **Zero overhead**: If not enabled, no performance impact
//! - **Flexible storage**: File-based (dev) or database (production)
//! - **User-friendly**: Queryable, filterable logs for debugging
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use agentflow_tracing::{TraceCollector, TraceConfig};
//! use agentflow_tracing::storage::file::FileTraceStorage;
//! use std::sync::Arc;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 1. Create storage
//!     let storage = Arc::new(FileTraceStorage::new(
//!         PathBuf::from("./traces")
//!     )?);
//!
//!     // 2. Create trace collector
//!     let _collector = TraceCollector::new(
//!         storage.clone(),
//!         TraceConfig::development()
//!     );
//!
//!     // 3. Use collector as EventListener in your Flow
//!     // Pass collector to Flow when creating it
//!
//!     // 4. Query traces later
//!     let trace = storage.get_trace("workflow-id").await?;
//!     println!("Trace: {:?}", trace);
//!
//!     Ok(())
//! }
//! ```

pub mod collector;
pub mod format;
pub mod storage;
pub mod types;

// Re-exports for convenience
pub use collector::{StorageErrorPolicy, TraceCollector, TraceConfig};
pub use format::{
  export_trace_json, export_trace_json_compact, format_trace_human_readable,
  format_trace_summary,
};
pub use storage::{TraceQuery, TraceStorage, TimeRange};
pub use types::*;
