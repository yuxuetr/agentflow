//! Basic usage example for Paper Assistant
//!
//! This example demonstrates how to use the Paper Assistant library
//! programmatically in Rust code.

use anyhow::Result;
use paper_assistant::{PaperAssistant, PaperAssistantConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    println!("Paper Assistant - Basic Usage Example");
    println!("=====================================\n");

    // Example 1: Using default configuration
    println!("Example 1: Default configuration");
    example_default_config().await?;

    println!("\n" + &"=".repeat(50) + "\n");

    // Example 2: Using fast processing mode
    println!("Example 2: Fast processing mode");
    example_fast_processing().await?;

    println!("\n" + &"=".repeat(50) + "\n");

    // Example 3: Using custom configuration
    println!("Example 3: Custom configuration");
    example_custom_config().await?;

    Ok(())
}

/// Example using default configuration
async fn example_default_config() -> Result<()> {
    // Create Paper Assistant with default settings
    let mut assistant = PaperAssistant::new()?;

    // Example arXiv URL - replace with actual paper
    let arxiv_url = "https://arxiv.org/abs/2312.07104";
    
    println!("Processing paper: {}", arxiv_url);
    println!("Configuration: Default settings");
    println!("  - Model: qwen-turbo");
    println!("  - Temperature: 0.3"); 
    println!("  - Max tokens: 4000");
    println!("  - Mind maps: enabled");
    println!("  - Poster generation: enabled");

    // Note: This is just an example - actual processing would require API keys
    println!("\n[EXAMPLE ONLY - Would process paper with default settings]");

    // In real usage:
    // let result = assistant.process_paper(arxiv_url).await?;
    // assistant.save_results(&result, "./example_output").await?;

    Ok(())
}

/// Example using fast processing mode
async fn example_fast_processing() -> Result<()> {
    // Create configuration for fast processing
    let config = PaperAssistantConfig::fast_processing();
    let mut assistant = PaperAssistant::with_config(config)?;

    let arxiv_url = "2312.07104";
    
    println!("Processing paper: {}", arxiv_url);
    println!("Configuration: Fast processing mode");
    println!("  - Model: qwen-turbo");
    println!("  - Temperature: 0.1 (more focused)");
    println!("  - Max tokens: 2000 (reduced)");
    println!("  - Max sections: 5 (limited)");
    println!("  - Poster generation: disabled");

    println!("\n[EXAMPLE ONLY - Would process paper in fast mode]");

    // In real usage:
    // let result = assistant.process_paper(arxiv_url).await?;
    // println!("Processing completed in {}ms", result.processing_time_ms);

    Ok(())
}

/// Example using custom configuration
async fn example_custom_config() -> Result<()> {
    // Create custom configuration
    let config = PaperAssistantConfig {
        qwen_turbo_model: "qwen-plus".to_string(), // Higher quality model
        qwen_image_model: "qwen-vl-max".to_string(), // Higher quality image model
        temperature: Some(0.2), // Lower temperature for more focused output
        max_tokens: Some(6000), // More tokens for detailed analysis
        output_directory: "./custom_paper_output".to_string(),
        enable_mind_maps: true,
        enable_poster_generation: true,
        max_sections_for_mind_maps: Some(8),
        
        // Custom Chinese summary prompt
        chinese_summary_prompt: r#"请仔细分析以下学术论文，生成一个专业的中文摘要，重点关注：

1. 研究问题和背景
2. 创新方法和技术贡献  
3. 实验验证和结果分析
4. 学术价值和应用前景

论文内容：
{{paper_content}}

请生成约600字的专业中文摘要："#.to_string(),

        // Use all other defaults
        ..Default::default()
    };

    // Validate the custom configuration
    config.validate().map_err(|e| anyhow::anyhow!("Config validation failed: {}", e))?;

    let mut assistant = PaperAssistant::with_config(config)?;

    let arxiv_url = "https://arxiv.org/pdf/2312.07104.pdf";
    
    println!("Processing paper: {}", arxiv_url);
    println!("Configuration: Custom settings");
    println!("  - Model: qwen-plus (higher quality)");
    println!("  - Temperature: 0.2");
    println!("  - Max tokens: 6000"); 
    println!("  - Max sections: 8");
    println!("  - Custom summary prompt");
    println!("  - Output directory: ./custom_paper_output");

    println!("\n[EXAMPLE ONLY - Would process paper with custom settings]");

    // In real usage:
    // let result = assistant.process_paper(arxiv_url).await?;
    // assistant.save_results(&result, &assistant.config().output_directory).await?;
    // 
    // println!("Results saved to: {}", assistant.config().output_directory);
    // println!("Chinese summary length: {} chars", result.chinese_summary.len());
    // println!("Mind maps generated: {}", result.mind_maps.len());

    Ok(())
}

/// Example of processing multiple papers
#[allow(dead_code)]
async fn example_batch_processing() -> Result<()> {
    let config = PaperAssistantConfig::fast_processing();
    
    let paper_urls = vec![
        "https://arxiv.org/abs/2312.07104",
        "https://arxiv.org/abs/2311.12345", 
        "https://arxiv.org/abs/2310.54321",
    ];

    println!("Batch processing {} papers", paper_urls.len());

    for (i, url) in paper_urls.iter().enumerate() {
        println!("\nProcessing paper {} of {}: {}", i + 1, paper_urls.len(), url);
        
        // Create new assistant for each paper to ensure clean state
        let mut assistant = PaperAssistant::with_config(config.clone())?;
        
        // Create unique output directory for each paper
        let output_dir = format!("./batch_output/paper_{:02d}", i + 1);
        let mut custom_config = config.clone();
        custom_config.output_directory = output_dir.clone();
        
        println!("Output directory: {}", output_dir);
        println!("[EXAMPLE ONLY - Would process paper]");
        
        // In real usage:
        // let result = assistant.process_paper(url).await?;
        // assistant.save_results(&result, &output_dir).await?;
        // println!("Completed in {}ms", result.processing_time_ms);
    }

    Ok(())
}

/// Example of custom prompt engineering
#[allow(dead_code)]
async fn example_custom_prompts() -> Result<()> {
    let config = PaperAssistantConfig::default()
        .with_custom_prompts(
            // Custom summary prompt focused on technical details
            Some(r#"请分析以下技术论文，重点提取：
1. 核心技术创新点
2. 算法原理和数学模型
3. 实验设计和评估指标
4. 性能对比和优势分析
5. 局限性和未来改进方向

论文内容：{{paper_content}}

生成技术导向的中文摘要（约500字）："#.to_string()),
            
            // Custom translation prompt with formatting preservation
            Some(r#"请将以下学术论文翻译为高质量的中文：

翻译要求：
- 保持原文的逻辑结构和段落格式
- 专业术语使用准确的中文对应词汇
- 数学公式和符号保持原样
- 重要英文术语可在中文后标注 (English term)
- 确保语言流畅，符合中文学术写作习惯

原文：{{paper_content}}

中文翻译："#.to_string()),
            
            None, // Keep default section extraction prompt
            None, // Keep default poster prompt
        );

    let mut assistant = PaperAssistant::with_config(config)?;

    println!("Custom prompts configured:");
    println!("- Technical summary focused on innovation and algorithms");
    println!("- Translation with format preservation and terminology handling");
    println!("[EXAMPLE ONLY - Would use custom prompts for processing]");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_examples_compile() {
        // Test that all examples compile and run without panicking
        assert!(example_default_config().await.is_ok());
        assert!(example_fast_processing().await.is_ok());
        assert!(example_custom_config().await.is_ok());
    }

    #[test]
    fn test_config_creation() {
        let config = PaperAssistantConfig::fast_processing();
        assert!(!config.enable_poster_generation);
        
        let config = PaperAssistantConfig::comprehensive_analysis();
        assert!(config.enable_mind_maps);
        assert!(config.enable_poster_generation);
    }
}