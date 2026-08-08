use clap::{Args, Subcommand};

use super::{install, search, update, verify};

#[derive(Args)]
pub struct RemoteMarketplaceArgs {
  #[command(subcommand)]
  command: RemoteMarketplaceCommands,
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
    /// Output format (P10.9.2). `text` (default) prints the existing
    /// human-readable banner; `json` emits the bare structured result;
    /// `json-envelope` wraps it in the canonical `agentflow.cli/1`
    /// envelope so script consumers can parse without scraping stdout.
    #[arg(long, default_value = "text", value_parser = ["text", "json", "json-envelope"])]
    format: String,
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
    /// Downgrade a non-local (http/https) registry from mandatory Ed25519
    /// signature verification to checksum-only verification. Prints a
    /// warning; refuse to use this for production installs.
    #[arg(long)]
    allow_unsigned: bool,
    /// Directory of Ed25519 publisher public keys (default: ~/.agentflow/marketplace-keys)
    #[arg(long)]
    keys_dir: Option<String>,
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
    /// Downgrade a non-local (http/https) registry from mandatory Ed25519
    /// signature verification to checksum-only verification. Prints a
    /// warning; refuse to use this for production installs.
    #[arg(long)]
    allow_unsigned: bool,
    /// Directory of Ed25519 publisher public keys (default: ~/.agentflow/marketplace-keys)
    #[arg(long)]
    keys_dir: Option<String>,
  },
}

pub async fn dispatch(args: RemoteMarketplaceArgs) -> anyhow::Result<()> {
  match args.command {
    RemoteMarketplaceCommands::Search {
      registry,
      query,
      package_type,
      format,
    } => search(registry, query, package_type, format).await,
    RemoteMarketplaceCommands::Install {
      registry,
      package,
      package_type,
      cache_dir,
      install_dir,
      force,
      cache_only,
      allow_unsigned,
      keys_dir,
    } => {
      install(
        registry,
        package,
        package_type,
        cache_dir,
        install_dir,
        force,
        cache_only,
        allow_unsigned,
        keys_dir,
      )
      .await
    }
    RemoteMarketplaceCommands::Update {
      registry,
      cache_dir,
    } => update(registry, cache_dir).await,
    RemoteMarketplaceCommands::Verify {
      registry,
      package,
      package_type,
      cache_dir,
      strict,
      allow_unsigned,
      keys_dir,
    } => {
      verify(
        registry,
        package,
        package_type,
        cache_dir,
        strict,
        allow_unsigned,
        keys_dir,
      )
      .await
    }
  }
}
