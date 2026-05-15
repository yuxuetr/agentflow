use clap::{Args, Parser, Subcommand};

mod commands;
use agentflow_cli::{config, executor, redaction};

#[cfg(feature = "plugin")]
use commands::plugin;
#[cfg(feature = "rag")]
use commands::rag;
use commands::{
  audio, cleanup as cleanup_cmd, config as config_cmd, doctor, harness, image, llm, marketplace,
  mcp, serve as serve_cmd, skill, trace, workflow,
};

#[derive(Parser)]
#[command(name = "agentflow", version, about = "AgentFlow V2 CLI")]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Workflow orchestration commands
  Workflow(WorkflowArgs),
  /// Audio generation and transcription commands
  Audio(AudioArgs),
  /// Configuration management commands
  Config(ConfigArgs),
  /// Image generation and understanding commands
  Image(ImageArgs),
  /// LLM model discovery commands
  Llm(LlmArgs),
  /// Model Context Protocol (MCP) commands
  Mcp(McpArgs),
  /// Skill management commands
  Skill(SkillArgs),
  /// Remote Skill / Plugin marketplace commands
  Marketplace(RemoteMarketplaceArgs),
  /// Trace inspection and replay commands
  Trace(TraceArgs),
  /// Diagnose local AgentFlow configuration and runtime capabilities
  Doctor(DoctorArgs),
  /// Harness Agent Mode: workspace-aware, long-lived agent sessions
  Harness(HarnessArgs),
  /// Boot the AgentFlow Gateway (Axum HTTP API) by spawning `agentflow-server`
  Serve(ServeArgs),
  /// Run the retention sweep once and exit (delegates to `agentflow-server --cleanup`)
  Cleanup(CleanupArgs),
  #[cfg(feature = "plugin")]
  /// Plugin management commands (subprocess plugins)
  Plugin(PluginArgs),
  #[cfg(not(feature = "plugin"))]
  /// Plugin management commands (disabled in this build)
  Plugin(FeatureUnavailableArgs),
  #[cfg(feature = "rag")]
  /// RAG (Retrieval-Augmented Generation) commands
  Rag(RagArgs),
  #[cfg(not(feature = "rag"))]
  /// RAG commands (disabled in this build)
  Rag(FeatureUnavailableArgs),
}

#[derive(Args)]
struct WorkflowArgs {
  #[command(subcommand)]
  command: WorkflowCommands,
}
#[derive(Args)]
struct AudioArgs {
  #[command(subcommand)]
  command: AudioCommands,
}
#[derive(Args)]
struct ConfigArgs {
  #[command(subcommand)]
  command: ConfigCommands,
}
#[derive(Args)]
struct ImageArgs {
  #[command(subcommand)]
  command: ImageCommands,
}
#[derive(Args)]
struct LlmArgs {
  #[command(subcommand)]
  command: LlmCommands,
}
#[derive(Args)]
struct McpArgs {
  #[command(subcommand)]
  command: McpCommands,
}
#[derive(Args)]
struct SkillArgs {
  #[command(subcommand)]
  command: SkillCommands,
}
#[derive(Args)]
struct RemoteMarketplaceArgs {
  #[command(subcommand)]
  command: RemoteMarketplaceCommands,
}
#[derive(Args)]
struct TraceArgs {
  #[command(subcommand)]
  command: TraceCommands,
}
#[derive(Args)]
struct DoctorArgs {
  /// Output format
  #[arg(long, default_value = "text", value_parser = ["text", "json"])]
  format: String,
  /// Pass/fail threshold profile
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  profile: String,
  /// When supplied, also probe `<url>/health` for server reachability
  #[arg(long)]
  server: Option<String>,
}
#[derive(Args)]
struct HarnessArgs {
  #[command(subcommand)]
  command: HarnessCommands,
}

#[derive(Args)]
struct CleanupArgs {
  /// Postgres URL (default env: DATABASE_URL)
  #[arg(long)]
  database_url: Option<String>,
  /// Workflow run-artifact root (env: AGENTFLOW_RUN_DIR)
  #[arg(long)]
  run_dir: Option<String>,
  /// Trace directory (env: AGENTFLOW_TRACE_DIR)
  #[arg(long)]
  trace_dir: Option<String>,
  /// Active security profile (drives retention defaults)
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  security_profile: String,
  /// Preview targets without deleting anything
  #[arg(long)]
  dry_run: bool,
}

#[derive(Args)]
struct ServeArgs {
  /// `host:port` to bind to (default: 127.0.0.1:8080, env: AGENTFLOW_SERVE_BIND)
  #[arg(long)]
  bind: Option<String>,
  /// Postgres URL (default env: DATABASE_URL)
  #[arg(long)]
  database_url: Option<String>,
  /// Workflow run-artifact root (env: AGENTFLOW_RUN_DIR)
  #[arg(long)]
  run_dir: Option<String>,
  /// Trace directory (env: AGENTFLOW_TRACE_DIR)
  #[arg(long)]
  trace_dir: Option<String>,
  /// Active security profile
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  security_profile: String,
  /// Name of the env var that carries the bearer auth token
  #[arg(long, default_value = "AGENTFLOW_API_TOKEN")]
  auth_token_env: String,
  /// Explicit CORS allow-list (comma-separated)
  #[arg(long, value_delimiter = ',')]
  cors_origins: Vec<String>,
  /// Maximum request body size in megabytes
  #[arg(long)]
  max_body_mb: Option<u64>,
  /// Run readiness diagnostics without binding any sockets and exit
  #[arg(long)]
  check: bool,
}

#[derive(Subcommand)]
enum HarnessCommands {
  /// Run a Harness session and stream events to disk (+ optional stdout)
  Run {
    /// User input that opens the session
    input: String,
    /// Path to a skill directory to load (optional)
    #[arg(long)]
    skill: Option<String>,
    /// Model id (required when no --skill is supplied)
    #[arg(long)]
    model: Option<String>,
    /// Resume an existing session id rather than generating a fresh one
    #[arg(long)]
    session: Option<String>,
    /// Workspace root (default: current working directory)
    #[arg(long)]
    workspace: Option<String>,
    /// Security profile
    #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
    profile: String,
    /// Underlying agent runtime
    #[arg(long, default_value = "react", value_parser = ["react", "plan_execute", "plan-execute", "handoff", "blackboard", "debate"])]
    runtime: String,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json"])]
    output: String,
    /// Override the run-dir (session log root). Defaults to AGENTFLOW_RUN_DIR or ~/.agentflow/runs
    #[arg(long)]
    run_dir: Option<String>,
    /// Maximum total agent steps
    #[arg(long)]
    max_steps: Option<usize>,
    /// Maximum total tool calls
    #[arg(long)]
    max_tool_calls: Option<usize>,
    /// Wall-clock timeout in milliseconds
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// Skip the default workspace context providers (AGENTS.md / TODOs.md / ...)
    #[arg(long)]
    no_default_context: bool,
  },
  /// Replay a persisted Harness session log (no LLM is invoked).
  Resume {
    /// Session id to replay
    session_id: String,
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json"])]
    output: String,
  },
  /// List persisted Harness session logs
  List {
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json"])]
    output: String,
  },
  /// Inspect a single persisted Harness session log
  Inspect {
    /// Session id to inspect
    session_id: String,
    /// Override the run-dir
    #[arg(long)]
    run_dir: Option<String>,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json", "stream-json"])]
    output: String,
  },
}
#[cfg(feature = "plugin")]
#[derive(Args)]
struct PluginArgs {
  #[command(subcommand)]
  command: PluginCommands,
}
#[cfg(any(not(feature = "plugin"), not(feature = "rag")))]
#[derive(Args)]
#[command(
  after_help = "This command is not available in this binary. Rebuild with the matching Cargo feature, e.g. `cargo build -p agentflow-cli --features rag` or `--features plugin`."
)]
struct FeatureUnavailableArgs {
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
  args: Vec<String>,
}
#[cfg(feature = "rag")]
#[derive(Args)]
struct RagArgs {
  #[command(subcommand)]
  command: RagCommands,
}

#[derive(Subcommand)]
enum WorkflowCommands {
  Run {
    workflow_file: String,
    #[arg(short, long)]
    watch: bool,
    #[arg(short, long)]
    output: Option<String>,
    /// Override the model used by LLM nodes in this workflow
    #[arg(short = 'm', long)]
    model: Option<String>,
    #[arg(short, long, num_args = 2, value_names = ["KEY", "VALUE"])]
    input: Vec<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value = "60s")]
    timeout: String,
    #[arg(long, default_value_t = 0)]
    max_retries: u32,
    /// Workflow execution mode: serial or concurrent
    #[arg(long, default_value = "serial", value_parser = ["serial", "concurrent"])]
    execution_mode: String,
    /// Maximum concurrently running workflow nodes when --execution-mode concurrent
    #[arg(long, default_value_t = 4)]
    max_concurrency: usize,
    /// Base directory for per-run workflow artifacts. Defaults to AGENTFLOW_RUN_DIR or ~/.agentflow/runs.
    #[arg(long)]
    run_dir: Option<String>,
  },
  /// Validate workflow schema and dependencies without execution
  Validate {
    workflow_file: String,
    /// Output format: text or json
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    format: String,
    /// Treat schema warnings for unknown node parameters as validation errors
    #[arg(long)]
    strict: bool,
  },
  /// Inspect the resume plan for a checkpointed workflow run
  ResumePlan {
    /// Run / workflow id whose checkpoint should be inspected
    run_id: String,
    /// Checkpoint directory (default: ~/.agentflow/checkpoints)
    #[arg(long)]
    checkpoint_dir: Option<String>,
    /// Treat `Unknown` idempotency calls as safe to replay
    #[arg(long)]
    force_replay: bool,
    /// Output format
    #[arg(long, default_value = "text", value_parser = ["text", "json"])]
    format: String,
  },
  /// Debug and inspect workflow structure
  Debug {
    workflow_file: String,
    /// Visualize the workflow DAG
    #[arg(long)]
    visualize: bool,
    /// Perform dry run without execution
    #[arg(long)]
    dry_run: bool,
    /// Analyze workflow structure and dependencies
    #[arg(long)]
    analyze: bool,
    /// Validate workflow configuration
    #[arg(long)]
    validate: bool,
    /// Show execution plan
    #[arg(long)]
    plan: bool,
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
  },
}

#[derive(Subcommand)]
enum AudioCommands {
  Asr {
    file_path: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    language: Option<String>,
    #[arg(short, long)]
    prompt: Option<String>,
    #[arg(long, default_value = "text")]
    format: String,
  },
  /// Voice cloning (experimental - not yet implemented)
  #[command(hide = true)]
  Clone {
    text: String,
    file_id: String,
    output: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(long)]
    sample_text: Option<String>,
    #[arg(long, default_value = "wav")]
    format: String,
  },
  Tts {
    input: String,
    voice: String,
    output: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(long, default_value_t = 1.0)]
    speed: f32,
    #[arg(long, default_value = "mp3")]
    format: String,
    #[arg(long)]
    emotion: Option<String>,
  },
}

#[derive(Subcommand)]
enum ConfigCommands {
  Init {
    #[arg(short, long)]
    force: bool,
  },
  Show {
    section: Option<String>,
  },
  Validate,
}

#[derive(Subcommand)]
enum ImageCommands {
  Generate {
    prompt: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long, default_value = "1024x1024")]
    size: String,
    #[arg(short, long)]
    output: String,
    #[arg(short, long, default_value = "b64_json")]
    format: String,
    #[arg(long, default_value_t = 20)]
    steps: u32,
    #[arg(long, default_value_t = 7.5)]
    cfg_scale: f32,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    strength: Option<f32>,
    #[arg(long)]
    input_image: Option<String>,
  },
  Understand {
    image_path: String,
    prompt: String,
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    temperature: Option<f32>,
    #[arg(long)]
    max_tokens: Option<u32>,
    #[arg(short, long)]
    output: Option<String>,
  },
}

#[derive(Subcommand)]
enum LlmCommands {
  Models {
    #[arg(short, long)]
    provider: Option<String>,
    #[arg(short, long)]
    detailed: bool,
  },
  /// Deprecated compatibility stub. Use `skill chat`, `skill run`, or workflow `skill_agent`.
  #[command(hide = true)]
  Chat {
    #[arg(short, long)]
    model: Option<String>,
    #[arg(short, long)]
    system: Option<String>,
    #[arg(long)]
    save: Option<String>,
    #[arg(long)]
    load: Option<String>,
  },
}

#[derive(Subcommand)]
enum McpCommands {
  /// List available tools from an MCP server
  ListTools {
    /// Server command to execute (e.g., "npx -y @modelcontextprotocol/server-filesystem /tmp")
    server_command: Vec<String>,
    #[arg(long, default_value_t = 30000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
  },
  /// Call a tool on an MCP server
  CallTool {
    /// Server command to execute
    server_command: Vec<String>,
    /// Tool name to call
    #[arg(short, long)]
    tool: String,
    /// Tool parameters as JSON string
    #[arg(short, long)]
    params: Option<String>,
    #[arg(long, default_value_t = 30000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
    /// Output file path to save the result
    #[arg(short, long)]
    output: Option<String>,
  },
  /// List available resources from an MCP server
  ListResources {
    /// Server command to execute
    server_command: Vec<String>,
    #[arg(long, default_value_t = 30000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
  },
}

#[derive(Subcommand)]
enum SkillCommands {
  /// Inspect or validate a local skill registry index
  Index(IndexArgs),
  /// Browse a local marketplace that groups skill registry indexes
  Marketplace(MarketplaceArgs),
  /// Create a new standard SKILL.md scaffold
  Init {
    /// Directory to create the skill in
    skill_dir: String,
    /// Skill name. Defaults to the target directory name.
    #[arg(long)]
    name: Option<String>,
    /// Skill description written to SKILL.md
    #[arg(short, long)]
    description: Option<String>,
    /// Overwrite scaffold files if they already exist
    #[arg(long)]
    force: bool,
  },
  /// Install a skill from a local registry index
  Install {
    /// Path to the skill registry index file
    index_file: String,
    /// Skill name or alias to install
    skill: String,
    /// Target skills directory (default: ~/.agentflow/skills)
    #[arg(short, long)]
    dir: Option<String>,
    /// Overwrite an existing installed skill directory
    #[arg(long)]
    force: bool,
  },
  /// Validate a skill manifest and print its configuration
  Validate {
    /// Path to the skill directory (must contain skill.toml or SKILL.md)
    skill_dir: String,
  },
  /// Inspect a skill manifest without running the agent
  Inspect {
    /// Path to the skill directory (must contain skill.toml or SKILL.md)
    skill_dir: String,
    /// Explain the capability decision for each declared tool
    #[arg(long = "explain-permissions")]
    explain_permissions: bool,
    /// Operator override: admit tool by name (repeatable). Beats skill manifest.
    #[arg(long = "allow-tool", value_name = "TOOL")]
    allow_tools: Vec<String>,
    /// Operator override: deny tool by name (repeatable). Highest precedence.
    #[arg(long = "deny-tool", value_name = "TOOL")]
    deny_tools: Vec<String>,
  },
  /// Run a skill with a single message and exit
  Run {
    /// Path to the skill directory
    skill_dir: String,
    /// The message to send to the agent
    #[arg(short, long)]
    message: String,
    /// Override the model declared by the skill manifest
    #[arg(long)]
    model: Option<String>,
    /// Override memory backend for this run: session, sqlite, or none
    #[arg(long, value_parser = ["session", "sqlite", "none"])]
    memory: Option<String>,
    /// Reuse an existing session ID for multi-turn conversations
    #[arg(long, visible_alias = "session-id")]
    session: Option<String>,
    /// Print the structured AgentRuntime trace as JSON
    #[arg(long)]
    trace: bool,
  },
  /// Start an interactive multi-turn chat session with a skill
  Chat {
    /// Path to the skill directory
    skill_dir: String,
    /// Override the model declared by the skill manifest
    #[arg(long)]
    model: Option<String>,
    /// Override memory backend for this chat: session, sqlite, or none
    #[arg(long, value_parser = ["session", "sqlite", "none"])]
    memory: Option<String>,
    /// Resume an existing session by ID (optional)
    #[arg(long, visible_alias = "session-id")]
    session: Option<String>,
  },
  /// List available skills in a directory
  List {
    /// Skills directory (default: ~/.agentflow/skills)
    #[arg(short, long)]
    dir: Option<String>,
  },
  /// List built-in, script, and MCP tools exposed by a skill
  ListTools {
    /// Path to the skill directory
    skill_dir: String,
  },
  /// Run skill validation, tool discovery, and minimal regression checks
  Test {
    /// Path to the skill directory
    skill_dir: String,
    /// Only validate manifest and discover tools; do not execute regressions or smoke scripts
    #[arg(long)]
    dry_run: bool,
    /// Also run tests/smoke.sh when present
    #[arg(long)]
    smoke: bool,
  },
}

#[derive(Args)]
struct IndexArgs {
  #[command(subcommand)]
  command: SkillIndexCommands,
}

#[derive(Args)]
struct MarketplaceArgs {
  #[command(subcommand)]
  command: SkillMarketplaceCommands,
}

#[derive(Subcommand)]
enum SkillIndexCommands {
  Validate {
    /// Path to the skill registry index file
    index_file: String,
  },
  List {
    /// Path to the skill registry index file
    index_file: String,
  },
  Resolve {
    /// Path to the skill registry index file
    index_file: String,
    /// Skill name or alias to resolve
    skill: String,
  },
}

#[derive(Subcommand)]
enum SkillMarketplaceCommands {
  Validate {
    /// Path to the skill marketplace file
    marketplace_file: String,
  },
  List {
    /// Path to the skill marketplace file
    marketplace_file: String,
  },
  Resolve {
    /// Path to the skill marketplace file
    marketplace_file: String,
    /// Skill name or alias to resolve
    skill: String,
  },
  Install {
    /// Path to the skill marketplace file
    marketplace_file: String,
    /// Skill name or alias to install
    skill: String,
    /// Target skills directory (default: ~/.agentflow/skills)
    #[arg(short, long)]
    dir: Option<String>,
    /// Overwrite an existing installed skill directory
    #[arg(long)]
    force: bool,
  },
}

#[derive(Subcommand)]
enum RemoteMarketplaceCommands {
  /// Search a remote marketplace manifest for Skills or Plugins
  Search {
    /// HTTP(S) registry URL or local remote marketplace TOML file
    registry: String,
    /// Optional text query matched against name, aliases, and description
    query: Option<String>,
    /// Restrict results to one package type
    #[arg(long = "type", value_parser = ["skill", "plugin"])]
    package_type: Option<String>,
  },
  /// Download and cache a verified package artifact
  Install {
    /// HTTP(S) registry URL or local remote marketplace TOML file
    registry: String,
    /// Package name or alias
    package: String,
    /// Disambiguate when the same name exists as both a Skill and Plugin
    #[arg(long = "type", value_parser = ["skill", "plugin"])]
    package_type: Option<String>,
    /// Cache directory (default: ~/.agentflow/marketplace/cache)
    #[arg(long)]
    cache_dir: Option<String>,
    /// Target install root. Defaults to ~/.agentflow/skills for Skills and ~/.agentflow/plugins for Plugins.
    #[arg(long = "dir")]
    install_dir: Option<String>,
    /// Overwrite an existing installed package directory
    #[arg(long)]
    force: bool,
    /// Only download/verify/cache the artifact; do not unpack it into the runtime install directory.
    #[arg(long)]
    cache_only: bool,
  },
  /// Fetch and cache the registry manifest itself
  Update {
    /// HTTP(S) registry URL or local remote marketplace TOML file
    registry: String,
    /// Cache directory (default: ~/.agentflow/marketplace/cache)
    #[arg(long)]
    cache_dir: Option<String>,
  },
  /// Verify cached package artifacts against marketplace checksums/signatures
  Verify {
    /// HTTP(S) registry URL or local remote marketplace TOML file
    registry: String,
    /// Optional package name or alias. When omitted, verifies all matching entries.
    package: Option<String>,
    /// Restrict verification to one package type
    #[arg(long = "type", value_parser = ["skill", "plugin"])]
    package_type: Option<String>,
    /// Cache directory (default: ~/.agentflow/marketplace/cache)
    #[arg(long)]
    cache_dir: Option<String>,
    /// Require each verified artifact to include and pass signature metadata
    #[arg(long)]
    strict: bool,
  },
}

#[derive(Subcommand)]
enum TraceCommands {
  /// Replay a persisted workflow/agent trace without re-executing tools or LLMs
  Replay {
    /// Workflow run ID / trace ID to replay
    run_id: String,
    /// Directory containing file-backed traces (default: AGENTFLOW_TRACE_DIR or ~/.agentflow/traces)
    #[arg(long)]
    dir: Option<String>,
    /// Include raw trace JSON after the replay timeline
    #[arg(long)]
    json: bool,
    /// Maximum characters printed for prompt, response, params, and output fields
    #[arg(long, default_value_t = 160)]
    max_field_chars: usize,
  },
  /// Inspect a persisted trace as a focused terminal timeline
  Tui {
    /// Workflow run ID / trace ID to inspect
    run_id: String,
    /// Directory containing file-backed traces (default: AGENTFLOW_TRACE_DIR or ~/.agentflow/traces)
    #[arg(long)]
    dir: Option<String>,
    /// Timeline focus: all, workflow, agent, tool, or mcp
    #[arg(long, default_value = "all")]
    filter: trace::tui::CliTraceTuiFilter,
    /// Expand matching timeline rows with captured fields
    #[arg(long)]
    details: bool,
    /// Maximum characters printed for params, steps, input, and output fields
    #[arg(long, default_value_t = 120)]
    max_field_chars: usize,
  },
}

#[cfg(feature = "plugin")]
#[derive(Subcommand)]
enum PluginCommands {
  /// Install a plugin from a local source directory containing plugin.toml
  Install {
    /// Path to the plugin source directory
    source_dir: String,
    /// Target plugins directory (default: ~/.agentflow/plugins)
    #[arg(short, long)]
    dir: Option<String>,
    /// Overwrite an existing installed plugin directory
    #[arg(long)]
    force: bool,
    /// Opt out of sandbox requirement (`local` profile only;
    /// `production` always refuses this flag).
    #[arg(long)]
    allow_unsandboxed_plugin: bool,
    /// Treat the plugin archive as signature-verified (`production`
    /// profile requires this).
    #[arg(long)]
    signed: bool,
  },
  /// List installed plugins and the node types each one declares
  List {
    /// Plugins directory (default: ~/.agentflow/plugins)
    #[arg(short, long)]
    dir: Option<String>,
  },
  /// Inspect a plugin manifest without spawning the subprocess
  Inspect {
    /// Path to a plugin directory or its plugin.toml file
    plugin: String,
  },
  /// Remove an installed plugin
  Uninstall {
    /// Plugin name (matches the directory name under the plugins dir)
    name: String,
    /// Plugins directory (default: ~/.agentflow/plugins)
    #[arg(short, long)]
    dir: Option<String>,
    /// Succeed even if the plugin is not installed
    #[arg(long)]
    force: bool,
  },
}

#[cfg(feature = "rag")]
#[derive(Subcommand)]
enum RagCommands {
  /// Search documents in a RAG collection
  Search {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Collection name
    #[arg(short, long)]
    collection: String,
    /// Search query
    #[arg(short, long)]
    query: String,
    /// Number of results to return
    #[arg(short = 'k', long, default_value_t = 5)]
    top_k: usize,
    /// Search type: semantic, hybrid, or keyword
    #[arg(short = 't', long, default_value = "semantic")]
    search_type: String,
    /// Alpha for hybrid search (0.0=keyword, 1.0=semantic)
    #[arg(short, long, default_value_t = 0.5)]
    alpha: f32,
    /// Enable MMR re-ranking for diversity
    #[arg(long)]
    rerank: bool,
    /// Lambda for MMR (0.0=diversity, 1.0=relevance)
    #[arg(short, long, default_value_t = 0.5)]
    lambda: f32,
    /// OpenAI embedding model
    #[arg(short = 'm', long, default_value = "text-embedding-3-small")]
    embedding_model: String,
    /// Output file path to save results as JSON
    #[arg(short, long)]
    output: Option<String>,
  },
  /// Index documents into a RAG collection
  Index {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Collection name
    #[arg(short, long)]
    collection: String,
    /// Documents as JSON array: [{"content": "...", "metadata": {...}}]
    #[arg(short, long)]
    documents: String,
    /// OpenAI embedding model
    #[arg(short = 'm', long, default_value = "text-embedding-3-small")]
    embedding_model: String,
  },
  /// Manage RAG collections (create, delete, list, stats)
  Collections {
    /// Qdrant URL
    #[arg(long, default_value = "http://localhost:6334")]
    qdrant_url: String,
    /// Operation: create, delete, list, stats
    #[arg(short, long)]
    operation: String,
    /// Collection name (required for create, delete, stats)
    #[arg(short, long)]
    collection: Option<String>,
    /// Vector size (for create operation)
    #[arg(short = 's', long)]
    vector_size: Option<usize>,
    /// Distance metric: cosine, euclidean, dot (for create operation)
    #[arg(short, long)]
    distance: Option<String>,
  },
  /// Evaluate a retriever against a labeled dataset
  Eval {
    /// Dataset directory (must contain corpus.jsonl, queries.jsonl, qrels.jsonl)
    #[arg(short, long)]
    dataset: std::path::PathBuf,
    /// Retriever backend (currently: bm25)
    #[arg(short = 'r', long, default_value = "bm25")]
    retriever: String,
    /// K cutoffs for Recall@K and nDCG@K (default: 1, 3, 5, 10)
    #[arg(short = 'k', long, value_delimiter = ',')]
    k_values: Vec<usize>,
    /// Compare baseline against a candidate retriever spec, e.g. "k1=1.5,b=0.6"
    #[arg(long)]
    compare_to: Option<String>,
    /// Optional JSON report output path
    #[arg(short, long)]
    output: Option<std::path::PathBuf>,
  },
}

#[tokio::main]
async fn main() {
  let cli = Cli::parse();

  let result = match cli.command {
    Commands::Workflow(args) => match args.command {
      WorkflowCommands::Run {
        workflow_file,
        watch,
        output,
        model,
        input,
        dry_run,
        timeout,
        max_retries,
        execution_mode,
        max_concurrency,
        run_dir,
      } => {
        if input.len() % 2 != 0 {
          eprintln!(
            "Error: Input must be provided in key-value pairs. Got {} arguments (expected even number).",
            input.len()
          );
          std::process::exit(1);
        }
        let input_pairs = input
          .chunks_exact(2)
          .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
          .collect();
        workflow::run::execute(
          workflow_file,
          watch,
          output,
          model,
          input_pairs,
          dry_run,
          timeout,
          max_retries,
          execution_mode,
          max_concurrency,
          run_dir,
        )
        .await
      }
      WorkflowCommands::Validate {
        workflow_file,
        format,
        strict,
      } => workflow::validate::execute(workflow_file, format, strict).await,
      WorkflowCommands::ResumePlan {
        run_id,
        checkpoint_dir,
        force_replay,
        format,
      } => workflow::resume_plan::execute(run_id, checkpoint_dir, force_replay, format).await,
      WorkflowCommands::Debug {
        workflow_file,
        visualize,
        dry_run,
        analyze,
        validate,
        plan,
        verbose,
      } => {
        workflow::debug::execute(
          workflow_file,
          visualize,
          dry_run,
          analyze,
          validate,
          plan,
          verbose,
        )
        .await
      }
    },
    Commands::Audio(args) => match args.command {
      AudioCommands::Asr {
        model,
        file_path,
        language,
        prompt,
        format,
      } => audio::asr::execute(file_path, model, format, language, prompt).await,
      AudioCommands::Clone {
        model,
        text,
        file_id,
        sample_text: _,
        format,
        output,
      } => audio::clone::execute(file_id, text, model, format, output).await,
      AudioCommands::Tts {
        model,
        voice,
        input,
        output,
        speed,
        format,
        emotion,
      } => audio::tts::execute(input, model, voice, format, speed, output, emotion).await,
    },
    Commands::Config(args) => match args.command {
      ConfigCommands::Init { force } => config_cmd::init::execute(force).await,
      ConfigCommands::Show { section } => config_cmd::show::execute(section).await,
      ConfigCommands::Validate => config_cmd::validate::execute().await,
    },
    Commands::Image(args) => match args.command {
      ImageCommands::Generate {
        prompt,
        model,
        size,
        output,
        format,
        steps,
        cfg_scale,
        seed,
        strength,
        input_image,
      } => {
        image::generate::execute(
          prompt,
          model,
          size,
          output,
          format,
          steps,
          cfg_scale,
          seed,
          strength,
          input_image,
        )
        .await
      }
      ImageCommands::Understand {
        image_path,
        prompt,
        model,
        temperature,
        max_tokens,
        output,
      } => {
        image::understand::execute(image_path, prompt, model, temperature, max_tokens, output).await
      }
    },
    Commands::Llm(args) => match args.command {
      LlmCommands::Models { provider, detailed } => llm::models::execute(provider, detailed).await,
      LlmCommands::Chat {
        model: _,
        system: _,
        save: _,
        load: _,
      } => Err(anyhow::anyhow!(
        "`agentflow llm chat` has been retired. AgentFlow interactions are agent-first: use `agentflow skill chat`, `agentflow skill run`, or a workflow `skill_agent` node. Use `agentflow llm models` only for model discovery."
      )),
    },
    Commands::Mcp(args) => match args.command {
      McpCommands::ListTools {
        server_command,
        timeout_ms,
        max_retries,
      } => mcp::list_tools::execute(server_command, Some(timeout_ms), Some(max_retries)).await,
      McpCommands::CallTool {
        server_command,
        tool,
        params,
        timeout_ms,
        max_retries,
        output,
      } => {
        mcp::call_tool::execute(
          server_command,
          tool,
          params,
          Some(timeout_ms),
          Some(max_retries),
          output,
        )
        .await
      }
      McpCommands::ListResources {
        server_command,
        timeout_ms,
        max_retries,
      } => mcp::list_resources::execute(server_command, Some(timeout_ms), Some(max_retries)).await,
    },
    Commands::Skill(args) => match args.command {
      SkillCommands::Index(args) => match args.command {
        SkillIndexCommands::Validate { index_file } => skill::index::validate(index_file).await,
        SkillIndexCommands::List { index_file } => skill::index::list(index_file).await,
        SkillIndexCommands::Resolve { index_file, skill } => {
          skill::index::resolve(index_file, skill).await
        }
      },
      SkillCommands::Marketplace(args) => match args.command {
        SkillMarketplaceCommands::Validate { marketplace_file } => {
          skill::marketplace::validate(marketplace_file).await
        }
        SkillMarketplaceCommands::List { marketplace_file } => {
          skill::marketplace::list(marketplace_file).await
        }
        SkillMarketplaceCommands::Resolve {
          marketplace_file,
          skill,
        } => skill::marketplace::resolve(marketplace_file, skill).await,
        SkillMarketplaceCommands::Install {
          marketplace_file,
          skill,
          dir,
          force,
        } => skill::marketplace::install(marketplace_file, skill, dir, force).await,
      },
      SkillCommands::Init {
        skill_dir,
        name,
        description,
        force,
      } => skill::init::execute(skill_dir, name, description, force).await,
      SkillCommands::Install {
        index_file,
        skill,
        dir,
        force,
      } => skill::install::execute(index_file, skill, dir, force).await,
      SkillCommands::Validate { skill_dir } => skill::validate::execute(skill_dir).await,
      SkillCommands::Inspect {
        skill_dir,
        explain_permissions,
        allow_tools,
        deny_tools,
      } => skill::inspect::execute(skill_dir, explain_permissions, allow_tools, deny_tools).await,
      SkillCommands::Run {
        skill_dir,
        message,
        model,
        memory,
        session,
        trace,
      } => skill::run::execute(skill_dir, message, model, memory, session, trace).await,
      SkillCommands::Chat {
        skill_dir,
        model,
        memory,
        session,
      } => skill::chat::execute(skill_dir, model, memory, session).await,
      SkillCommands::List { dir } => skill::list::execute(dir).await,
      SkillCommands::ListTools { skill_dir } => skill::list_tools::execute(skill_dir).await,
      SkillCommands::Test {
        skill_dir,
        dry_run,
        smoke,
      } => skill::test::execute(skill_dir, dry_run, smoke).await,
    },
    Commands::Marketplace(args) => match args.command {
      RemoteMarketplaceCommands::Search {
        registry,
        query,
        package_type,
      } => marketplace::search(registry, query, package_type).await,
      RemoteMarketplaceCommands::Install {
        registry,
        package,
        package_type,
        cache_dir,
        install_dir,
        force,
        cache_only,
      } => {
        marketplace::install(
          registry,
          package,
          package_type,
          cache_dir,
          install_dir,
          force,
          cache_only,
        )
        .await
      }
      RemoteMarketplaceCommands::Update {
        registry,
        cache_dir,
      } => marketplace::update(registry, cache_dir).await,
      RemoteMarketplaceCommands::Verify {
        registry,
        package,
        package_type,
        cache_dir,
        strict,
      } => marketplace::verify(registry, package, package_type, cache_dir, strict).await,
    },
    Commands::Trace(args) => match args.command {
      TraceCommands::Replay {
        run_id,
        dir,
        json,
        max_field_chars,
      } => trace::replay::execute(run_id, dir, json, max_field_chars).await,
      TraceCommands::Tui {
        run_id,
        dir,
        filter,
        details,
        max_field_chars,
      } => trace::tui::execute(run_id, dir, filter, details, max_field_chars).await,
    },
    Commands::Doctor(args) => match (
      doctor::OutputFormat::parse(&args.format),
      doctor::DoctorProfile::parse(&args.profile),
    ) {
      (Ok(format), Ok(profile)) => doctor::execute(format, profile, args.server).await,
      (Err(err), _) | (_, Err(err)) => Err(err),
    },
    Commands::Cleanup(args) => {
      cleanup_cmd::execute(
        args.database_url,
        args.run_dir,
        args.trace_dir,
        args.security_profile,
        args.dry_run,
      )
      .await
    }
    Commands::Serve(args) => {
      serve_cmd::execute(
        args.bind,
        args.database_url,
        args.run_dir,
        args.trace_dir,
        args.security_profile,
        args.auth_token_env,
        args.cors_origins,
        args.max_body_mb,
        args.check,
      )
      .await
    }
    Commands::Harness(args) => match args.command {
      HarnessCommands::Run {
        input,
        skill,
        model,
        session,
        workspace,
        profile,
        runtime,
        output,
        run_dir,
        max_steps,
        max_tool_calls,
        timeout_ms,
        no_default_context,
      } => {
        harness::run::execute(
          input,
          skill,
          model,
          session,
          workspace,
          profile,
          runtime,
          output,
          run_dir,
          max_steps,
          max_tool_calls,
          timeout_ms,
          no_default_context,
        )
        .await
      }
      HarnessCommands::Resume {
        session_id,
        run_dir,
        output,
      } => harness::resume::execute(session_id, run_dir, output).await,
      HarnessCommands::List { run_dir, output } => harness::list::execute(run_dir, output).await,
      HarnessCommands::Inspect {
        session_id,
        run_dir,
        output,
      } => harness::inspect::execute(session_id, run_dir, output).await,
    },
    #[cfg(feature = "plugin")]
    Commands::Plugin(args) => match args.command {
      PluginCommands::Install {
        source_dir,
        dir,
        force,
        allow_unsandboxed_plugin,
        signed,
      } => plugin::install::execute(source_dir, dir, force, allow_unsandboxed_plugin, signed).await,
      PluginCommands::List { dir } => plugin::list::execute(dir).await,
      PluginCommands::Inspect { plugin } => plugin::inspect::execute(plugin).await,
      PluginCommands::Uninstall { name, dir, force } => {
        plugin::uninstall::execute(name, dir, force).await
      }
    },
    #[cfg(not(feature = "plugin"))]
    Commands::Plugin(_) => Err(anyhow::anyhow!(
      "`agentflow plugin` is not available in this binary; rebuild with `cargo build -p agentflow-cli --features plugin`"
    )),
    #[cfg(feature = "rag")]
    Commands::Rag(args) => match args.command {
      RagCommands::Search {
        qdrant_url,
        collection,
        query,
        top_k,
        search_type,
        alpha,
        rerank,
        lambda,
        embedding_model,
        output,
      } => {
        rag::search::execute(
          qdrant_url,
          collection,
          query,
          top_k,
          search_type,
          alpha,
          rerank,
          lambda,
          embedding_model,
          output,
        )
        .await
      }
      RagCommands::Index {
        qdrant_url,
        collection,
        documents,
        embedding_model,
      } => rag::index::execute(qdrant_url, collection, documents, embedding_model).await,
      RagCommands::Collections {
        qdrant_url,
        operation,
        collection,
        vector_size,
        distance,
      } => {
        rag::collections::execute(qdrant_url, operation, collection, vector_size, distance).await
      }
      RagCommands::Eval {
        dataset,
        retriever,
        k_values,
        compare_to,
        output,
      } => rag::eval::execute(dataset, retriever, k_values, compare_to, output).await,
    },
    #[cfg(not(feature = "rag"))]
    Commands::Rag(_) => Err(anyhow::anyhow!(
      "`agentflow rag` is not available in this binary; rebuild with `cargo build -p agentflow-cli --features rag`"
    )),
  };

  if let Err(e) = result {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
