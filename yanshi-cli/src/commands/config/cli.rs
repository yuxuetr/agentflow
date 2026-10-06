use clap::{Args, Subcommand};

use super::{init, show, validate};

#[derive(Args)]
pub struct ConfigArgs {
  #[command(subcommand)]
  command: ConfigCommands,
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

pub async fn dispatch(args: ConfigArgs) -> anyhow::Result<()> {
  match args.command {
    ConfigCommands::Init { force } => init::execute(force).await,
    ConfigCommands::Show { section } => show::execute(section).await,
    ConfigCommands::Validate => validate::execute().await,
  }
}
