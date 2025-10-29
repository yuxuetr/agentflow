use clap::{Args, Parser, Subcommand};

mod commands;
mod config;
mod executor;

use commands::{audio, config as config_cmd, image, llm, workflow};

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
}

#[derive(Args)]
struct WorkflowArgs { #[command(subcommand)] command: WorkflowCommands, }
#[derive(Args)]
struct AudioArgs { #[command(subcommand)] command: AudioCommands, }
#[derive(Args)]
struct ConfigArgs { #[command(subcommand)] command: ConfigCommands, }
#[derive(Args)]
struct ImageArgs { #[command(subcommand)] command: ImageCommands, }
#[derive(Args)]
struct LlmArgs { #[command(subcommand)] command: LlmCommands, }

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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Workflow(args) => match args.command {
            WorkflowCommands::Run { workflow_file, watch, output, input, dry_run, timeout, max_retries } => {
                let input_pairs = input.chunks_exact(2).map(|chunk| (chunk[0].clone(), chunk[1].clone())).collect();
                workflow::run::execute(workflow_file, watch, output, input_pairs, dry_run, timeout, max_retries).await
            }
            WorkflowCommands::Debug { workflow_file, visualize, dry_run, analyze, validate, plan, verbose } => {
                workflow::debug::execute(workflow_file, visualize, dry_run, analyze, validate, plan, verbose).await
            }
        },
        Commands::Audio(args) => match args.command {
            AudioCommands::Asr { model, file_path, language, prompt, format } => {
                audio::asr::execute(file_path, model, format, language, prompt).await
            }
            AudioCommands::Clone { model, text, file_id, sample_text: _, format, output } => {
                audio::clone::execute(file_id, text, model, format, output).await
            }
            AudioCommands::Tts { model, voice, input, output, speed, format, emotion } => {
                audio::tts::execute(input, model, voice, format, speed, output, emotion).await
            }
        },
        Commands::Config(args) => match args.command {
            ConfigCommands::Init { force } => {
                config_cmd::init::execute(force).await
            }
            ConfigCommands::Show { section } => {
                config_cmd::show::execute(section).await
            }
            ConfigCommands::Validate => {
                config_cmd::validate::execute().await
            }
        },
        Commands::Image(args) => match args.command {
            ImageCommands::Generate { prompt, model, size, output, format, steps, cfg_scale, seed, strength, input_image } => {
                image::generate::execute(prompt, model, size, output, format, steps, cfg_scale, seed, strength, input_image).await
            }
            ImageCommands::Understand { image_path, prompt, model, temperature, max_tokens, output } => {
                image::understand::execute(image_path, prompt, model, temperature, max_tokens, output).await
            }
        },
        Commands::Llm(args) => match args.command {
            LlmCommands::Models { provider, detailed } => {
                llm::models::execute(provider, detailed).await
            }
            LlmCommands::Chat { model, system, save, load } => {
                llm::chat::execute(model, system, save, load).await
            }
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
