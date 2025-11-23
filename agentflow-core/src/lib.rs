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
pub mod retry;
pub mod retry_executor;
pub mod concurrency;
pub mod timeout;

// Reliability
pub mod checkpoint;
pub mod resource_limits;
pub mod resource_manager;
pub mod state_monitor;

// Observability (lightweight events only)
pub mod events;

// Core traits and types
pub use error::{AgentFlowError, Result};
pub use error_context::{ErrorContext, ErrorInfo};
pub use flow::Flow;
pub use node::Node;
pub use async_node::AsyncNode;
pub use value::FlowValue;
pub use retry::{RetryPolicy, RetryStrategy, RetryContext, ErrorPattern};
pub use retry_executor::{execute_with_retry, execute_with_retry_and_context};
pub use resource_limits::ResourceLimits;
pub use state_monitor::{StateMonitor, ResourceAlert, ResourceStats};
pub use checkpoint::{CheckpointManager, CheckpointConfig, Checkpoint, WorkflowStatus};
pub use concurrency::{ConcurrencyLimiter, ConcurrencyConfig, ConcurrencyStats};
pub use resource_manager::{ResourceManager, ResourceManagerConfig, CombinedResourceStats};
pub use events::{WorkflowEvent, EventListener, NoOpListener, ConsoleListener, MultiListener};