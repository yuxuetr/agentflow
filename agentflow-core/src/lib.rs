// Core AgentFlow library - placeholder for implementation
// This file will be filled in after tests are written

pub mod shared_state;
pub mod node;
pub mod flow;
pub mod error;

pub use shared_state::SharedState;
pub use node::{Node, BaseNode};
pub use flow::Flow;
pub use error::{AgentFlowError, Result};
