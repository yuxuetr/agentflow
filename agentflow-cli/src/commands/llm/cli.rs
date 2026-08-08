use clap::{Args, Subcommand};

use super::models;

#[derive(Args)]
pub struct LlmArgs {
  #[command(subcommand)]
  command: LlmCommands,
}

#[derive(Subcommand)]
enum LlmCommands {
  Models {
    #[arg(short, long)]
    provider: Option<String>,
    #[arg(short, long)]
    detailed: bool,
    /// Live-query each OpenAI-compatible provider's `/v1/models`
    /// endpoint and print the delta vs the local registry: which
    /// models are NEW on the provider (not yet in your `models.yml`)
    /// and which LOCAL entries don't appear in the provider's list
    /// (deprecated / typos / private models). Read-only — does not
    /// write to `models.yml`. Requires each provider's API key in
    /// the environment. Currently supported: openai, moonshot,
    /// stepfun, dashscope. Other providers fall back to "skipped
    /// (refresh not supported)". F-A7-6.
    #[arg(long)]
    refresh_from_api: bool,
    /// Output format: text (default) or json-envelope (canonical
    /// `CliJsonEnvelope` — `agentflow.cli/1` wire schema; mutually
    /// exclusive with `--refresh-from-api`).
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Deprecated compatibility stub. Use `skill chat`, `skill run`, or workflow `skill_agent`.
  ///
  /// Q3.5.1: the previous `--model` / `--system` / `--save` / `--load`
  /// flags were accepted by clap but unconditionally dropped by the
  /// handler — accepting structured arguments that the command will
  /// never read is misleading. All flags removed; the command itself
  /// stays so users who still type `agentflow llm chat` get the
  /// redirect message.
  ///
  /// Old invocations almost always carry `--model <X>` and friends
  /// because that was the pre-retirement contract. Without
  /// `allow_hyphen_values`, clap would reject those flags as
  /// "unexpected argument" with exit code 2 before our redirect
  /// handler runs, leaving the user staring at a confusing clap
  /// error instead of the migration message. Collect leftover args
  /// into a `_extra` sink so any legacy flag form lands on the
  /// retired-message handler.
  #[command(hide = true)]
  Chat {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    _extra: Vec<String>,
  },
}

pub async fn dispatch(args: LlmArgs) -> anyhow::Result<()> {
  match args.command {
    LlmCommands::Models {
      provider,
      detailed,
      refresh_from_api,
      format,
    } => models::execute(provider, detailed, refresh_from_api, format).await,
    LlmCommands::Chat { _extra: _ } => Err(anyhow::anyhow!(
      "`agentflow llm chat` has been retired. AgentFlow interactions are agent-first: use `agentflow skill chat`, `agentflow skill run`, or a workflow `skill_agent` node. Use `agentflow llm models` only for model discovery."
    )),
  }
}
