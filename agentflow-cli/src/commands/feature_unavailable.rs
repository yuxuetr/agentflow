//! Stand-in `clap::Args` type for a top-level command whose backing
//! Cargo feature is disabled in this build (`plugin` / `rag`). It accepts
//! and swallows any trailing arguments so the CLI still parses, and its
//! `after_help` points the operator at the rebuild flag they need.
//!
//! The whole module is gated (see its declaration in `commands/mod.rs`)
//! so that a default build (both `plugin` and `rag` enabled) never
//! compiles an unused type.

use clap::Args;

#[derive(Args)]
#[command(
  after_help = "This command is not available in this binary. Rebuild with the matching Cargo feature, e.g. `cargo build -p agentflow-cli --features rag` or `--features plugin`."
)]
pub struct FeatureUnavailableArgs {
  #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
  args: Vec<String>,
}
