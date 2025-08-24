//! Paper Research Analyzer - Standalone Agent Binary
//!
//! A comprehensive PDF research paper analysis agent built with AgentFlow.

use clap::Parser;
use paper_research_analyzer::{PDFAnalyzer, AnalysisDepth};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "paper-research-analyzer")]
#[command(about = "Analyze PDF research papers using AI", long_about = None)]
struct Cli {
    /// Path to PDF file (for single analysis)
    #[arg(long = "pdf-path")]
    pdf_path: Option<PathBuf>,
    
    /// Directory containing PDF files (for batch analysis)  
    #[arg(long = "batch-dir")]
    batch_dir: Option<PathBuf>,
    
    /// Output directory
    #[arg(short, long, default_value = "./analysis_output")]
    output_dir: PathBuf,
    
    /// Analysis depth
    #[arg(short, long, default_value = "comprehensive")]
    depth: String,
    
    /// Target language for translation
    #[arg(short, long, default_value = "zh")]
    language: String,
    
    /// Model to use
    #[arg(short, long, default_value = "qwen-turbo")]
    model: String,
    
    /// Generate mind map
    #[arg(long)]
    mind_map: bool,
    
    /// Concurrency for batch processing
    #[arg(long, default_value = "3")]
    concurrency: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    
    // Get API key from environment
    let api_key = std::env::var("STEP_API_KEY")
        .or_else(|_| std::env::var("API_KEY"))
        .unwrap_or_else(|_| "demo_key".to_string());
        
    let analysis_depth = parse_analysis_depth(&args.depth)?;

    // Determine operation mode based on arguments
    if let Some(pdf_path) = args.pdf_path {
        // Single PDF analysis
        println!("🔍 Analyzing single PDF...");
        println!("📄 File: {}", pdf_path.display());
        println!("🎯 Depth: {}", args.depth);
        println!("🤖 Model: {}", args.model);
        println!("🧠 Mind Map: {}", args.mind_map);

        let mut analyzer = PDFAnalyzer::new(api_key)
            .analysis_depth(analysis_depth)
            .model(&args.model)
            .generate_mind_map(args.mind_map);
            
        if analysis_depth == AnalysisDepth::WithTranslation {
            analyzer = analyzer.target_language(&args.language);
        }

        match analyzer.analyze_paper(&pdf_path).await {
            Ok(result) => {
                println!("✅ Analysis completed successfully!");
                if let Err(e) = result.save_to_files(&args.output_dir).await {
                    eprintln!("❌ Failed to save results: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("❌ Analysis failed: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(batch_directory) = args.batch_dir {
        // Batch analysis
        println!("🔄 Starting batch analysis...");
        println!("📁 Directory: {}", batch_directory.display());
        println!("🎯 Depth: {}", args.depth);
        println!("🤖 Model: {}", args.model);
        println!("⚡ Concurrency: {}", args.concurrency);

        let analyzer = PDFAnalyzer::new(api_key)
            .analysis_depth(analysis_depth)
            .model(&args.model)
            .generate_mind_map(args.mind_map);

        match analyzer.analyze_batch(&batch_directory).await {
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
                
                if let Err(e) = batch_result.save_to_directory(&args.output_dir).await {
                    eprintln!("❌ Failed to save batch results: {}", e);
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("❌ Batch analysis failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("❌ Error: Must provide either --pdf-path for single analysis or --batch-dir for batch analysis");
        eprintln!("Usage examples:");
        eprintln!("  Single PDF:  cargo run -- --pdf-path paper.pdf --depth comprehensive --mind-map");
        eprintln!("  Batch mode:  cargo run -- --batch-dir ./papers/ --depth summary");
        std::process::exit(1);
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