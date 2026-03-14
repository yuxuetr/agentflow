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
    pub tools: Vec<ToolConfig>,
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
        self.embedding_model
            .as_deref()
            .unwrap_or("text-embedding-3-small")
    }
}
