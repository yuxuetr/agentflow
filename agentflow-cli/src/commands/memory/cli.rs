use clap::{Args, Subcommand};

use super::prune;

#[derive(Args)]
pub struct MemoryArgs {
  #[command(subcommand)]
  command: MemoryCommands,
}

#[derive(Subcommand)]
enum MemoryCommands {
  /// Prune memory-store rows older than a retention cutoff.
  ///
  /// Operates on the SQLite file your agent runtime writes to —
  /// either an explicit `--db <path>` or default
  /// `~/.agentflow/memory.db`. Pruning is layer-scoped:
  ///
  /// - `preference`: drops rows whose `updated_at` is older than
  ///   `--older-than`. Used to retire stale per-user prefs.
  /// - `entity_facts`: drops INVALIDATED rows whose `invalidated_at`
  ///   is older than `--older-than`. Active facts are never touched.
  ///
  /// Session + semantic layers expose per-session clear instead of
  /// retention-based prune and are out of scope for this command.
  Prune {
    /// Memory layer to prune. Supported: preference, entity_facts.
    #[arg(long, value_parser = ["preference", "entity_facts"])]
    layer: String,
    /// SQLite file backing the chosen layer. Defaults to
    /// `~/.agentflow/memory.db` (the agent runtime convention).
    #[arg(long)]
    db: Option<std::path::PathBuf>,
    /// Retention cutoff: rows updated/invalidated this far in the
    /// past or further are removed. Format: `<integer><unit>` where
    /// unit ∈ {s, m, h, d, w, y}. Examples: `30d`, `12w`, `2y`.
    /// A bare integer is rejected — silently defaulting to a unit
    /// would turn typos into data loss.
    #[arg(long)]
    older_than: String,
    /// Output format: text (default — coloured ✓ line) or
    /// json-envelope (canonical `CliJsonEnvelope` wrapping
    /// `{layer, db, older_than, older_than_seconds, removed_rows}`).
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
}

pub async fn dispatch(args: MemoryArgs) -> anyhow::Result<()> {
  match args.command {
    MemoryCommands::Prune {
      layer,
      db,
      older_than,
      format,
    } => {
      // Default to ~/.agentflow/memory.db when --db isn't passed.
      // This mirrors the convention agent runtimes follow when
      // constructing SqlitePreferenceStore / SqliteEntityFactStore.
      let db_path = db.unwrap_or_else(|| {
        dirs::home_dir()
          .map(|h| h.join(".agentflow").join("memory.db"))
          .unwrap_or_else(|| std::path::PathBuf::from("memory.db"))
      });
      prune::execute(layer, db_path, older_than, format).await
    }
  }
}
