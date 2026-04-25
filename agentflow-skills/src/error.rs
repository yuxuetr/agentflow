use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
  #[error("Skill manifest not found at path: {path}")]
  ManifestNotFound { path: String },

  #[error("Failed to read skill manifest: {0}")]
  ReadError(#[from] std::io::Error),

  #[error("Failed to parse skill.toml: {0}")]
  TomlError(#[from] toml::de::Error),

  /// General parse error (used for SKILL.md frontmatter / validation).
  #[error("Parse error: {message}")]
  ParseError { message: String },

  #[error("Invalid skill configuration: {message}")]
  ValidationError { message: String },

  #[error("Unknown tool '{name}' — available: shell, file, http, script")]
  UnknownTool { name: String },

  #[error("Knowledge file not found: {path}")]
  KnowledgeFileNotFound { path: String },

  #[error("Memory error: {0}")]
  MemoryError(#[from] agentflow_memory::MemoryError),

  #[error("MCP error: {0}")]
  McpError(String),

  #[error("IO error: {0}")]
  IoError(String),
}
