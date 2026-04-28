use clap::{Args, Parser, Subcommand};

mod commands;
mod config;
mod executor;
mod redaction;

#[cfg(feature = "rag")]
use commands::rag;
use commands::{audio, config as config_cmd, image, llm, mcp, skill, trace, workflow};

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
  /// LLM interaction commands
  Llm(LlmArgs),
  /// Model Context Protocol (MCP) commands
  Mcp(McpArgs),
  /// Skill management commands
  Skill(SkillArgs),
  /// Trace inspection and replay commands
  Trace(TraceArgs),
  #[cfg(feature = "rag")]
  /// RAG (Retrieval-Augmented Generation) commands
  Rag(RagArgs),
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
struct TraceArgs {
  #[command(subcommand)]
  command: TraceCommands,
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
    #[arg(short, long, num_args = 2, value_names = ["KEY", "VALUE"])]
    input: Vec<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long, default_value = "60s")]
    timeout: String,
    #[arg(long, default_value_t = 0)]
    max_retries: u32,
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
  /// Run a skill with a single message and exit
  Run {
    /// Path to the skill directory
    skill_dir: String,
    /// The message to send to the agent
    #[arg(short, long)]
    message: String,
    /// Reuse an existing session ID for multi-turn conversations
    #[arg(long)]
    session: Option<String>,
    /// Print the structured AgentRuntime trace as JSON
    #[arg(long)]
    trace: bool,
  },
  /// Start an interactive multi-turn chat session with a skill
  Chat {
    /// Path to the skill directory
    skill_dir: String,
    /// Resume an existing session by ID (optional)
    #[arg(long)]
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
}

#[derive(Subcommand)]
enum TraceCommands {
  /// Replay a persisted workflow/agent trace without re-executing tools or LLMs
  Replay {
    /// Workflow run ID / trace ID to replay
    run_id: String,
    /// Directory containing file-backed traces (default: ~/.agentflow/traces)
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
    /// Directory containing file-backed traces (default: ~/.agentflow/traces)
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
        input,
        dry_run,
        timeout,
        max_retries,
      } => {
        if input.len() % 2 != 0 {
          eprintln!("Error: Input must be provided in key-value pairs. Got {} arguments (expected even number).", input.len());
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
          input_pairs,
          dry_run,
          timeout,
          max_retries,
        )
        .await
      }
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
        model,
        system,
        save,
        load,
      } => llm::chat::execute(model, system, save, load).await,
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
      SkillCommands::Run {
        skill_dir,
        message,
        session,
        trace,
      } => skill::run::execute(skill_dir, message, session, trace).await,
      SkillCommands::Chat { skill_dir, session } => skill::chat::execute(skill_dir, session).await,
      SkillCommands::List { dir } => skill::list::execute(dir).await,
      SkillCommands::ListTools { skill_dir } => skill::list_tools::execute(skill_dir).await,
      SkillCommands::Test { skill_dir, smoke } => skill::test::execute(skill_dir, smoke).await,
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
    },
  };

  if let Err(e) = result {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
