//! AgentFlow Core - V2
//!
//! This crate provides the fundamental building blocks for the V2 AgentFlow architecture.

// Core abstractions.
//
// The execution IR (`async_node` / `node` / `expr` / `error`) moved to the
// `agentflow-graph` crate (P-A1.3, IR ≠ executor per RFC §5). Re-export each
// under its original `agentflow_core::<module>` path so every existing
// `crate::async_node::AsyncNode` / `agentflow_core::AgentFlowError` consumer —
// inside core and downstream — keeps compiling unchanged. The `Flow` orchestrator
// + scheduler stay here for now (sub-step 2 moves the `Flow` *type* to graph).
pub use agentflow_graph::{async_node, error, expr, node};
pub mod error_context;
pub mod flow;

// `FlowValue` lives in the `agentflow-value` leaf crate (P-A1.5); also re-exported
// transitively by `agentflow-graph`. Surface it under the original
// `agentflow_core::value` module path + crate root for backward compatibility.
pub use agentflow_value as value;

// Execution engine
pub mod concurrency;
pub mod health;
pub mod retry;
pub mod retry_executor;
pub mod timeout;

// Reliability
pub mod checkpoint;
pub mod resource_limits;
pub mod resource_manager;
pub mod resume;
pub mod scheduler;
pub mod state_monitor;

// `state_size` (StateSizeObserver) and `events` (EventListener / WorkflowEvent)
// moved to `agentflow-graph` (P-A1.3 step 2): they are the observability
// *contracts* a `Flow` holds, so the IR crate must own them. Re-exported here
// under their original `agentflow_core::*` paths. The event drain/dispatch
// *logic* (where it exists) stays in core.
pub use agentflow_graph::{events, state_size};

// Q5.3: shared SIGINT/SIGTERM shutdown handling used by the CLI,
// server, and worker binaries.
pub mod shutdown;

// Plugin runtime (subprocess-based; gated behind the `plugin` feature)
#[cfg(feature = "plugin")]
pub mod plugin;

// Core traits and types
pub use async_node::AsyncNode;
pub use checkpoint::{Checkpoint, CheckpointConfig, CheckpointManager, WorkflowStatus};
pub use concurrency::{ConcurrencyConfig, ConcurrencyLimiter, ConcurrencyStats};
pub use error::{AgentFlowError, Result};
pub use error_context::{ErrorContext, ErrorInfo};
pub use events::{ConsoleListener, EventListener, MultiListener, NoOpListener, WorkflowEvent};
pub use flow::Flow;
pub use health::{HealthChecker, HealthReport, HealthStatus};
pub use node::Node;
pub use resource_limits::ResourceLimits;
pub use resource_manager::{CombinedResourceStats, ResourceManager, ResourceManagerConfig};
pub use resume::{
  RESUME_PLAN_SCHEMA_VERSION, ResumeDecision, ResumeIdempotency, ResumePlan, ResumePlanOptions,
  ResumeSummary, ResumeToolCall, build_resume_plan,
};
pub use retry::{ErrorPattern, RetryContext, RetryPolicy, RetryStrategy};
pub use retry_executor::{execute_with_retry, execute_with_retry_and_context};
pub use scheduler::{FlowCancellationToken, FlowExecutionConfig, FlowExecutionMode};
pub use state_monitor::{ResourceAlert, ResourceStats, StateMonitor};
pub use state_size::{StateSizeObserver, estimated_state_pool_bytes};
pub use value::FlowValue;
