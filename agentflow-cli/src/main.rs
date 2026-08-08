use clap::{Parser, Subcommand};

use agentflow_cli::commands;

#[derive(Parser)]
#[command(name = "agentflow", version, about = "AgentFlow V2 CLI")]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Workflow orchestration commands
  Workflow(commands::workflow::cli::WorkflowArgs),
  /// Audio generation and transcription commands
  Audio(commands::audio::cli::AudioArgs),
  /// Configuration management commands
  Config(commands::config::cli::ConfigArgs),
  /// Image generation and understanding commands
  Image(commands::image::cli::ImageArgs),
  /// LLM model discovery commands
  Llm(commands::llm::cli::LlmArgs),
  /// Model Context Protocol (MCP) commands
  Mcp(commands::mcp::cli::McpArgs),
  /// Memory store maintenance commands (prune retention layers)
  Memory(commands::memory::cli::MemoryArgs),
  /// Skill management commands
  Skill(commands::skill::cli::SkillArgs),
  /// Remote Skill / Plugin marketplace commands
  Marketplace(commands::marketplace::cli::RemoteMarketplaceArgs),
  /// Trace inspection and replay commands
  Trace(commands::trace::cli::TraceArgs),
  /// Diagnose local AgentFlow configuration and runtime capabilities
  Doctor(commands::doctor::cli::DoctorArgs),
  /// Harness Agent Mode: workspace-aware, long-lived agent sessions
  Harness(commands::harness::cli::HarnessArgs),
  /// ReAct-native agent inspection commands (trace diff, etc.)
  Agent(commands::agent::cli::AgentArgs),
  /// Boot the AgentFlow Gateway (Axum HTTP API) by spawning `agentflow-server`
  Serve(commands::serve::cli::ServeArgs),
  /// Run the retention sweep once and exit (delegates to `agentflow-server --cleanup`)
  Cleanup(commands::cleanup::cli::CleanupArgs),
  /// Snapshot Postgres + filesystem state into a single bundle directory (P10.15.1)
  Backup(commands::backup::cli::BackupArgs),
  /// Restore Postgres + filesystem state from an `agentflow backup` bundle (T2.2)
  Restore(commands::restore::cli::RestoreArgs),
  /// Run an agent eval dataset and emit a structured report
  Eval(commands::eval::cli::EvalArgs),
  #[cfg(feature = "plugin")]
  /// Plugin management commands (subprocess plugins)
  Plugin(commands::plugin::cli::PluginArgs),
  #[cfg(not(feature = "plugin"))]
  /// Plugin management commands (disabled in this build)
  Plugin(commands::feature_unavailable::FeatureUnavailableArgs),
  #[cfg(feature = "rag")]
  /// RAG (Retrieval-Augmented Generation) commands
  Rag(commands::rag::cli::RagArgs),
  #[cfg(not(feature = "rag"))]
  /// RAG commands (disabled in this build)
  Rag(commands::feature_unavailable::FeatureUnavailableArgs),
}

/// Load `~/.agentflow/.env` if present so operators don't have to
/// `source` it before every CLI invocation. Silent no-op when the
/// file is missing (e.g. CI without ~/.agentflow/) — and process
/// env vars take precedence over file values (dotenvy default), so
/// inline overrides like `MOONSHOT_API_KEY=other agentflow ...` keep
/// working.
///
/// Called before `Cli::parse()` so any flag whose default reads an
/// env var (none today, but future-proofing) sees the loaded values.
fn load_agentflow_dotenv() {
  if let Some(home) = std::env::home_dir() {
    let _ = dotenvy::from_path(home.join(".agentflow").join(".env"));
  }
}

#[tokio::main]
async fn main() {
  load_agentflow_dotenv();
  let cli = Cli::parse();

  let result = match cli.command {
    Commands::Workflow(args) => commands::workflow::cli::dispatch(args).await,
    Commands::Audio(args) => commands::audio::cli::dispatch(args).await,
    Commands::Config(args) => commands::config::cli::dispatch(args).await,
    Commands::Image(args) => commands::image::cli::dispatch(args).await,
    Commands::Llm(args) => commands::llm::cli::dispatch(args).await,
    Commands::Mcp(args) => commands::mcp::cli::dispatch(args).await,
    Commands::Memory(args) => commands::memory::cli::dispatch(args).await,
    Commands::Skill(args) => commands::skill::cli::dispatch(args).await,
    Commands::Marketplace(args) => commands::marketplace::cli::dispatch(args).await,
    Commands::Trace(args) => commands::trace::cli::dispatch(args).await,
    Commands::Doctor(args) => commands::doctor::cli::dispatch(args).await,
    Commands::Harness(args) => commands::harness::cli::dispatch(args).await,
    Commands::Agent(args) => commands::agent::cli::dispatch(args).await,
    Commands::Serve(args) => commands::serve::cli::dispatch(args).await,
    Commands::Cleanup(args) => commands::cleanup::cli::dispatch(args).await,
    Commands::Backup(args) => commands::backup::cli::dispatch(args).await,
    Commands::Restore(args) => commands::restore::cli::dispatch(args).await,
    Commands::Eval(args) => commands::eval::cli::dispatch(args).await,
    #[cfg(feature = "plugin")]
    Commands::Plugin(args) => commands::plugin::cli::dispatch(args).await,
    #[cfg(not(feature = "plugin"))]
    Commands::Plugin(_) => Err(anyhow::anyhow!(
      "`agentflow plugin` is not available in this binary; rebuild with `cargo build -p agentflow-cli --features plugin`"
    )),
    #[cfg(feature = "rag")]
    Commands::Rag(args) => commands::rag::cli::dispatch(args).await,
    #[cfg(not(feature = "rag"))]
    Commands::Rag(_) => Err(anyhow::anyhow!(
      "`agentflow rag` is not available in this binary; rebuild with `cargo build -p agentflow-cli --features rag`"
    )),
  };

  if let Err(e) = result {
    // `{:#}` is anyhow's "chain display" formatter — prints the
    // outermost context plus every `source()` joined by ": ".
    // Bare `{}` would only show the outermost message, hiding the
    // underlying cause (e.g. `agentflow skill validate` was bailing
    // with just "Error: Validation failed", swallowing the
    // structured `SkillError::ValidationError.message` that explained
    // *why*). See P9.1 / F-AF-1 in the L1+L3 reflection doc.
    eprintln!("Error: {:#}", e);
    std::process::exit(1);
  }
}
