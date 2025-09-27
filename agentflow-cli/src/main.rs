use clap::{Parser, Subcommand};

mod commands;
mod config;
mod executor;

use commands::workflow;

#[derive(Parser)]
#[command(name = "agentflow", version, about = "AgentFlow V2 CLI")]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Workflow orchestration commands
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },
}

#[derive(Subcommand)]
enum WorkflowCommands {
    /// Execute workflow from file
    Run {
        workflow_file: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Workflow { command } => match command {
            WorkflowCommands::Run { workflow_file } => {
                workflow::run::execute(workflow_file, false, None, vec![], false, "".to_string(), 0).await
            }
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}