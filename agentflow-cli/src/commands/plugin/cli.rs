use clap::{Args, Subcommand};

use super::{generate, inspect, install, list, uninstall};

#[derive(Args)]
pub struct PluginArgs {
  #[command(subcommand)]
  command: PluginCommands,
}

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
    /// Directory of `<key_id>.pub` Ed25519 public keys used to verify
    /// a manifest `[plugin.signature]` block, if present (default:
    /// `~/.agentflow/marketplace-keys/`, shared with `agentflow
    /// marketplace install/verify`). Whether an unsigned plugin is
    /// accepted is a `production`-profile policy decision, not a
    /// flag — see `--allow-unsandboxed-plugin` for the sandbox
    /// equivalent.
    #[arg(long)]
    keys_dir: Option<String>,
    /// Output format: text (default) or json-envelope (canonical
    /// `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// List installed plugins and the node types each one declares
  List {
    /// Plugins directory (default: ~/.agentflow/plugins)
    #[arg(short, long)]
    dir: Option<String>,
    /// Output format: text (default) or json-envelope (canonical
    /// `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Inspect a plugin manifest without spawning the subprocess
  Inspect {
    /// Path to a plugin directory or its plugin.toml file
    plugin: String,
    /// Output format: text (default) or json-envelope (canonical
    /// `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
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
    /// Output format: text (default) or json-envelope (canonical
    /// `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Generate a workflow YAML stub for plugin-declared nodes
  GenerateWorkflowStub {
    /// Path to the plugin directory or plugin.toml file
    plugin: String,
    /// Only emit a stub for this specific node type (default: all)
    #[arg(short, long)]
    node: Option<String>,
    /// Write the stub to this file instead of stdout
    #[arg(short, long)]
    output: Option<String>,
    /// Output format: text (default) or json-envelope. In envelope
    /// mode the file written by `--output` carries the raw stub
    /// (unchanged); stdout always carries the envelope with the
    /// stub inlined as a string when no `--output` is set.
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
}

pub async fn dispatch(args: PluginArgs) -> anyhow::Result<()> {
  match args.command {
    PluginCommands::Install {
      source_dir,
      dir,
      force,
      allow_unsandboxed_plugin,
      keys_dir,
      format,
    } => {
      install::execute(
        source_dir,
        dir,
        force,
        allow_unsandboxed_plugin,
        keys_dir,
        format,
      )
      .await
    }
    PluginCommands::List { dir, format } => list::execute(dir, format).await,
    PluginCommands::Inspect { plugin, format } => inspect::execute(plugin, format).await,
    PluginCommands::Uninstall {
      name,
      dir,
      force,
      format,
    } => uninstall::execute(name, dir, force, format).await,
    PluginCommands::GenerateWorkflowStub {
      plugin,
      node,
      output,
      format,
    } => generate::execute(plugin, node, output, format).await,
  }
}
