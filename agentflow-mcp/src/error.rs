//! Error types for AgentFlow MCP integration

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MCPError {
    #[error("Transport error: {message}")]
    Transport { message: String },

    #[error("Protocol error: {message}")]
    Protocol { message: String },

    #[error("Tool execution error: {message}")]
    ToolExecution { message: String },

    #[error("Server connection error: {message}")]
    Connection { message: String },

    #[error("Serialization error: {source}")]
    Serialization {
        #[from]
        source: serde_json::Error,
    },

    #[error("IO error: {source}")]
    Io {
        #[from]
        source: std::io::Error,
    },

    #[error("Other error: {message}")]
    Other { message: String },
}

impl From<String> for MCPError {
    fn from(message: String) -> Self {
        MCPError::Other { message }
    }
}

impl From<&str> for MCPError {
    fn from(message: &str) -> Self {
        MCPError::Other {
            message: message.to_string(),
        }
    }
}

pub type MCPResult<T> = Result<T, MCPError>;