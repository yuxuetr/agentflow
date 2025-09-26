//! Paper Assistant - CLI Application
//!
//! A command-line tool for comprehensive arXiv paper processing using AI agents.
//! Provides Chinese summarization, translation, mind mapping, and poster generation.

use anyhow::Result;
use clap::{Arg, Command, ArgMatches};
use env_logger;
use log::{info, error, warn};
use std::path::Path;
use tokio;

use paper_assistant::{PaperAssistant, PaperAssistantConfig, ConfigBuilder};

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize logging
  env_logger::Builder::from_default_env()
    .filter_level(log::LevelFilter::Info)
    .init();

  // Parse command line arguments
  let matches = create_cli_app().get_matches();

  match matches.subcommand() {
    Some(("process", sub_matches)) => {
      process_paper_command(sub_matches).await?;
    },
    Some(("config", sub_matches)) => {
      config_command(sub_matches).await?;
    },
    Some(("examples", _)) => {
      show_examples();
    },
    _ => {
      println!("Use --help for usage information or 'examples' subcommand for examples");
    }
  }

  Ok(())
}

/// Create the CLI application structure
fn create_cli_app() -> Command {
  Command::new("paper-assistant")
    .version("0.1.0")
    .author("AgentFlow Team")
    .about("AI Agent for comprehensive arXiv paper processing with Chinese translation and mind mapping")
    .subcommand(
      Command::new("process")
        .about("Process an arXiv paper")
        .arg(
          Arg::new("url")
            .help("arXiv paper URL or ID (e.g., https://arxiv.org/abs/2312.07104 or 2312.07104)")
            .required(true)
            .index(1)
        )
        .arg(
          Arg::new("output")
            .short('o')
            .long("output")
            .help("Output directory for results")
            .default_value("./paper_assistant_output")
        )
        .arg(
          Arg::new("config")
            .short('c')
            .long("config")
            .help("Path to configuration JSON file")
        )
        .arg(
          Arg::new("fast")
            .long("fast")
            .help("Use fast processing mode (skip image generation)")
            .action(clap::ArgAction::SetTrue)
        )
        .arg(
          Arg::new("comprehensive")
            .long("comprehensive")
            .help("Use comprehensive analysis mode")
            .action(clap::ArgAction::SetTrue)
        )
        .arg(
          Arg::new("no-mindmaps")
            .long("no-mindmaps")
            .help("Skip mind map generation")
            .action(clap::ArgAction::SetTrue)
        )
        .arg(
          Arg::new("no-poster")
            .long("no-poster")
            .help("Skip poster generation")
            .action(clap::ArgAction::SetTrue)
        )
        .arg(
          Arg::new("max-sections")
            .long("max-sections")
            .help("Maximum number of sections for mind mapping")
            .value_parser(clap::value_parser!(usize))
        )
    )
    .subcommand(
      Command::new("config")
        .about("Configuration management")
        .subcommand(
          Command::new("show")
            .about("Show current default configuration")
        )
        .subcommand(
          Command::new("create")
            .about("Create a configuration file")
            .arg(
              Arg::new("output")
                .short('o')
                .long("output")
                .help("Output path for configuration file")
                .default_value("paper-assistant-config.json")
            )
            .arg(
              Arg::new("type")
                .short('t')
                .long("type")
                .help("Configuration type")
                .value_parser(["default", "fast", "comprehensive"])
                .default_value("default")
            )
        )
    )
    .subcommand(
      Command::new("examples")
        .about("Show usage examples")
    )
}

/// Handle paper processing command
async fn process_paper_command(matches: &ArgMatches) -> Result<()> {
  let url = matches.get_one::<String>("url").unwrap();
  let output_dir = matches.get_one::<String>("output").unwrap();

  info!("Starting paper processing for: {}", url);
  info!("Output directory: {}", output_dir);

  // Load or create configuration
  let config = if let Some(config_path) = matches.get_one::<String>("config") {
    info!("Loading configuration from: {}", config_path);
    PaperAssistantConfig::from_json_file(config_path)?
  } else {
    // Create configuration based on command line flags
    let mut config = if matches.get_flag("fast") {
      info!("Using fast processing mode");
      PaperAssistantConfig::fast_processing()
    } else if matches.get_flag("comprehensive") {
      info!("Using comprehensive analysis mode");
      PaperAssistantConfig::comprehensive_analysis()
    } else {
      ConfigBuilder::new().from_env().build()?
    };

    // Override output directory
    config.output_directory = output_dir.clone();

    // Apply command line flags
    if matches.get_flag("no-mindmaps") {
      config.enable_mind_maps = false;
      info!("Mind map generation disabled");
    }

    if matches.get_flag("no-poster") {
      config.enable_poster_generation = false;
      info!("Poster generation disabled");
    }

    if let Some(max_sections) = matches.get_one::<usize>("max-sections") {
      config.max_sections_for_mind_maps = Some(*max_sections);
      info!("Maximum sections for mind mapping set to: {}", max_sections);
    }

    config
  };

  // Validate configuration
  config.validate().map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

  // Create and run paper assistant
  let mut assistant = PaperAssistant::with_config(config)?;

  info!("Processing paper...");
  match assistant.process_paper(url).await {
    Ok(result) => {
      info!("Paper processing completed successfully!");
      info!("Paper ID: {}", result.paper_id);
      info!("Processing time: {}ms", result.processing_time_ms);
      info!("Sections with mind maps: {}", result.mind_maps.len());

      if result.poster_image_path.is_some() {
        info!("Poster image generated");
      }

      // Save results to files
      assistant.save_results(&result, output_dir).await?;
      info!("Results saved to: {}", output_dir);

      // Print summary
      println!("\n=== Paper Processing Summary ===");
      println!("Paper ID: {}", result.paper_id);
      println!("Original URL: {}", result.original_url);
      println!("Processing time: {}ms", result.processing_time_ms);
      println!("Chinese summary generated: ✓");
      println!("Chinese translation generated: ✓");
      println!("Mind maps created: {}", result.mind_maps.len());
      if result.poster_image_path.is_some() {
        println!("Poster image generated: ✓");
      } else {
        println!("Poster image generated: ✗");
      }
      println!("Output directory: {}", output_dir);
      println!("\nProcessing completed successfully!");
    },
    Err(e) => {
      error!("Paper processing failed: {}", e);
      println!("Error: {}", e);
      
      // Try to save partial results if available
      if let Some(shared_state) = Some(assistant.shared_state()) {
        if !shared_state.is_empty() {
          warn!("Attempting to save partial results...");
          let partial_output_dir = format!("{}/partial_results", output_dir);
          
          // Create output directory
          if let Err(e) = tokio::fs::create_dir_all(&partial_output_dir).await {
            error!("Failed to create partial results directory: {}", e);
          } else {
            // Save shared state as JSON for debugging
            let debug_path = format!("{}/debug_state.json", partial_output_dir);
            if let Ok(json_content) = serde_json::to_string_pretty(&shared_state.clone()) {
              if let Err(e) = tokio::fs::write(&debug_path, json_content).await {
                error!("Failed to save debug state: {}", e);
              } else {
                info!("Debug state saved to: {}", debug_path);
              }
            }
          }
        }
      }
      
      std::process::exit(1);
    }
  }

  Ok(())
}

/// Handle configuration commands
async fn config_command(matches: &ArgMatches) -> Result<()> {
  match matches.subcommand() {
    Some(("show", _)) => {
      println!("=== Default Paper Assistant Configuration ===");
      let config = PaperAssistantConfig::default();
      let json_output = serde_json::to_string_pretty(&config)?;
      println!("{}", json_output);
    },
    Some(("create", sub_matches)) => {
      let output_path = sub_matches.get_one::<String>("output").unwrap();
      let config_type = sub_matches.get_one::<String>("type").unwrap();

      let config = match config_type.as_str() {
        "fast" => PaperAssistantConfig::fast_processing(),
        "comprehensive" => PaperAssistantConfig::comprehensive_analysis(),
        _ => PaperAssistantConfig::default(),
      };

      config.to_json_file(output_path)?;
      println!("Configuration file created at: {}", output_path);
      println!("Type: {}", config_type);
    },
    _ => {
      println!("Use 'config show' or 'config create' subcommands");
    }
  }

  Ok(())
}

/// Show usage examples
fn show_examples() {
  println!("=== Paper Assistant Usage Examples ===\n");

  println!("1. Basic paper processing:");
  println!("   paper-assistant process https://arxiv.org/abs/2312.07104\n");

  println!("2. Process with custom output directory:");
  println!("   paper-assistant process 2312.07104 -o ./my_output\n");

  println!("3. Fast processing mode (skip image generation):");
  println!("   paper-assistant process https://arxiv.org/abs/2312.07104 --fast\n");

  println!("4. Comprehensive analysis mode:");
  println!("   paper-assistant process 2312.07104 --comprehensive\n");

  println!("5. Skip mind maps generation:");
  println!("   paper-assistant process 2312.07104 --no-mindmaps\n");

  println!("6. Limit sections for mind mapping:");
  println!("   paper-assistant process 2312.07104 --max-sections 5\n");

  println!("7. Use custom configuration file:");
  println!("   paper-assistant process 2312.07104 -c my-config.json\n");

  println!("8. Create custom configuration:");
  println!("   paper-assistant config create -t comprehensive -o my-config.json\n");

  println!("9. Show default configuration:");
  println!("   paper-assistant config show\n");

  println!("=== Environment Variables ===");
  println!("QWEN_TURBO_MODEL      - Override Qwen turbo model name");
  println!("QWEN_IMAGE_MODEL      - Override Qwen image model name");
  println!("PAPER_ASSISTANT_OUTPUT_DIR - Default output directory");
  println!("PAPER_ASSISTANT_TEMPERATURE - LLM temperature (0.0-2.0)");
  println!("PAPER_ASSISTANT_MAX_TOKENS  - Maximum tokens per request");
  println!("DASHSCOPE_API_KEY     - Required for Qwen models");
  println!("RUST_LOG             - Set to 'debug' for verbose logging\n");

  println!("=== Supported arXiv URL Formats ===");
  println!("- https://arxiv.org/abs/2312.07104");
  println!("- https://arxiv.org/abs/2312.07104v2");
  println!("- https://arxiv.org/pdf/2312.07104.pdf");
  println!("- 2312.07104");
  println!("- 2312.07104v2");
}

/// Print application banner
#[allow(dead_code)]
fn print_banner() {
  println!(r#"
 ____                        _                _     _              _   
|  _ \ __ _ _ __   ___ _ __   / \   ___ ___(_)___| |_ __ _ _ __ | |_ 
| |_) / _` | '_ \ / _ \ '__| / _ \ / __/ __| / __| __/ _` | '_ \| __|
|  __/ (_| | |_) |  __/ |   / ___ \\__ \__ \ \__ \ || (_| | | | | |_ 
|_|   \__,_| .__/ \___|_|  /_/   \_\___/___/_|___/\__\__,_|_| |_|\__|
           |_|                                                      

AI Agent for arXiv Paper Processing with Chinese Translation & Mind Mapping
Version 0.1.0
"#);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_cli_app_creation() {
    let app = create_cli_app();
    assert_eq!(app.get_name(), "paper-assistant");
    
    // Test that required subcommands exist
    let subcommands: Vec<&str> = app.get_subcommands()
      .map(|cmd| cmd.get_name())
      .collect();
    
    assert!(subcommands.contains(&"process"));
    assert!(subcommands.contains(&"config"));
    assert!(subcommands.contains(&"examples"));
  }

  #[tokio::test]
  async fn test_config_validation() {
    let config = PaperAssistantConfig::default();
    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_config_types() {
    let fast = PaperAssistantConfig::fast_processing();
    assert!(!fast.enable_poster_generation);
    assert_eq!(fast.max_sections_for_mind_maps, Some(5));

    let comprehensive = PaperAssistantConfig::comprehensive_analysis();
    assert!(comprehensive.enable_poster_generation);
    assert!(comprehensive.enable_mind_maps);
    assert_eq!(comprehensive.max_sections_for_mind_maps, Some(15));
  }
}