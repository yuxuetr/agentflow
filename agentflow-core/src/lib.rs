// Core AgentFlow library - Phase 2: Async Concurrency Framework

pub mod error;
pub mod flow;
pub mod node;
pub mod shared_state;

// Phase 2: Async modules
pub mod async_flow;
pub mod async_node;
pub mod observability;
pub mod robustness;

// Configuration-first modules
pub mod config;
pub mod nodes;
pub mod workflow_runner;

pub use error::{AgentFlowError, Result};
pub use flow::Flow;
pub use node::{BaseNode, Node};
pub use shared_state::SharedState;

// Phase 2 exports
pub use async_flow::AsyncFlow;
pub use async_node::{AsyncBaseNode, AsyncNode};
pub use observability::{AlertManager, ExecutionEvent, MetricsCollector};
pub use robustness::{CircuitBreaker, RateLimiter, TimeoutManager};

// Configuration-first exports
pub use config::WorkflowConfig;
pub use workflow_runner::ConfigWorkflowRunner;
