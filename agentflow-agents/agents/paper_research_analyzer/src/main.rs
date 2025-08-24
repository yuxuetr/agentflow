//! Paper Research Analyzer - Standalone Agent Binary
//!
//! A comprehensive PDF research paper analysis agent built with AgentFlow.

use clap::{Parser, Subcommand};
use paper_research_analyzer::{PDFAnalyzer, AnalysisDepth};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "paper-research-analyzer")]
#[command(about = "Analyze PDF research papers using AI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a single PDF research paper
    Analyze {
        /// Path to PDF file
        #[arg(short, long)]
        pdf: PathBuf,
        
        /// Output directory
        #[arg(short, long, default_value = "./analysis_output")]
        output: PathBuf,
        
        /// Analysis depth
        #[arg(short = 'd', long, default_value = "comprehensive")]
        depth: String,
        
        /// Target language for translation
        #[arg(short = 'l', long, default_value = "zh")]
        language: String,
        
        /// Model to use
        #[arg(short, long, default_value = "qwen-turbo")]
        model: String,
        
        /// Generate mind map
        #[arg(long, default_value = "true")]
        mind_map: bool,
    },
    
    /// Batch analyze multiple PDFs in a directory
    Batch {
        /// Directory containing PDF files
        #[arg(short, long)]
        directory: PathBuf,
        
        /// Output directory
        #[arg(short, long, default_value = "./batch_analysis_output")]
        output: PathBuf,
        
        /// Analysis depth
        #[arg(short = 'd', long, default_value = "summary")]
        depth: String,
        
        /// Model to use
        #[arg(short, long, default_value = "qwen-turbo")]
        model: String,
        
        /// Concurrency limit
        #[arg(short, long, default_value = "3")]
        concurrency: usize,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Get API key from environment (use STEP_API_KEY for PDF parsing)
    let api_key = std::env::var("STEP_API_KEY")
        .map_err(|_| "STEP_API_KEY environment variable not set")?;

    match &cli.command {
        Commands::Analyze { 
            pdf, 
            output, 
            depth, 
            language, 
            model, 
            mind_map 
        } => {
            let analysis_depth = parse_analysis_depth(depth)?;
            
            let analyzer = PDFAnalyzer::new(api_key)
                .analysis_depth(analysis_depth)
                .target_language(language)
                .model(model)
                .generate_mind_map(*mind_map);

            println!("🚀 Starting PDF Research Paper Analysis");
            println!("📄 File: {}", pdf.display());
            println!("🎯 Depth: {}", depth);
            println!("🌍 Language: {}", language);
            println!("🤖 Model: {}", model);

            match analyzer.analyze_paper(pdf).await {
                Ok(result) => {
                    println!("✅ Analysis completed successfully!");
                    if let Err(e) = result.save_to_files(output).await {
                        eprintln!("❌ Failed to save results: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Analysis failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        
        Commands::Batch { 
            directory, 
            output, 
            depth, 
            model, 
            concurrency 
        } => {
            let analysis_depth = parse_analysis_depth(depth)?;
            
            let analyzer = PDFAnalyzer::new(api_key)
                .analysis_depth(analysis_depth)
                .model(model);

            println!("🔄 Starting batch analysis...");
            println!("📁 Directory: {}", directory.display());
            println!("🎯 Depth: {}", depth);
            println!("🤖 Model: {}", model);
            println!("⚡ Concurrency: {}", concurrency);

            match analyzer.analyze_batch(directory).await {
                Ok(batch_result) => {
                    println!("✅ Batch analysis completed!");
                    println!("📊 Processed: {} papers", batch_result.total_processed);
                    println!("✅ Successful: {} papers", batch_result.successful_analyses.len());
                    println!("❌ Failed: {} papers", batch_result.failed_analyses.len());
                    
                    if !batch_result.failed_analyses.is_empty() {
                        println!("\n❌ Failed files:");
                        for (path, error) in &batch_result.failed_analyses {
                            println!("  - {}: {}", path.display(), error);
                        }
                    }
                    
                    if let Err(e) = batch_result.save_to_directory(output).await {
                        eprintln!("❌ Failed to save batch results: {}", e);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Batch analysis failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn parse_analysis_depth(depth: &str) -> Result<AnalysisDepth, Box<dyn std::error::Error>> {
    match depth.to_lowercase().as_str() {
        "summary" => Ok(AnalysisDepth::Summary),
        "insights" => Ok(AnalysisDepth::Insights),
        "comprehensive" => Ok(AnalysisDepth::Comprehensive),
        "translation" | "with-translation" => Ok(AnalysisDepth::WithTranslation),
        _ => Err(format!("Invalid analysis depth: {}. Valid options: summary, insights, comprehensive, translation", depth).into()),
    }
}