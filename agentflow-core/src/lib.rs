//! AgentFlow Core - V2
//!
//! This crate provides the fundamental building blocks for the V2 AgentFlow architecture.

// Core abstractions
pub mod async_node;
pub mod error;
pub mod error_context;
pub mod flow;
pub mod node;
pub mod value;

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
pub mod state_monitor;

// Observability (lightweight events only)
pub mod events;

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
pub use retry::{ErrorPattern, RetryContext, RetryPolicy, RetryStrategy};
pub use retry_executor::{execute_with_retry, execute_with_retry_and_context};
pub use state_monitor::{ResourceAlert, ResourceStats, StateMonitor};
pub use value::FlowValue;
