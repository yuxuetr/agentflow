//! Transport layer implementations for MCP communication

use crate::error::{MCPError, MCPResult};
use serde_json::Value;
use tokio::process::{Child, Command as TokioCommand};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Transport mechanisms for MCP communication
pub enum Transport {
    /// Standard I/O transport for local processes
    Stdio {
        command: Vec<String>,
    },
    /// HTTP transport for remote servers  
    Http {
        base_url: String,
    },
}

impl Transport {
    /// Create a new stdio transport
    pub fn stdio(command: Vec<String>) -> Self {
        Transport::Stdio { command }
    }

    /// Create a new HTTP transport
    pub fn http<S: Into<String>>(base_url: S) -> Self {
        Transport::Http {
            base_url: base_url.into(),
        }
    }
}

/// MCP transport client for communicating with servers
pub struct TransportClient {
    transport: Transport,
    process: Option<Child>,
}

impl TransportClient {
    /// Create a new transport client
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            process: None,
        }
    }

    /// Connect to the MCP server
    pub async fn connect(&mut self) -> MCPResult<()> {
        match &self.transport {
            Transport::Stdio { command } => {
                if command.is_empty() {
                    return Err(MCPError::Transport {
                        message: "Empty command for stdio transport".to_string(),
                    });
                }

                let mut cmd = TokioCommand::new(&command[0]);
                if command.len() > 1 {
                    cmd.args(&command[1..]);
                }

                let child = cmd
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| MCPError::Connection {
                        message: format!("Failed to spawn MCP server process: {}", e),
                    })?;

                self.process = Some(child);
                Ok(())
            }
            Transport::Http { .. } => {
                // HTTP transport implementation would go here
                todo!("HTTP transport not yet implemented")
            }
        }
    }

    /// Send a message to the server and receive response
    pub async fn send_message(&mut self, message: Value) -> MCPResult<Value> {
        match &mut self.process {
            Some(child) => {
                // Write JSON-RPC message to stdin
                let message_str = serde_json::to_string(&message)?;
                let message_bytes = format!("{}\n", message_str);

                if let Some(stdin) = child.stdin.as_mut() {
                    stdin
                        .write_all(message_bytes.as_bytes())
                        .await
                        .map_err(|e| MCPError::Transport {
                            message: format!("Failed to write to MCP server: {}", e),
                        })?;
                    stdin.flush().await.map_err(|e| MCPError::Transport {
                        message: format!("Failed to flush MCP server stdin: {}", e),
                    })?;
                }

                // Read response from stdout
                if let Some(stdout) = child.stdout.as_mut() {
                    let mut buffer = Vec::new();
                    let mut byte = [0u8; 1];
                    
                    // Read until newline
                    loop {
                        match stdout.read_exact(&mut byte).await {
                            Ok(_) => {
                                if byte[0] == b'\n' {
                                    break;
                                }
                                buffer.push(byte[0]);
                            }
                            Err(e) => {
                                return Err(MCPError::Transport {
                                    message: format!("Failed to read from MCP server: {}", e),
                                });
                            }
                        }
                    }

                    let response_str = String::from_utf8(buffer).map_err(|e| {
                        MCPError::Transport {
                            message: format!("Invalid UTF-8 response from MCP server: {}", e),
                        }
                    })?;

                    let response: Value = serde_json::from_str(&response_str)?;
                    Ok(response)
                } else {
                    Err(MCPError::Transport {
                        message: "No stdout available from MCP server".to_string(),
                    })
                }
            }
            None => Err(MCPError::Connection {
                message: "Not connected to MCP server".to_string(),
            }),
        }
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self) -> MCPResult<()> {
        if let Some(mut child) = self.process.take() {
            child.kill().await.map_err(|e| MCPError::Connection {
                message: format!("Failed to terminate MCP server process: {}", e),
            })?;
        }
        Ok(())
    }
}

impl Drop for TransportClient {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = futures::executor::block_on(child.kill());
        }
    }
}