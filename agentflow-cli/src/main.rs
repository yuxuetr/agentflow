use clap::{Parser, Subcommand};
use std::process;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod commands;
mod config;
mod executor;
mod utils;

use commands::{config as config_cmd, llm, workflow, image, audio};

#[derive(Parser)]
#[command(
    name = "agentflow",
    about = "AgentFlow CLI - Workflow orchestration and LLM interaction tool",
    version,
    long_about = "AgentFlow CLI provides a unified interface for workflow execution and LLM interaction.\n\
                 Supports YAML-based workflow configuration, direct LLM commands, and comprehensive\n\
                 multimodal input handling for automation and development workflows."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Set log level (error, warn, info, debug, trace)
    #[arg(long, global = true, default_value = "info")]
    log_level: String,

    /// Output format (json, yaml, text)
    #[arg(long, global = true, default_value = "text")]
    output_format: String,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute workflow from file
    #[command(alias = "r")]
    Run {
        /// Workflow file path
        workflow_file: String,

        /// Watch for file changes and rerun
        #[arg(short, long)]
        watch: bool,

        /// Save execution results to file
        #[arg(short, long)]
        output: Option<String>,

        /// Set input parameters (key=value format)
        #[arg(short, long, value_parser = parse_key_val)]
        input: Vec<(String, String)>,

        /// Validate without executing
        #[arg(long)]
        dry_run: bool,

        /// Set execution timeout
        #[arg(long, default_value = "5m")]
        timeout: String,

        /// Set maximum retry attempts
        #[arg(long, default_value = "3")]
        max_retries: u32,
    },

    /// Validate workflow configuration
    #[command(alias = "v")]
    Validate {
        /// Workflow file path
        workflow_file: String,
    },

    /// List available workflow templates
    List {
        /// List type
        #[arg(value_enum, default_value = "workflows")]
        list_type: ListType,
    },

    /// LLM interaction commands
    Llm {
        #[command(subcommand)]
        command: LlmCommands,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Image generation and understanding
    Image {
        #[command(subcommand)]
        command: ImageCommands,
    },

    /// Audio processing (TTS, ASR, voice cloning)
    Audio {
        #[command(subcommand)]
        command: AudioCommands,
    },
}

#[derive(clap::ValueEnum, Clone)]
pub enum ListType {
    Workflows,
    Templates,
    Models,
}

#[derive(Subcommand)]
enum LlmCommands {
    /// Send prompt to LLM
    #[command(alias = "p")]
    Prompt {
        /// Prompt text
        text: String,

        /// Specify model name
        #[arg(short, long)]
        model: Option<String>,

        /// Set temperature (0.0-1.0)
        #[arg(short, long)]
        temperature: Option<f32>,

        /// Maximum tokens to generate
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Input file (text, image, audio)
        #[arg(short, long)]
        file: Option<String>,

        /// Output file
        #[arg(short, long)]
        output: Option<String>,

        /// Enable streaming output
        #[arg(long)]
        stream: bool,

        /// System prompt
        #[arg(long)]
        system: Option<String>,
    },

    /// Interactive chat session
    #[command(alias = "c")]
    Chat {
        /// Specify model name
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt
        #[arg(long)]
        system: Option<String>,

        /// Save conversation to file
        #[arg(long)]
        save: Option<String>,

        /// Load conversation from file
        #[arg(long)]
        load: Option<String>,
    },

    /// List available models
    #[command(alias = "m")]
    Models {
        /// Filter by provider
        #[arg(short, long)]
        provider: Option<String>,

        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Initialize configuration files
    Init {
        /// Force overwrite existing configuration
        #[arg(short, long)]
        force: bool,
    },

    /// Display current configuration
    Show {
        /// Configuration section to show
        section: Option<String>,
    },

    /// Validate configuration
    Validate,
}

#[derive(Subcommand)]
enum ImageCommands {
    /// Generate images from text descriptions
    #[command(alias = "gen")]
    Generate {
        /// Text prompt for image generation
        prompt: String,

        /// Specify model name (default: step-1x-medium)
        #[arg(short, long)]
        model: Option<String>,

        /// Image size (e.g., "1024x1024", "768x768")
        #[arg(short, long, default_value = "1024x1024")]
        size: String,

        /// Output file path
        #[arg(short, long)]
        output: String,

        /// Response format (b64_json, url)
        #[arg(short, long, default_value = "b64_json")]
        format: String,

        /// Number of inference steps (higher = better quality)
        #[arg(long, default_value = "30")]
        steps: u32,

        /// CFG scale for guidance (higher = more prompt adherence)
        #[arg(long, default_value = "7.5")]
        cfg_scale: f32,

        /// Random seed for reproducible generation
        #[arg(long)]
        seed: Option<u64>,

        /// Strength for image-to-image (0.0-1.0)
        #[arg(long)]
        strength: Option<f32>,

        /// Input image for image-to-image generation
        #[arg(long)]
        input_image: Option<String>,
    },

    /// Understand and analyze images
    #[command(alias = "analyze")]
    Understand {
        /// Path to image file
        image_path: String,

        /// Analysis prompt
        prompt: String,

        /// Specify model name (default: step-1v-8k)
        #[arg(short, long)]
        model: Option<String>,

        /// Set temperature (0.0-1.0)
        #[arg(short, long)]
        temperature: Option<f32>,

        /// Maximum tokens to generate
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Output file
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum AudioCommands {
    /// Convert text to speech
    #[command(alias = "tts")]
    TextToSpeech {
        /// Text to convert to speech
        text: String,

        /// Specify model name (default: step-tts-mini)
        #[arg(short, long)]
        model: Option<String>,

        /// Voice name
        #[arg(short, long, default_value = "default")]
        voice: String,

        /// Audio format (mp3, wav, flac)
        #[arg(short, long, default_value = "mp3")]
        format: String,

        /// Speech speed (0.5-2.0)
        #[arg(long, default_value = "1.0")]
        speed: f32,

        /// Output file path
        #[arg(short, long)]
        output: String,

        /// Emotion/style (for supported models)
        #[arg(long)]
        emotion: Option<String>,
    },

    /// Convert speech to text
    #[command(alias = "asr")]
    SpeechToText {
        /// Audio file path
        audio_path: String,

        /// Specify model name (default: step-asr)
        #[arg(short, long)]
        model: Option<String>,

        /// Output format (text, json, srt)
        #[arg(short, long, default_value = "text")]
        format: String,

        /// Language code (auto-detect if not specified)
        #[arg(short, long)]
        language: Option<String>,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Clone voice from reference audio
    #[command(alias = "clone")]
    VoiceClone {
        /// Reference audio file for voice cloning
        reference_audio: String,

        /// New text to speak in cloned voice
        text: String,

        /// Specify model name (default: step-speech)
        #[arg(short, long)]
        model: Option<String>,

        /// Audio format (mp3, wav, flac)
        #[arg(short, long, default_value = "mp3")]
        format: String,

        /// Output file path
        #[arg(short, long)]
        output: String,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli);

    // Execute command
    let result = match cli.command {
        Commands::Run {
            workflow_file,
            watch,
            output,
            input,
            dry_run,
            timeout,
            max_retries,
        } => {
            workflow::run::execute(
                workflow_file,
                watch,
                output,
                input,
                dry_run,
                timeout,
                max_retries,
            )
            .await
        }

        Commands::Validate { workflow_file } => workflow::validate::execute(workflow_file).await,

        Commands::List { list_type } => workflow::list::execute(list_type).await,

        Commands::Llm { command } => match command {
            LlmCommands::Prompt {
                text,
                model,
                temperature,
                max_tokens,
                file,
                output,
                stream,
                system,
            } => {
                llm::prompt::execute(text, model, temperature, max_tokens, file, output, stream, system)
                    .await
            }

            LlmCommands::Chat {
                model,
                system,
                save,
                load,
            } => llm::chat::execute(model, system, save, load).await,

            LlmCommands::Models { provider, detailed } => {
                llm::models::execute(provider, detailed).await
            }
        },

        Commands::Config { command } => match command {
            ConfigCommands::Init { force } => config_cmd::init::execute(force).await,
            ConfigCommands::Show { section } => config_cmd::show::execute(section).await,
            ConfigCommands::Validate => config_cmd::validate::execute().await,
        },

        Commands::Image { command } => match command {
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
                    prompt, model, size, output, format, steps, cfg_scale, seed, strength, input_image,
                ).await
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

        Commands::Audio { command } => match command {
            AudioCommands::TextToSpeech {
                text,
                model,
                voice,
                format,
                speed,
                output,
                emotion,
            } => {
                audio::tts::execute(text, model, voice, format, speed, output, emotion).await
            }

            AudioCommands::SpeechToText {
                audio_path,
                model,
                format,
                language,
                output,
            } => {
                audio::asr::execute(audio_path, model, format, language, output).await
            }

            AudioCommands::VoiceClone {
                reference_audio,
                text,
                model,
                format,
                output,
            } => {
                audio::clone::execute(reference_audio, text, model, format, output).await
            }
        },
    };

    // Handle result
    match result {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}

fn init_logging(cli: &Cli) {
    let log_level = if cli.verbose {
        "debug"
    } else {
        &cli.log_level
    };

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_ansi(!cli.no_color)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set logger");
}