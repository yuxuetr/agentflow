//! Paper Research Analyzer Agent
//!
//! A comprehensive PDF research paper analysis system using Yanshi.

pub mod analyzer;
pub mod config;
pub mod nodes;

pub use analyzer::*;
pub use config::*;

// Re-export for convenience
pub use yanshi_agents::{AgentApplication, FileAgent, AgentResult};