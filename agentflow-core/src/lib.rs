//! AgentFlow Core - V2
//!
//! This crate provides the fundamental building blocks for the V2 AgentFlow architecture.

pub mod async_node;
pub mod error;
pub mod error_context;
pub mod flow;
pub mod node;
pub mod value;
pub mod observability;
pub mod retry;
pub mod retry_executor;

// Core traits and types
pub use error::{AgentFlowError, Result};
pub use error_context::{ErrorContext, ErrorInfo};
pub use flow::Flow;
pub use node::Node;
pub use async_node::AsyncNode;
pub use value::FlowValue;
pub use retry::{RetryPolicy, RetryStrategy, RetryContext, ErrorPattern};
pub use retry_executor::{execute_with_retry, execute_with_retry_and_context};