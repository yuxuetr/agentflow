use clap::{Args, Subcommand};

use super::{call_tool, config, list_resources, list_tools};

#[derive(Args)]
pub struct McpArgs {
  #[command(subcommand)]
  command: McpCommands,
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
    /// Output format: text (colored progress) or json-envelope
    /// (canonical `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
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
    /// Output format: text (default) or json-envelope. In envelope
    /// mode the file written by `--output` carries the envelope, not
    /// the bare result, so the file is self-describing.
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// List available resources from an MCP server
  ListResources {
    /// Server command to execute
    server_command: Vec<String>,
    #[arg(long, default_value_t = 30000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 3)]
    max_retries: u32,
    /// Output format: text (colored progress) or json-envelope
    /// (canonical `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Manage `~/.agentflow/mcp.toml` — the top-level MCP server registry
  Config {
    #[command(subcommand)]
    command: McpConfigCommands,
  },
}

#[derive(Subcommand)]
enum McpConfigCommands {
  /// Print the resolved `mcp.toml` path (or `"<no mcp.toml configured>"`)
  Path,
  /// Parse + validate the config and report the server count
  Validate,
  /// List configured MCP servers (text by default; `--format json`
  /// for legacy bare body, `--format json-envelope` for the
  /// canonical `agentflow.cli/1` wrapper).
  List {
    /// Output format: `text` (default), `json` (legacy bare body
    /// — `{source, servers}`), or `json-envelope` (canonical
    /// `CliJsonEnvelope` wrapping the same body). The bare-body
    /// `json` mode is preserved for back-compat with existing
    /// scripts; new tooling should use `json-envelope` for the
    /// closed wire shape promise.
    #[arg(long, value_parser = ["text", "json", "json-envelope"], default_value = "text")]
    format: String,
  },
  /// Print one server's full config (env, args, timeouts) as JSON
  Show {
    /// Server name to look up
    name: String,
  },
}

pub async fn dispatch(args: McpArgs) -> anyhow::Result<()> {
  match args.command {
    McpCommands::ListTools {
      server_command,
      timeout_ms,
      max_retries,
      format,
    } => list_tools::execute(server_command, Some(timeout_ms), Some(max_retries), format).await,
    McpCommands::CallTool {
      server_command,
      tool,
      params,
      timeout_ms,
      max_retries,
      output,
      format,
    } => {
      call_tool::execute(
        server_command,
        tool,
        params,
        Some(timeout_ms),
        Some(max_retries),
        output,
        format,
      )
      .await
    }
    McpCommands::ListResources {
      server_command,
      timeout_ms,
      max_retries,
      format,
    } => list_resources::execute(server_command, Some(timeout_ms), Some(max_retries), format).await,
    McpCommands::Config { command } => match command {
      McpConfigCommands::Path => config::run_path(),
      McpConfigCommands::Validate => config::run_validate(),
      McpConfigCommands::List { format } => config::run_list(&format),
      McpConfigCommands::Show { name } => config::run_show(&name),
    },
  }
}
