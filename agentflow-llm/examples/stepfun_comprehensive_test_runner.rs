/// StepFun Comprehensive Test Runner
/// 
/// Runs all StepFun API test examples in sequence with proper error handling,
/// performance monitoring, and comprehensive reporting.
/// 
/// Usage:
/// ```bash
/// export STEP_API_KEY="your-stepfun-api-key-here"
/// cargo run --example stepfun_comprehensive_test_runner
/// ```
/// 
/// Options:
/// ```bash
/// # Run with verbose logging
/// RUST_LOG=debug cargo run --example stepfun_comprehensive_test_runner
/// 
/// # Run specific test categories only
/// cargo run --example stepfun_comprehensive_test_runner -- --categories text,image
/// 
/// # Run with performance benchmarking
/// cargo run --example stepfun_comprehensive_test_runner -- --benchmark
/// ```

use std::env;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::process::Command;
use tokio::fs;

#[derive(Debug, Clone)]
struct TestResult {
    name: String,
    category: String,
    duration: Duration,
    success: bool,
    error_message: Option<String>,
    output_files: Vec<String>,
}

#[derive(Debug, Clone)]
struct TestCategory {
    name: String,
    description: String,
    examples: Vec<String>,
    total_tests: usize,
    passed_tests: usize,
    total_duration: Duration,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    // Check for API key
    let api_key = env::var("STEP_API_KEY")
        .expect("STEP_API_KEY environment variable is required");

    println!("🚀 StepFun Comprehensive Test Runner");
    println!("=====================================\n");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let run_benchmark = args.iter().any(|arg| arg == "--benchmark");
    let selected_categories = parse_categories(&args);

    // Initialize test runner
    let mut test_runner = TestRunner::new(api_key, run_benchmark);
    
    // Run all tests
    let results = test_runner.run_all_tests(selected_categories).await?;
    
    // Generate comprehensive report
    generate_final_report(&results).await?;
    
    // Print summary
    print_test_summary(&results);
    
    // Exit with appropriate code
    let total_failures = results.iter().filter(|r| !r.success).count();
    if total_failures > 0 {
        std::process::exit(1);
    }
    
    Ok(())
}

struct TestRunner {
    api_key: String,
    benchmark_mode: bool,
    start_time: Instant,
}

impl TestRunner {
    fn new(api_key: String, benchmark_mode: bool) -> Self {
        Self {
            api_key,
            benchmark_mode,
            start_time: Instant::now(),
        }
    }

    async fn run_all_tests(&mut self, selected_categories: Option<Vec<String>>) -> Result<Vec<TestResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();
        
        // Define test categories
        let categories = self.define_test_categories();
        
        for category in categories {
            if let Some(ref selected) = selected_categories {
                if !selected.contains(&category.name) {
                    println!("⏭️  Skipping category: {}", category.name);
                    continue;
                }
            }
            
            println!("📂 Running {} Tests", category.name);
            println!("   {}", category.description);
            println!("   Examples: {}", category.examples.len());
            println!();
            
            for example in &category.examples {
                let result = self.run_single_test(example, &category.name).await;
                results.push(result);
                
                // Short pause between tests to avoid rate limiting
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            
            println!(); // Empty line between categories
        }
        
        Ok(results)
    }

    fn define_test_categories(&self) -> Vec<TestCategory> {
        vec![
            TestCategory {
                name: "Text".to_string(),
                description: "Text completion models with streaming and non-streaming modes".to_string(),
                examples: vec!["stepfun_text_models".to_string()],
                total_tests: 0,
                passed_tests: 0,
                total_duration: Duration::ZERO,
            },
            TestCategory {
                name: "Image Understanding".to_string(),
                description: "Vision models with multimodal image analysis capabilities".to_string(),
                examples: vec!["stepfun_image_understanding".to_string()],
                total_tests: 0,
                passed_tests: 0,
                total_duration: Duration::ZERO,
            },
            TestCategory {
                name: "TTS".to_string(),
                description: "Text-to-Speech synthesis with voice customization".to_string(),
                examples: vec!["stepfun_tts_models".to_string()],
                total_tests: 0,
                passed_tests: 0,
                total_duration: Duration::ZERO,
            },
            TestCategory {
                name: "ASR".to_string(),
                description: "Automatic Speech Recognition with multiple output formats".to_string(),
                examples: vec!["stepfun_asr_models".to_string()],
                total_tests: 0,
                passed_tests: 0,
                total_duration: Duration::ZERO,
            },
            TestCategory {
                name: "Image Generation".to_string(),
                description: "Text-to-image generation and editing capabilities".to_string(),
                examples: vec!["stepfun_image_generation".to_string()],
                total_tests: 0,
                passed_tests: 0,
                total_duration: Duration::ZERO,
            },
        ]
    }

    async fn run_single_test(&self, example_name: &str, category: &str) -> TestResult {
        println!("🧪 Testing: {}", example_name);
        let start_time = Instant::now();
        
        // Set environment variable for the test
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--example", example_name])
           .env("STEP_API_KEY", &self.api_key)
           .env("RUST_LOG", "info");

        if self.benchmark_mode {
            cmd.env("STEPFUN_BENCHMARK_MODE", "1");
        }

        println!("   📤 Executing: cargo run --example {}", example_name);
        let output = cmd.output().await;
        
        let duration = start_time.elapsed();
        
        match output {
            Ok(result) => {
                let success = result.status.success();
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                
                if success {
                    println!("   ✅ Completed in {:?}", duration);
                    if self.benchmark_mode {
                        self.extract_performance_metrics(&stdout);
                    }
                } else {
                    println!("   ❌ Failed in {:?}", duration);
                    if !stderr.is_empty() {
                        println!("   Error: {}", stderr.trim());
                    }
                }
                
                // Find output files generated by the test
                let output_files = self.find_generated_files(example_name).await;
                if !output_files.is_empty() {
                    println!("   📁 Generated {} output files", output_files.len());
                    for file in &output_files {
                        if let Ok(metadata) = fs::metadata(file).await {
                            println!("     - {} ({} bytes)", file, metadata.len());
                        }
                    }
                }
                
                TestResult {
                    name: example_name.to_string(),
                    category: category.to_string(),
                    duration,
                    success,
                    error_message: if success { None } else { Some(stderr.to_string()) },
                    output_files,
                }
            }
            Err(e) => {
                println!("   ❌ Execution failed: {}", e);
                TestResult {
                    name: example_name.to_string(),
                    category: category.to_string(),
                    duration,
                    success: false,
                    error_message: Some(e.to_string()),
                    output_files: Vec::new(),
                }
            }
        }
    }

    fn extract_performance_metrics(&self, output: &str) {
        // Extract performance metrics from test output
        println!("   📊 Performance metrics:");
        
        // Look for timing information
        for line in output.lines() {
            if line.contains("completed in") || line.contains("received in") {
                println!("     {}", line.trim());
            }
        }
        
        // Look for token usage
        for line in output.lines() {
            if line.contains("tokens:") || line.contains("Token usage") {
                println!("     {}", line.trim());
            }
        }
        
        // Look for file sizes
        for line in output.lines() {
            if line.contains("bytes") && (line.contains("Audio") || line.contains("Image")) {
                println!("     {}", line.trim());
            }
        }
    }

    async fn find_generated_files(&self, example_name: &str) -> Vec<String> {
        let mut files = Vec::new();
        
        // Common file patterns generated by tests
        let patterns = match example_name {
            "stepfun_text_models" => vec![], // Text models don't generate files
            "stepfun_image_understanding" => vec![], // Image understanding doesn't generate files
            "stepfun_tts_models" => vec![
                "stepfun_tts_basic.mp3",
                "stepfun_tts_mini_fast.wav", 
                "stepfun_tts_emotional.mp3",
                "stepfun_tts_cantonese.mp3",
                "stepfun_tts_sichuan.mp3",
                "stepfun_tts_advanced.opus",
            ],
            "stepfun_asr_models" => vec![
                "sample_audio.wav",
                "sample_audio.mp3",
                "transcription.srt",
                "transcription.vtt",
            ],
            "stepfun_image_generation" => vec![
                "stepfun_generated_basic_1.png",
                "stepfun_generated_quick_1.png",
                "stepfun_generated_advanced_1.png",
                "stepfun_generated_style_reference_1.png",
                "stepfun_edited_image_1.png",
                "base_image_for_edit.png",
            ],
            _ => vec![],
        };

        for pattern in patterns {
            if fs::metadata(pattern).await.is_ok() {
                files.push(pattern.to_string());
            }
        }

        // Also look for any files with the example name prefix
        if let Ok(mut entries) = fs::read_dir(".").await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.starts_with(&format!("{}_", example_name)) || 
                       name.starts_with("stepfun_") {
                        if !files.contains(&name) {
                            files.push(name);
                        }
                    }
                }
            }
        }

        files
    }
}

fn parse_categories(args: &[String]) -> Option<Vec<String>> {
    for i in 0..args.len() {
        if args[i] == "--categories" && i + 1 < args.len() {
            return Some(
                args[i + 1]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            );
        }
    }
    None
}

async fn generate_final_report(results: &[TestResult]) -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 Generating comprehensive test report...");
    
    let mut report = String::new();
    report.push_str("# StepFun API Test Results\n\n");
    report.push_str(&format!("**Test Date:** {}\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    report.push_str(&format!("**Total Tests:** {}\n", results.len()));
    
    let passed = results.iter().filter(|r| r.success).count();
    let failed = results.len() - passed;
    report.push_str(&format!("**Passed:** {}\n", passed));
    report.push_str(&format!("**Failed:** {}\n", failed));
    report.push_str(&format!("**Success Rate:** {:.1}%\n\n", passed as f32 / results.len() as f32 * 100.0));

    // Group results by category
    let mut categories: HashMap<String, Vec<&TestResult>> = HashMap::new();
    for result in results {
        categories.entry(result.category.clone()).or_insert_with(Vec::new).push(result);
    }

    report.push_str("## Test Results by Category\n\n");
    
    for (category, tests) in categories {
        let category_passed = tests.iter().filter(|r| r.success).count();
        let category_total = tests.len();
        let category_duration: Duration = tests.iter().map(|r| r.duration).sum();
        
        report.push_str(&format!("### {} ({}/{})\n\n", category, category_passed, category_total));
        
        for test in tests {
            let status = if test.success { "✅" } else { "❌" };
            report.push_str(&format!("- {} **{}** - {:?}\n", status, test.name, test.duration));
            
            if !test.output_files.is_empty() {
                report.push_str(&format!("  - Generated {} files\n", test.output_files.len()));
            }
            
            if let Some(ref error) = test.error_message {
                if !error.trim().is_empty() {
                    report.push_str(&format!("  - Error: {}\n", error.lines().next().unwrap_or("Unknown error")));
                }
            }
        }
        
        report.push_str(&format!("\n**Category Duration:** {:?}\n\n", category_duration));
    }

    // Performance summary
    report.push_str("## Performance Summary\n\n");
    let total_duration: Duration = results.iter().map(|r| r.duration).sum();
    let avg_duration = total_duration / results.len() as u32;
    
    report.push_str(&format!("- **Total Test Time:** {:?}\n", total_duration));
    report.push_str(&format!("- **Average Test Time:** {:?}\n", avg_duration));
    
    if let Some(slowest) = results.iter().max_by_key(|r| r.duration) {
        report.push_str(&format!("- **Slowest Test:** {} ({:?})\n", slowest.name, slowest.duration));
    }
    
    if let Some(fastest) = results.iter().filter(|r| r.success).min_by_key(|r| r.duration) {
        report.push_str(&format!("- **Fastest Test:** {} ({:?})\n", fastest.name, fastest.duration));
    }

    // Output file summary
    let total_files: usize = results.iter().map(|r| r.output_files.len()).sum();
    if total_files > 0 {
        report.push_str(&format!("\n- **Total Files Generated:** {}\n", total_files));
    }

    // Save report
    fs::write("stepfun_test_report.md", report).await?;
    println!("📄 Report saved to: stepfun_test_report.md");
    
    Ok(())
}

fn print_test_summary(results: &[TestResult]) {
    let total = results.len();
    let passed = results.iter().filter(|r| r.success).count();
    let failed = total - passed;
    let total_duration: Duration = results.iter().map(|r| r.duration).sum();

    println!("\n🎯 Final Test Summary");
    println!("=====================");
    println!("📊 Results:");
    println!("   Total tests: {}", total);
    println!("   Passed: {} ✅", passed);
    println!("   Failed: {} ❌", failed);
    println!("   Success rate: {:.1}%", passed as f32 / total as f32 * 100.0);
    println!("   Total duration: {:?}", total_duration);

    if failed > 0 {
        println!("\n❌ Failed tests:");
        for result in results.iter().filter(|r| !r.success) {
            println!("   - {}: {}", result.name, 
                result.error_message.as_ref().map(|e| e.lines().next().unwrap_or("Unknown error")).unwrap_or("Unknown error"));
        }
    }

    // Performance insights
    println!("\n⚡ Performance insights:");
    if let Some(slowest) = results.iter().max_by_key(|r| r.duration) {
        println!("   Slowest: {} ({:?})", slowest.name, slowest.duration);
    }
    if let Some(fastest) = results.iter().filter(|r| r.success).min_by_key(|r| r.duration) {
        println!("   Fastest: {} ({:?})", fastest.name, fastest.duration);
    }

    // Category breakdown
    let mut categories: HashMap<String, (usize, usize)> = HashMap::new();
    for result in results {
        let entry = categories.entry(result.category.clone()).or_insert((0, 0));
        entry.1 += 1; // total
        if result.success {
            entry.0 += 1; // passed
        }
    }

    println!("\n📂 Category breakdown:");
    for (category, (passed, total)) in categories {
        println!("   {}: {}/{} ({:.0}%)", category, passed, total, passed as f32 / total as f32 * 100.0);
    }

    // Output files summary
    let total_files: usize = results.iter().map(|r| r.output_files.len()).sum();
    if total_files > 0 {
        println!("\n📁 Generated {} output files across all tests", total_files);
    }

    println!("\n🏁 Test execution complete!");
    if failed == 0 {
        println!("🎉 All tests passed successfully!");
    } else {
        println!("⚠️  {} test(s) failed - check the logs above for details", failed);
    }
}