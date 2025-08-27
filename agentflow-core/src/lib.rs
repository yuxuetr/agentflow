//! AgentFlow Core - Pure Code-First Workflow Engine
//!
//! This crate provides the fundamental building blocks for creating
//! AI workflows programmatically. For configuration-based workflows,
//! see the `agentflow-config` crate.

pub mod error;
pub mod flow;
pub mod node;
pub mod shared_state;

// Async workflow engine modules
pub mod async_flow;
pub mod async_node;
pub mod observability;
pub mod robustness;

// Core traits and types - the foundation everything else builds on
pub use error::{AgentFlowError, Result};
pub use flow::Flow;
pub use node::{BaseNode, Node};
pub use shared_state::SharedState;

// Async workflow exports
pub use async_flow::AsyncFlow;
pub use async_node::{AsyncBaseNode, AsyncNode};
pub use observability::{AlertManager, ExecutionEvent, MetricsCollector};
pub use robustness::{CircuitBreaker, RateLimiter, TimeoutManager};

// Core result type
pub type CoreResult<T> = std::result::Result<T, AgentFlowError>;
