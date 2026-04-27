use serde::{Deserialize, Serialize};

/// Top-level structure of a `skill.toml` file.
///
/// # Example
/// ```toml
/// [skill]
/// name    = "rust_expert"
/// version = "1.0.0"
/// description = "Rust code expert focused on safety and performance"
///
/// [persona]
/// role = "You are a senior Rust engineer..."
///
/// [model]
/// name           = "gpt-4o"
/// max_iterations = 15
/// budget_tokens  = 30000
///
/// [[tools]]
/// name             = "shell"
/// allowed_commands = ["cargo", "clippy", "rustfmt"]
///
/// [[tools]]
/// name          = "file"
/// allowed_paths = ["/tmp", "./src"]
///
/// [[knowledge]]
/// path        = "./knowledge/rust-guidelines.md"
/// description = "Internal Rust coding standards"
///
/// [memory]
/// type         = "sqlite"
/// window_tokens = 8000
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
  pub skill: SkillInfo,
  pub persona: PersonaConfig,
  #[serde(default)]
  pub model: ModelConfig,
  #[serde(default)]
  pub security: SecurityConfig,
  #[serde(default)]
  pub tools: Vec<ToolConfig>,
  #[serde(default)]
  pub mcp_servers: Vec<McpServerConfig>,
  #[serde(default)]
  pub knowledge: Vec<KnowledgeConfig>,
  pub memory: Option<MemoryConfig>,
}

/// Basic identity metadata for the skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
  pub name: String,
  pub version: String,
  pub description: String,
}

/// Defines the LLM persona injected into the system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
  /// The role / instruction text for the LLM (becomes the system prompt base).
  pub role: String,
  /// Optional language hint appended to the system prompt.
  pub language: Option<String>,
}

/// Model and runtime constraints for the agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
  /// LLM model identifier. Defaults to `"gpt-4o"`.
  pub name: Option<String>,
  /// Maximum ReAct iterations. Defaults to 15.
  pub max_iterations: Option<usize>,
  /// Token budget before halting. Defaults to 50 000.
  pub budget_tokens: Option<u32>,
}

impl ModelConfig {
  pub fn resolved_model(&self) -> &str {
    self.name.as_deref().unwrap_or("gpt-4o")
  }
  pub fn resolved_max_iterations(&self) -> usize {
    self.max_iterations.unwrap_or(15)
  }
  pub fn resolved_budget_tokens(&self) -> u32 {
    self.budget_tokens.unwrap_or(50_000)
  }
}

/// Declares an MCP server the skill connects to.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerConfig {
  pub name: String,
  pub command: String,
  #[serde(default)]
  pub args: Vec<String>,
  #[serde(default)]
  pub env: std::collections::HashMap<String, String>,
  /// Timeout for MCP connect, discovery, and tool calls in seconds.
  /// Defaults to [`SecurityConfig::mcp_default_timeout_secs`].
  pub timeout_secs: Option<u64>,
  /// Maximum concurrent calls admitted for this server.
  /// Defaults to [`SecurityConfig::mcp_max_concurrent_calls`].
  pub max_concurrent_calls: Option<usize>,
}

impl McpServerConfig {
  pub fn resolved_timeout(&self) -> std::time::Duration {
    std::time::Duration::from_secs(self.resolved_timeout_secs())
  }

  pub fn resolved_timeout_secs(&self) -> u64 {
    self
      .timeout_secs
      .unwrap_or(DEFAULT_MCP_TIMEOUT_SECS)
      .clamp(1, MAX_MCP_TIMEOUT_SECS)
  }

  pub fn resolved_max_concurrent_calls(&self) -> usize {
    self
      .max_concurrent_calls
      .unwrap_or(DEFAULT_MCP_MAX_CONCURRENT_CALLS)
      .clamp(1, MAX_MCP_MAX_CONCURRENT_CALLS)
  }
}

pub const DEFAULT_MCP_TIMEOUT_SECS: u64 = 30;
pub const MAX_MCP_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_MCP_MAX_CONCURRENT_CALLS: usize = 4;
pub const MAX_MCP_MAX_CONCURRENT_CALLS: usize = 32;
pub const DEFAULT_MCP_MAX_SERVERS: usize = 4;
pub const MAX_MCP_MAX_SERVERS: usize = 32;

/// Skill-level governance controls for tool and MCP execution.
///
/// Empty allowlists mean "use AgentFlow's default policy"; they do not mean
/// unrestricted execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
  /// Allowed MCP server names. Empty = allow all declared server names.
  #[serde(default)]
  pub mcp_server_allowlist: Vec<String>,
  /// Allowed executable names for MCP stdio servers.
  #[serde(default = "default_mcp_command_allowlist")]
  pub mcp_command_allowlist: Vec<String>,
  /// Allowed environment variable names forwarded to MCP servers.
  #[serde(default)]
  pub mcp_env_allowlist: Vec<String>,
  /// Default MCP connect/discovery/call timeout in seconds.
  #[serde(default = "default_mcp_timeout_secs")]
  pub mcp_default_timeout_secs: u64,
  /// Default max concurrent tool calls admitted per MCP server.
  #[serde(default = "default_mcp_max_concurrent_calls")]
  pub mcp_max_concurrent_calls: usize,
  /// Maximum MCP server declarations in one skill.
  #[serde(default = "default_mcp_max_servers")]
  pub mcp_max_servers: usize,
}

impl Default for SecurityConfig {
  fn default() -> Self {
    Self {
      mcp_server_allowlist: Vec::new(),
      mcp_command_allowlist: default_mcp_command_allowlist(),
      mcp_env_allowlist: Vec::new(),
      mcp_default_timeout_secs: DEFAULT_MCP_TIMEOUT_SECS,
      mcp_max_concurrent_calls: DEFAULT_MCP_MAX_CONCURRENT_CALLS,
      mcp_max_servers: DEFAULT_MCP_MAX_SERVERS,
    }
  }
}

impl SecurityConfig {
  pub fn resolved_mcp_command_allowlist(&self) -> Vec<String> {
    if self.mcp_command_allowlist.is_empty() {
      default_mcp_command_allowlist()
    } else {
      self.mcp_command_allowlist.clone()
    }
  }

  pub fn resolved_mcp_default_timeout_secs(&self) -> u64 {
    self.mcp_default_timeout_secs.clamp(1, MAX_MCP_TIMEOUT_SECS)
  }

  pub fn resolved_mcp_max_concurrent_calls(&self) -> usize {
    self
      .mcp_max_concurrent_calls
      .clamp(1, MAX_MCP_MAX_CONCURRENT_CALLS)
  }

  pub fn resolved_mcp_max_servers(&self) -> usize {
    self.mcp_max_servers.clamp(1, MAX_MCP_MAX_SERVERS)
  }
}

fn default_mcp_timeout_secs() -> u64 {
  DEFAULT_MCP_TIMEOUT_SECS
}

fn default_mcp_max_concurrent_calls() -> usize {
  DEFAULT_MCP_MAX_CONCURRENT_CALLS
}

fn default_mcp_max_servers() -> usize {
  DEFAULT_MCP_MAX_SERVERS
}

pub fn default_mcp_command_allowlist() -> Vec<String> {
  ["python", "python3", "node", "npx", "uvx"]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

/// Declares a tool the skill is authorised to use, with optional constraints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolConfig {
  /// Tool name — one of `"shell"`, `"file"`, `"http"`, `"script"`.
  pub name: String,

  // ── shell constraints ────────────────────────────────────────────────────
  /// Allowed shell commands (first token). Empty = use the default safe list.
  #[serde(default)]
  pub allowed_commands: Vec<String>,

  // ── file constraints ─────────────────────────────────────────────────────
  /// Allowed filesystem path prefixes. Empty = all paths allowed.
  #[serde(default)]
  pub allowed_paths: Vec<String>,

  // ── http constraints ─────────────────────────────────────────────────────
  /// Allowed host suffixes for HTTP requests. Empty = all domains allowed.
  #[serde(default)]
  pub allowed_domains: Vec<String>,

  /// JSON schema for validating input parameters to the tool
  #[serde(default)]
  pub parameters: Option<serde_json::Value>,

  /// Override the sandbox exec-time limit (seconds).
  pub max_exec_time_secs: Option<u64>,
}

/// A knowledge file (markdown, txt, …) loaded into the agent's context.
///
/// For Phase 2 the content is injected directly into the system prompt.
/// In Phase 3 it will be indexed into a vector store for semantic retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
  /// Path to the file, relative to the skill directory. Glob patterns supported.
  pub path: String,
  /// Human-readable label shown in the system prompt header.
  pub description: Option<String>,
}

/// Configures the memory backend for the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
  /// `"session"` (in-memory, lost on exit) | `"sqlite"` (persistent) |
  /// `"semantic"` (SQLite + embedding vectors) | `"none"`.
  #[serde(rename = "type")]
  pub memory_type: String,
  /// Path to the SQLite database file. Supports `~` expansion.
  /// Defaults to `~/.agentflow/memory/<skill_name>.db`.
  pub db_path: Option<String>,
  /// Maximum tokens to keep in the sliding window. Defaults to 8 000.
  pub window_tokens: Option<u32>,
  /// OpenAI embedding model used by `"semantic"` memory.
  /// Supported: `"text-embedding-3-small"` (default), `"text-embedding-3-large"`,
  /// `"text-embedding-ada-002"`.
  pub embedding_model: Option<String>,
}

impl MemoryConfig {
  pub fn resolved_window_tokens(&self) -> u32 {
    self.window_tokens.unwrap_or(8_000)
  }

  pub fn resolved_embedding_model(&self) -> &str {
    self
      .embedding_model
      .as_deref()
      .unwrap_or("text-embedding-3-small")
  }
}
