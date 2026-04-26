//! Tools that wrap agents, allowing one agent to call another.

pub mod agent_tool;
pub mod workflow_tool;

pub use agent_tool::AgentTool;
pub use workflow_tool::WorkflowTool;
