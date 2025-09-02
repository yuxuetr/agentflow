# AgentFlow Nodes

Built-in node implementations for AgentFlow workflows, providing ready-to-use components for both code-first and configuration-first approaches.

[![Crates.io](https://img.shields.io/crates/v/agentflow-nodes.svg)](https://crates.io/crates/agentflow-nodes)
[![Documentation](https://docs.rs/agentflow-nodes/badge.svg)](https://docs.rs/agentflow-nodes)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

AgentFlow Nodes provides a comprehensive collection of specialized nodes for AI workflows, with clear separation of concerns between different AI capabilities:

- **🧠 LLMNode**: Text-only language model interactions
- **👁️ ImageUnderstandNode**: Vision and multimodal image analysis  
- **🎵 Audio Nodes**: Speech synthesis (TTS) and recognition (ASR)
- **🖼️ Image Generation**: Text-to-image, image editing, and transformations
- **🔧 Utility Nodes**: HTTP, file operations, templates, and workflow control

## Key Features

### 🏗️ **Clean Architecture**
- **Single Responsibility**: Each node type focuses on one domain
- **Type Safety**: Rust's type system ensures correct usage
- **Async/Await**: Full async support with Tokio integration
- **Error Handling**: Comprehensive error types and graceful fallbacks

### 🤖 **AI Model Integration**
- **Multiple Providers**: OpenAI, Anthropic, Google, StepFun, Moonshot
- **Automatic Configuration**: Model discovery and validation
- **Flexible Parameters**: Fine-tune temperature, tokens, and more
- **Response Formats**: JSON, Markdown, structured outputs

### 🖼️ **Advanced Image Handling**
- **Automatic Conversion**: Local files → Base64 seamlessly
- **Mixed Sources**: Local files + remote URLs in same workflow
- **Vision Tasks**: Analysis, OCR, comparison, Q&A
- **Multi-Image**: Compare and analyze multiple images together

## Quick Start

### Installation

```toml
[dependencies]
agentflow-nodes = "0.1"
agentflow-core = "0.1"  # Required for SharedState and AsyncNode
```

### Basic Text Processing

```rust
use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create shared state for data flow
    let shared = SharedState::new();
    
    // Simple text generation
    let writer = LlmNode::new("writer", "gpt-4")
        .with_prompt("Write a haiku about {{topic}}")
        .with_system("You are a creative poet")
        .with_temperature(0.8);
    
    // Add input data
    shared.insert("topic".to_string(), "autumn leaves".into());
    
    // Execute the node
    writer.run_async(&shared).await?;
    
    // Get the result
    if let Some(poem) = shared.get("writer_output") {
        println!("Generated poem: {}", poem.as_str().unwrap());
    }
    
    Ok(())
}
```

### Image Understanding

```rust
use agentflow_nodes::ImageUnderstandNode;
use agentflow_nodes::nodes::image_understand::VisionResponseFormat;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shared = SharedState::new();
    
    // Analyze a local image (automatically converted to base64)
    let analyzer = ImageUnderstandNode::image_analyzer(
        "analyzer", 
        "gpt-4o",  // Vision model
        "./my_diagram.jpg"  // Local file path
    )
    .with_system_message("You are an expert at analyzing technical diagrams")
    .with_text_prompt("What components and relationships do you see?")
    .with_temperature(0.3)
    .with_response_format(VisionResponseFormat::Markdown);
    
    analyzer.run_async(&shared).await?;
    
    if let Some(analysis) = shared.get("analyzer_output") {
        println!("Analysis: {}", analysis.as_str().unwrap());
    }
    
    Ok(())
}
```

### Mixed Workflow (Vision + Text)

```rust
use agentflow_nodes::{LlmNode, ImageUnderstandNode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shared = SharedState::new();
    
    // Step 1: Analyze image with vision model
    let vision_node = ImageUnderstandNode::image_analyzer(
        "vision", "gpt-4o", "./architecture.png"
    )
    .with_text_prompt("Analyze this system architecture");
    
    vision_node.run_async(&shared).await?;
    
    // Step 2: Generate documentation with text model
    let doc_node = LlmNode::new("docs", "gpt-4")  // Text-only model
        .with_prompt("Based on this analysis: {{vision_output}}, write technical documentation")
        .with_system("You are a technical writer")
        .with_temperature(0.2);
    
    doc_node.run_async(&shared).await?;
    
    if let Some(docs) = shared.get("docs_output") {
        println!("Documentation: {}", docs.as_str().unwrap());
    }
    
    Ok(())
}
```

## Node Types

### 🧠 LLMNode - Text Language Models

**Purpose**: Text-only language model interactions with comprehensive parameter support.

**Features**:
- Template-based prompts with variable substitution
- System message support
- Response format control (JSON, Markdown, etc.)
- Tool calling and MCP integration
- Retry logic and error handling

**Specialized Constructors**:
```rust
// Text analysis with structured output
let analyzer = LlmNode::text_analyzer("analyzer", "gpt-4");

// Creative writing with high temperature
let writer = LlmNode::creative_writer("writer", "claude-3-5-sonnet");

// Code generation with language specification
let coder = LlmNode::code_generator("coder", "gpt-4", "rust");

// Web research with tool integration
let researcher = LlmNode::web_researcher("research", "gpt-4");
```

### 👁️ ImageUnderstandNode - Vision Analysis

**Purpose**: Specialized multimodal image understanding with automatic format handling.

**Features**:
- Automatic base64 conversion for local files
- Direct URL support for remote images
- Multi-image comparison and analysis
- Vision-optimized parameters and response formats
- Template support for dynamic prompts

**Specialized Constructors**:
```rust
// Basic image description
let describer = ImageUnderstandNode::image_describer("desc", "gpt-4o", "photo.jpg");

// Detailed analysis with focus areas
let analyzer = ImageUnderstandNode::image_analyzer("analyze", "claude-3-5-sonnet", "diagram.png");

// OCR and text extraction
let extractor = ImageUnderstandNode::text_extractor("ocr", "gpt-4o", "document.pdf");

// Multi-image comparison
let comparator = ImageUnderstandNode::image_comparator(
    "compare", "gpt-4o", 
    "primary.jpg", 
    vec!["compare1.jpg".to_string(), "compare2.jpg".to_string()]
);

// Visual question answering
let qa = ImageUnderstandNode::visual_qa("qa", "gpt-4o", "scene.jpg");
```

### 🖼️ Image Generation Nodes

**TextToImageNode**: Generate images from text descriptions
```rust
use agentflow_nodes::TextToImageNode;

let generator = TextToImageNode::new("gen", "dall-e-3")
    .with_prompt("A serene mountain landscape at sunset")
    .with_size("1024x1024")
    .with_quality("hd");
```

**ImageToImageNode**: Transform existing images
```rust
use agentflow_nodes::ImageToImageNode;

let transformer = ImageToImageNode::new("transform", "dall-e-3")
    .with_source_image("original.jpg")
    .with_prompt("Make it look like a watercolor painting");
```

**ImageEditNode**: Edit specific parts of images
```rust
use agentflow_nodes::ImageEditNode;

let editor = ImageEditNode::new("edit", "dall-e-3")
    .with_source_image("photo.jpg")
    .with_mask("mask.png")
    .with_prompt("Replace the sky with a starry night");
```

### 🎵 Audio Nodes

**TTSNode**: Text-to-speech synthesis
```rust
use agentflow_nodes::TTSNode;

let tts = TTSNode::new("speech", "openai-tts")
    .with_text("Hello, this is a test of text-to-speech")
    .with_voice("alloy")
    .with_speed(1.0);
```

**ASRNode**: Automatic speech recognition
```rust
use agentflow_nodes::ASRNode;

let asr = ASRNode::new("transcribe", "whisper-1")
    .with_audio_file("recording.mp3")
    .with_language("en")
    .with_response_format("json");
```

### 🔧 Utility Nodes

**HttpNode**: HTTP requests and API integration
```rust
use agentflow_nodes::HttpNode;

let http = HttpNode::new("api_call", "https://api.example.com/data")
    .with_method("POST")
    .with_headers(vec![("Content-Type".to_string(), "application/json".to_string())])
    .with_body("{{request_data}}");
```

**FileNode**: File system operations
**TemplateNode**: Advanced template processing
**BatchNode**: Parallel execution of multiple operations
**ConditionalNode**: Conditional workflow control

## Architecture Benefits

### 🎯 **Clear Separation of Concerns**

| Node Type | Purpose | Best For |
|-----------|---------|----------|
| **LLMNode** | Text-only language models | Content generation, analysis, reasoning |
| **ImageUnderstandNode** | Vision and image analysis | OCR, image description, visual Q&A |
| **Image Generation** | Creating/editing images | Art generation, image manipulation |
| **Audio Nodes** | Speech processing | Voice synthesis, transcription |
| **Utility Nodes** | Infrastructure operations | HTTP, files, templates, control flow |

### 🔧 **Developer Experience**

```rust
// ❌ Before: Unclear which node to use
let node = SomeNode::new("analyze", "gpt-4o")
    .with_images(vec!["image.jpg"])  // Is this the right approach?
    .with_prompt("Analyze this");

// ✅ After: Clear intent and purpose
let vision_node = ImageUnderstandNode::image_analyzer("analyze", "gpt-4o", "image.jpg")
    .with_text_prompt("Analyze this diagram");

let text_node = LlmNode::new("summarize", "gpt-4")
    .with_prompt("Summarize: {{analyze_output}}");
```

### 🚀 **Workflow Composition**

```rust
// Complex workflow with multiple specialized nodes
async fn analyze_business_document() -> Result<(), Box<dyn std::error::Error>> {
    let shared = SharedState::new();
    
    // 1. Extract text from document image
    let ocr = ImageUnderstandNode::text_extractor("ocr", "gpt-4o", "document.png");
    ocr.run_async(&shared).await?;
    
    // 2. Analyze the extracted text
    let analyzer = LlmNode::text_analyzer("analyze", "gpt-4")
        .with_prompt("Analyze this document: {{ocr_output}}");
    analyzer.run_async(&shared).await?;
    
    // 3. Generate executive summary
    let summary = LlmNode::new("summary", "gpt-4")
        .with_prompt("Create executive summary: {{analyze_output}}")
        .with_system("You are a business analyst");
    summary.run_async(&shared).await?;
    
    // 4. Generate visual chart from data
    let chart = TextToImageNode::new("chart", "dall-e-3")
        .with_prompt("Create a business chart showing: {{analyze_output}}");
    chart.run_async(&shared).await?;
    
    Ok(())
}
```

## Configuration

### Model Configuration

Create `~/.agentflow/models.yml`:

```yaml
providers:
  openai:
    api_key: "${OPENAI_API_KEY}"
    models:
      - name: "gpt-4"
        type: "text"
        max_tokens: 4096
      - name: "gpt-4o"
        type: "multimodal"
        max_tokens: 4096
      - name: "dall-e-3"
        type: "image-generation"

  anthropic:
    api_key: "${ANTHROPIC_API_KEY}"
    models:
      - name: "claude-3-5-sonnet"
        type: "text"
        max_tokens: 8192

  stepfun:
    api_key: "${STEPFUN_API_KEY}"
    models:
      - name: "step-1o-turbo-vision"
        type: "multimodal"
        max_tokens: 4096
```

### Environment Variables

```bash
# Create ~/.agentflow/.env
OPENAI_API_KEY=your_openai_key
ANTHROPIC_API_KEY=your_anthropic_key
STEPFUN_API_KEY=your_stepfun_key
```

## Examples

The `examples/` directory contains comprehensive demonstrations:

- **`basic_llm_example.rs`**: Simple text processing
- **`multimodal_base64_example.rs`**: Local image analysis
- **`multimodal_http_example.rs`**: Remote image processing
- **`mixed_image_sources_example.rs`**: Combined local/remote workflows
- **`advanced_llm_example.rs`**: Complex text processing workflows
- **`specialized_ai_nodes.rs`**: Audio and image generation

Run examples:
```bash
# Text processing
cargo run --example basic_llm_example

# Image understanding
cargo run --example multimodal_base64_example

# Mixed workflows  
cargo run --example mixed_image_sources_example
```

## Features

### Default Features
```toml
default = ["llm", "http", "file", "template"]
```

### Available Features
- **`llm`**: Text language model integration
- **`http`**: HTTP client for API calls
- **`file`**: File system operations
- **`template`**: Advanced template processing with Handlebars
- **`batch`**: Parallel execution capabilities
- **`conditional`**: Conditional workflow control
- **`factories`**: Configuration-first node creation

Enable specific features:
```toml
[dependencies]
agentflow-nodes = { version = "0.1", features = ["llm", "http", "template"] }
```

## Error Handling

AgentFlow Nodes provides comprehensive error handling:

```rust
use agentflow_nodes::{NodeError, NodeResult};

// Errors are strongly typed
match node.run_async(&shared).await {
    Ok(_) => println!("Success!"),
    Err(NodeError::ConfigurationError { message }) => {
        eprintln!("Configuration issue: {}", message);
    },
    Err(NodeError::ExecutionError { message }) => {
        eprintln!("Execution failed: {}", message);
    },
    Err(NodeError::ValidationError { message }) => {
        eprintln!("Validation error: {}", message);
    },
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_llm_node_creation

# Run with specific features
cargo test --features="llm,http,template"

# Run examples to verify functionality
cargo run --example basic_llm_example
```

## Contributing

Contributions are welcome! Please see our [Contributing Guide](../CONTRIBUTING.md) for details.

### Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/yuxuetr/agentflow.git
   cd agentflow/agentflow-nodes
   ```

2. **Install dependencies**:
   ```bash
   cargo build
   ```

3. **Set up configuration**:
   ```bash
   mkdir -p ~/.agentflow
   cp examples/models.yml ~/.agentflow/
   cp examples/.env ~/.agentflow/
   # Edit with your API keys
   ```

4. **Run tests**:
   ```bash
   cargo test
   cargo run --example basic_llm_example
   ```

### Adding New Nodes

1. Create your node in `src/nodes/your_node.rs`
2. Implement the `AsyncNode` trait
3. Add to `src/nodes/mod.rs`
4. Export in `src/lib.rs`
5. Add tests and examples
6. Update documentation

Example node structure:
```rust
use agentflow_core::{AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YourNode {
    pub name: String,
    pub config: String,
    // ... other fields
}

impl YourNode {
    pub fn new(name: &str, config: &str) -> Self {
        Self {
            name: name.to_string(),
            config: config.to_string(),
        }
    }
}

#[async_trait]
impl AsyncNode for YourNode {
    async fn execute(&mut self, shared_state: &SharedState) -> Result<Value> {
        // Your implementation here
        todo!()
    }
}
```

## Roadmap

- 🔄 **MCP Integration**: Model Context Protocol support for tool calling
- 🔄 **RAG Support**: Retrieval-Augmented Generation capabilities  
- 📋 **More Audio Nodes**: Voice cloning, audio effects
- 🧠 **Advanced Reasoning**: Chain-of-thought, tree search
- 📊 **Data Processing**: CSV, JSON, database operations
- 🌐 **Web Scraping**: Advanced web content extraction
- 🔌 **Plugin System**: Custom node development framework

## License

This project is licensed under the MIT License - see the [LICENSE](../LICENSE) file for details.

## Links

- **Documentation**: [docs.rs/agentflow-nodes](https://docs.rs/agentflow-nodes)
- **Repository**: [GitHub](https://github.com/yuxuetr/agentflow)
- **Issues**: [GitHub Issues](https://github.com/yuxuetr/agentflow/issues)
- **Discord**: [Community Chat](https://discord.gg/yuxuetr)

---

Built with ❤️ by the AgentFlow team. Made for developers who want to build powerful AI workflows with clean, maintainable code.