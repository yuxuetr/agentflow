# AgentFlow Multimodal LLM Guide

AgentFlow now supports multimodal LLMs that can process both text and images. This guide covers how to use multimodal capabilities in your agent flows.

## Overview

Multimodal LLMs allow you to:
- Analyze images with natural language questions
- Combine multiple images with text instructions
- Process visual content in automated workflows
- Create sophisticated AI agents that understand visual context

## Supported Models

### StepFun (Primary Multimodal Provider)
- `step-1o-turbo-vision` - High-performance multimodal model
- `step-2-16k` - Advanced multimodal with extended context
- `step-1v-8k`, `step-1v-32k` - Vision-enabled models

### OpenAI (Compatible)
- `gpt-4o` - Text and image understanding
- `gpt-4o-mini` - Lightweight multimodal model

## Quick Start

### Simple Text + Image Analysis

```rust
use agentflow_llm::AgentFlow;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize AgentFlow
    AgentFlow::init().await?;
    
    // Simple image analysis - recreating the Python example
    let response = AgentFlow::model("step-1o-turbo-vision")
        .text_and_image(
            "Describe this image in elegant language",
            "https://www.stepfun.com/assets/section-1-CTe4nZiO.webp"
        )
        .temperature(0.7)
        .execute()
        .await?;
    
    println!("Analysis: {}", response);
    Ok(())
}
```

### Complex Multimodal Messages

```rust
use agentflow_llm::{AgentFlow, MultimodalMessage};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    AgentFlow::init().await?;
    
    // Create a system message (optional)
    let system_message = MultimodalMessage::system()
        .add_text("You are an expert image analyst. Provide detailed, accurate descriptions.")
        .build();
    
    // Create a user message with multiple images
    let user_message = MultimodalMessage::user()
        .add_text("Compare these architectural images and describe their similarities:")
        .add_image_url_with_detail("https://example.com/building1.jpg", "high")
        .add_image_url_with_detail("https://example.com/building2.jpg", "high")
        .build();
    
    let response = AgentFlow::model("step-1o-turbo-vision")
        .multimodal_messages(vec![system_message, user_message])
        .temperature(0.8)
        .max_tokens(1500)
        .execute()
        .await?;
    
    println!("Comparison: {}", response);
    Ok(())
}
```

## Multimodal Message Builder

The `MultimodalMessage` API provides a flexible way to construct complex messages:

### Text Content
```rust
let message = MultimodalMessage::user()
    .add_text("What do you see in this image?")
    .build();
```

### Image URLs
```rust
let message = MultimodalMessage::user()
    .add_text("Analyze this:")
    .add_image_url("https://example.com/image.jpg")
    .add_image_url_with_detail("https://example.com/detailed.jpg", "high")
    .build();
```

### Base64 Images (for local files)
```rust
let message = MultimodalMessage::user()
    .add_text("What's in this local image?")
    .add_image_data("data:image/jpeg;base64,/9j/4AAQ...", "image/jpeg")
    .build();
```

### Builder Shortcuts
```rust
// Quick text + single image
let message = MultimodalMessage::text_and_image(
    "user", 
    "Describe this", 
    "https://example.com/image.jpg"
);

// Text + multiple images
let message = MultimodalMessage::text_and_images(
    "user",
    "Compare these images:",
    vec!["https://example.com/1.jpg", "https://example.com/2.jpg"]
);
```

## Using in Agent Flows

Multimodal capabilities can be integrated into AgentFlow nodes for automated image processing workflows:

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState};
use agentflow_llm::{AgentFlow as LLMAgentFlow, MultimodalMessage};
use async_trait::async_trait;

struct ImageAnalyzerNode {
    model_name: String,
}

#[async_trait]
impl AsyncNode for ImageAnalyzerNode {
    async fn exec_async(&self, prep_result: serde_json::Value) -> agentflow_core::Result<serde_json::Value> {
        let image_url = prep_result["image_url"].as_str().unwrap();
        let prompt = prep_result["prompt"].as_str().unwrap();

        // Create multimodal message
        let message = MultimodalMessage::text_and_image("user", prompt, image_url);

        // Execute multimodal LLM
        let response = LLMAgentFlow::model(&self.model_name)
            .multimodal_prompt(message)
            .temperature(0.7)
            .execute()
            .await
            .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError {
                message: format!("Multimodal LLM failed: {}", e),
            })?;

        Ok(serde_json::json!({
            "analysis": response,
            "success": true
        }))
    }
    
    // ... other AsyncNode methods
}
```

## Streaming Multimodal Responses

```rust
use futures::StreamExt;

let message = MultimodalMessage::text_and_image(
    "user",
    "Provide a detailed analysis of this image",
    "https://example.com/complex-image.jpg"
);

let mut stream = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .execute_streaming()
    .await?;

println!("Streaming analysis:");
while let Some(chunk) = stream.next_chunk().await? {
    print!("{}", chunk.content);
    if chunk.is_final {
        break;
    }
}
println!();
```

## Configuration

### Environment Variables
```bash
# StepFun API key for multimodal models
export STEP_API_KEY="your-stepfun-api-key"

# Optional: OpenAI for additional multimodal support
export OPENAI_API_KEY="your-openai-api-key"
```

### Model Configuration
Models are automatically configured with multimodal support. You can check if a model supports multimodal:

```rust
// This information is available in model configs
// step-1o-turbo-vision: supports_multimodal: true
// gpt-4o: supports_multimodal: true (when OpenAI provider used)
```

## Best Practices

### Image Quality and Detail Levels
- Use `"high"` detail for images requiring precise analysis
- Use `"low"` detail for simple recognition tasks to save tokens
- Default is usually sufficient for most use cases

### Token Management
- Multimodal requests consume more tokens than text-only
- Images are processed and converted to tokens by the provider
- High-detail images use significantly more tokens

### Error Handling
```rust
match AgentFlow::model("step-1o-turbo-vision")
    .text_and_image("Analyze this", image_url)
    .execute()
    .await
{
    Ok(response) => println!("Analysis: {}", response),
    Err(agentflow_llm::LLMError::HttpError { status_code, message }) => {
        println!("API Error {}: {}", status_code, message);
    },
    Err(e) => println!("Other error: {}", e),
}
```

## Examples

### Image Description Service
```rust
async fn describe_image(image_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let response = AgentFlow::model("step-1o-turbo-vision")
        .text_and_image("Describe this image in detail", image_url)
        .temperature(0.5)  // Lower temperature for consistent descriptions
        .execute()
        .await?;
    
    Ok(response)
}
```

### Batch Image Processing
```rust
async fn analyze_multiple_images(image_urls: Vec<&str>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut results = Vec::new();
    
    for url in image_urls {
        let analysis = AgentFlow::model("step-1o-turbo-vision")
            .text_and_image("What are the key elements in this image?", url)
            .execute()
            .await?;
        
        results.push(analysis);
    }
    
    Ok(results)
}
```

### Visual Question Answering
```rust
async fn answer_about_image(image_url: &str, question: &str) -> Result<String, Box<dyn std::error::Error>> {
    let message = MultimodalMessage::user()
        .add_text(&format!("Question: {}", question))
        .add_image_url(image_url)
        .add_text("Please provide a specific answer based on what you can see in the image.")
        .build();
    
    let response = AgentFlow::model("step-1o-turbo-vision")
        .multimodal_prompt(message)
        .temperature(0.3)  // Low temperature for factual answers
        .execute()
        .await?;
    
    Ok(response)
}
```

## Limitations and Considerations

1. **Provider Support**: Not all providers support multimodal. StepFun has the most comprehensive support.

2. **Image Formats**: Most common formats (JPEG, PNG, WebP) are supported. Check provider documentation for specifics.

3. **Image Size**: Large images may be resized by the provider. Consider image dimensions and file size.

4. **Cost**: Multimodal requests are typically more expensive than text-only requests.

5. **Rate Limits**: Image processing may have different rate limits than text processing.

## Troubleshooting

### Common Issues

**"Model not found"**
- Ensure you're using a multimodal-capable model like `step-1o-turbo-vision`
- Check that the model is configured in your `models.yml`

**"Invalid image URL"**
- Verify the image URL is accessible
- Ensure the image format is supported
- Check that the URL returns an image, not an HTML page

**"API key not found"**
- Set the appropriate environment variable (`STEP_API_KEY` for StepFun)
- Verify the API key is valid and has multimodal access

**High token usage**
- Try using `"low"` detail level for images
- Consider resizing images before processing
- Use smaller context windows when possible

## Integration with Other AgentFlow Features

### With Observability
```rust
let response = AgentFlow::model("step-1o-turbo-vision")
    .text_and_image("Analyze this", image_url)
    .with_metrics(metrics_collector)  // Track multimodal usage
    .execute()
    .await?;
```

### With Tools (Future)
```rust
// When MCP integration is available
let response = AgentFlow::model("step-1o-turbo-vision")
    .multimodal_prompt(message)
    .tools(vision_tools)  // Image processing tools
    .execute()
    .await?;
```

For more examples, see:
- `examples/multimodal_demo.rs` - Comprehensive multimodal examples
- `examples/multimodal_agent_flow.rs` - Integration with agent flows