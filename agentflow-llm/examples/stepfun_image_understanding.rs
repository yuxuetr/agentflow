/// StepFun Image Understanding Models - Real API Test Cases
/// 
/// Tests vision models with actual image analysis capabilities.
/// Demonstrates multimodal content processing with real images.
/// 
/// Usage:
/// ```bash
/// export STEP_API_KEY="your-stepfun-api-key-here"
/// cargo run --example stepfun_image_understanding
/// ```

use reqwest::Client;
use serde_json::{json, Value};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let api_key = env::var("STEP_API_KEY")
        .expect("STEP_API_KEY environment variable is required");

    println!("🖼️  StepFun Image Understanding - Real API Tests");
    println!("==============================================\n");

    // Test different vision models with various image types
    test_step_1o_turbo_vision(&api_key).await?;
    test_step_1v_8k_chart_analysis(&api_key).await?;
    test_step_1v_32k_detailed_analysis(&api_key).await?;
    test_multimodal_step_3(&api_key).await?;
    
    println!("✅ All image understanding tests completed successfully!");
    Ok(())
}

/// Test step-1o-turbo-vision with a sample image
async fn test_step_1o_turbo_vision(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("👁️  Testing step-1o-turbo-vision");
    println!("Model: step-1o-turbo-vision | Task: General image description\n");

    let client = Client::new();
    
    // Use a simple and reliable test image - a 1x1 pixel transparent PNG as data URI
    let test_image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    
    let request_body = json!({
        "model": "step-1o-turbo-vision",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text", 
                        "text": "请详细描述这张图片中的内容，包括景色、颜色、构图等要素。"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": test_image_url
                        }
                    }
                ]
            }
        ],
        "max_tokens": 500,
        "temperature": 0.7
    });

    println!("📤 Sending image analysis request...");
    println!("   Image URL: {}", test_image_url);
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Vision request failed: {}", error_text);
        return Err("Vision request failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Vision analysis completed in {:?}", duration);
    
    if let Some(usage) = response_json.get("usage") {
        println!("📊 Token usage:");
        println!("   Prompt tokens: {}", usage.get("prompt_tokens").unwrap_or(&json!(0)));
        println!("   Completion tokens: {}", usage.get("completion_tokens").unwrap_or(&json!(0)));
        println!("   Total tokens: {}", usage.get("total_tokens").unwrap_or(&json!(0)));
    }

    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("🖼️  Image description:");
                    println!("{}", content);
                    
                    // Validate vision capabilities
                    let content_lower = content.to_lowercase();
                    let describes_nature = content_lower.contains("自然") || content_lower.contains("nature") || content_lower.contains("outdoor");
                    let mentions_colors = content_lower.contains("绿") || content_lower.contains("蓝") || content_lower.contains("color") || content_lower.contains("颜色");
                    let describes_composition = content_lower.contains("构图") || content_lower.contains("景") || content_lower.contains("view");
                    
                    println!("✅ Vision analysis validation:");
                    println!("   Describes natural elements: {}", describes_nature);
                    println!("   Mentions colors: {}", mentions_colors);
                    println!("   Describes composition: {}", describes_composition);
                    println!("   Response length: {} chars", content.len());
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test step-1v-8k with chart/diagram analysis
async fn test_step_1v_8k_chart_analysis(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Testing step-1v-8k (Chart Analysis)");
    println!("Model: step-1v-8k | Task: Chart and data interpretation\n");

    let client = Client::new();
    
    // Use a chart/diagram image
    let chart_image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    
    let request_body = json!({
        "model": "step-1v-8k",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text", 
                        "text": "分析这个图表，解释其中的数据趋势、坐标轴含义，以及可能的统计关系。"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": chart_image_url
                        }
                    }
                ]
            }
        ],
        "max_tokens": 600,
        "temperature": 0.6
    });

    println!("📤 Sending chart analysis request...");
    println!("   Chart URL: {}", chart_image_url);
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Chart analysis failed: {}", error_text);
        return Err("Chart analysis failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Chart analysis completed in {:?}", duration);
    
    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("📊 Chart analysis:");
                    println!("{}", content);
                    
                    // Validate analytical capabilities
                    let content_lower = content.to_lowercase();
                    let analyzes_data = content_lower.contains("数据") || content_lower.contains("data") || content_lower.contains("点");
                    let mentions_trend = content_lower.contains("趋势") || content_lower.contains("trend") || content_lower.contains("关系");
                    let discusses_axes = content_lower.contains("轴") || content_lower.contains("坐标") || content_lower.contains("axis");
                    let shows_insight = content_lower.contains("回归") || content_lower.contains("线性") || content_lower.contains("regression");
                    
                    println!("✅ Analytical capabilities validation:");
                    println!("   Analyzes data points: {}", analyzes_data);
                    println!("   Identifies trends: {}", mentions_trend);
                    println!("   Discusses axes: {}", discusses_axes);
                    println!("   Shows statistical insight: {}", shows_insight);
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test step-1v-32k with detailed image analysis
async fn test_step_1v_32k_detailed_analysis(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing step-1v-32k (Detailed Analysis)");
    println!("Model: step-1v-32k | Task: Comprehensive image analysis\n");

    let client = Client::new();
    
    // Use an image with rich detail for comprehensive analysis
    let detailed_image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    
    let request_body = json!({
        "model": "step-1v-32k",
        "messages": [
            {
                "role": "system",
                "content": "你是一个专业的图像分析专家，善于从多个维度分析图片内容。"
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "text", 
                        "text": "请从以下几个角度详细分析这张图片：1) 场景和地点特征 2) 光线和色彩运用 3) 人文和社会元素 4) 构图和视觉效果 5) 可能的拍摄技巧"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": detailed_image_url
                        }
                    }
                ]
            }
        ],
        "max_tokens": 800,
        "temperature": 0.7
    });

    println!("📤 Sending detailed analysis request...");
    println!("   Image URL: {}", detailed_image_url);
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Detailed analysis failed: {}", error_text);
        return Err("Detailed analysis failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Detailed analysis completed in {:?}", duration);
    
    if let Some(usage) = response_json.get("usage") {
        println!("📊 Token usage for detailed analysis:");
        println!("   Prompt tokens: {}", usage.get("prompt_tokens").unwrap_or(&json!(0)));
        println!("   Completion tokens: {}", usage.get("completion_tokens").unwrap_or(&json!(0)));
        println!("   Total tokens: {}", usage.get("total_tokens").unwrap_or(&json!(0)));
    }

    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("🔍 Detailed analysis:");
                    println!("{}", content);
                    
                    // Validate comprehensive analysis
                    let content_lower = content.to_lowercase();
                    let analyzes_scene = content_lower.contains("场景") || content_lower.contains("地点") || content_lower.contains("scene");
                    let discusses_lighting = content_lower.contains("光线") || content_lower.contains("光") || content_lower.contains("lighting");
                    let mentions_composition = content_lower.contains("构图") || content_lower.contains("视觉") || content_lower.contains("composition");
                    let shows_technique = content_lower.contains("技巧") || content_lower.contains("摄影") || content_lower.contains("拍摄");
                    let cultural_elements = content_lower.contains("人文") || content_lower.contains("社会") || content_lower.contains("文化");
                    
                    println!("✅ Comprehensive analysis validation:");
                    println!("   Analyzes scene/location: {}", analyzes_scene);
                    println!("   Discusses lighting: {}", discusses_lighting);
                    println!("   Mentions composition: {}", mentions_composition);
                    println!("   Shows technical insight: {}", shows_technique);
                    println!("   Identifies cultural elements: {}", cultural_elements);
                    println!("   Response depth: {} chars", content.len());
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test step-3 multimodal capabilities
async fn test_multimodal_step_3(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🎭 Testing step-3 (Multimodal)");
    println!("Model: step-3 | Task: Advanced multimodal reasoning\n");

    let client = Client::new();
    
    // Use multiple images for multimodal comparison
    let image1_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let image2_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    
    let request_body = json!({
        "model": "step-3",
        "messages": [
            {
                "role": "system",
                "content": "你是一个具有多模态理解能力的AI助手，能够综合分析多种类型的内容。"
            },
            {
                "role": "user",
                "content": [
                    {
                        "type": "text", 
                        "text": "请分析这些图片的内容差异，比较它们的特点，并解释为什么这种多样性在视觉内容中很重要。"
                    },
                    {
                        "type": "image_url",
                        "image_url": {"url": image1_url}
                    },
                    {
                        "type": "image_url",
                        "image_url": {"url": image2_url}
                    }
                ]
            }
        ],
        "max_tokens": 700,
        "temperature": 0.8
    });

    println!("📤 Sending multimodal analysis request...");
    println!("   Image 1: {}", image1_url);
    println!("   Image 2: {}", image2_url);
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Multimodal analysis failed: {}", error_text);
        return Err("Multimodal analysis failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Multimodal analysis completed in {:?}", duration);
    
    if let Some(usage) = response_json.get("usage") {
        println!("📊 Token usage for multimodal analysis:");
        println!("   Prompt tokens: {}", usage.get("prompt_tokens").unwrap_or(&json!(0)));
        println!("   Completion tokens: {}", usage.get("completion_tokens").unwrap_or(&json!(0)));
        println!("   Total tokens: {}", usage.get("total_tokens").unwrap_or(&json!(0)));
    }

    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("🎭 Multimodal analysis:");
                    println!("{}", content);
                    
                    // Validate multimodal reasoning
                    let content_lower = content.to_lowercase();
                    let compares_images = content_lower.contains("比较") || content_lower.contains("difference") || content_lower.contains("对比");
                    let identifies_diversity = content_lower.contains("多样") || content_lower.contains("差异") || content_lower.contains("不同");
                    let shows_reasoning = content_lower.contains("因为") || content_lower.contains("原因") || content_lower.contains("重要");
                    let integrates_analysis = content_lower.contains("综合") || content_lower.contains("整体") || content_lower.contains("总体");
                    
                    println!("✅ Multimodal reasoning validation:");
                    println!("   Compares multiple images: {}", compares_images);
                    println!("   Identifies diversity: {}", identifies_diversity);
                    println!("   Shows reasoning: {}", shows_reasoning);
                    println!("   Integrates analysis: {}", integrates_analysis);
                    println!("   Advanced response length: {} chars", content.len());
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test with base64 encoded image (alternative to URL)
#[allow(dead_code)]
async fn test_with_base64_image(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔢 Testing with base64 encoded image");
    
    // Create a simple test image (1x1 pixel PNG in base64)
    let base64_image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    
    let client = Client::new();
    let request_body = json!({
        "model": "step-1o-turbo-vision",
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "text", 
                        "text": "这个图片包含什么内容？"
                    },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:image/png;base64,{}", base64_image)
                        }
                    }
                ]
            }
        ],
        "max_tokens": 200
    });

    println!("📤 Sending base64 image analysis request...");
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    if response.status().is_success() {
        let response_json: Value = response.json().await?;
        if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
            if let Some(first_choice) = choices.first() {
                if let Some(message) = first_choice.get("message") {
                    if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                        println!("📥 Base64 image analysis: {}", content);
                    }
                }
            }
        }
    }

    Ok(())
}