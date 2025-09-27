//! AgentFlow Core - V2
//!
//! This crate provides the fundamental building blocks for the V2 AgentFlow architecture.

pub mod async_node;
pub mod error;
pub mod flow;
pub mod node;
pub mod value;
pub mod observability;

// Core traits and types
pub use error::{AgentFlowError, Result};
pub use flow::Flow;
pub use node::Node;
pub use async_node::AsyncNode;
pub use value::FlowValue;