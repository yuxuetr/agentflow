# AgentFlow Nodes - Real LLM Model Calling Examples

This directory contains comprehensive examples demonstrating how to use `agentflow-nodes` to call real LLM models with various configurations and features.

## Setup Requirements

### 1. Configure API Keys

Create or update your environment configuration:

```bash
# Generate default config files
mkdir -p ~/.agentflow
```

Add your API keys to `~/.agentflow/.env`:

```env
# OpenAI
OPENAI_API_KEY=sk-your-openai-key-here

# Anthropic  
ANTHROPIC_API_KEY=sk-ant-your-anthropic-key-here

# Google (Gemini)
GOOGLE_API_KEY=your-google-api-key-here

# StepFun (for multimodal examples)
STEPFUN_API_KEY=sk-your-stepfun-key-here

# Other providers
MOONSHOT_API_KEY=sk-your-moonshot-key-here
```

### 2. Configure Models

Update your `~/.agentflow/models.yml` to include the models used in examples:

```yaml
providers:
  openai:
    api_key: ${OPENAI_API_KEY}
    base_url: "https://api.openai.com/v1"
    models:
      - name: "gpt-4o"
        model_id: "gpt-4o"
        capabilities: ["text", "multimodal"]
      - name: "gpt-4o-mini"
        model_id: "gpt-4o-mini" 
        capabilities: ["text"]

  anthropic:
    api_key: ${ANTHROPIC_API_KEY}
    base_url: "https://api.anthropic.com"
    models:
      - name: "claude-3-5-sonnet"
        model_id: "claude-3-5-sonnet-20241022"
        capabilities: ["text"]
      - name: "claude-3-haiku"
        model_id: "claude-3-haiku-20240307"
        capabilities: ["text"]

  stepfun:
    api_key: ${STEPFUN_API_KEY}
    base_url: "https://api.stepfun.com/v1"
    models:
      - name: "step-1o-turbo-vision"
        model_id: "step-1o-turbo-vision"
        capabilities: ["text", "multimodal", "image"]
```

## Examples Overview

### 1. Basic LLM Example (`basic_llm_example.rs`)

**What it demonstrates:**
- Simple LLM node creation and execution
- Basic parameters: temperature, max_tokens
- System and user prompts
- Real vs mock LLM modes
- SharedState variable resolution

**Key features:**
```rust
let llm_node = LlmNode::new("math_assistant", "gpt-4o-mini")
  .with_prompt("Question: {{user_question}}")
  .with_system("You are a helpful math tutor.")
  .with_temperature(0.3)
  .with_max_tokens(150)
  .with_real_llm(true);
```

**Run it:**
```bash
cargo run --example basic_llm_example
```

### 2. Advanced LLM Example (`advanced_llm_example.rs`)

**What it demonstrates:**
- Multiple LLM parameters (temperature, top_p, frequency_penalty, etc.)
- Stop sequences for controlled generation
- Response format configuration
- Chaining multiple LLM nodes
- Complex template variable resolution
- Structured JSON responses

**Key features:**
```rust
let creative_node = LlmNode::new("content_creator", "gpt-4o")
  .with_temperature(0.8)
  .with_top_p(0.9)
  .with_frequency_penalty(0.3)
  .with_presence_penalty(0.2)
  .with_stop_sequences(vec!["---".to_string()])
  .with_response_format(ResponseFormat::Markdown);
```

**Run it:**
```bash
cargo run --example advanced_llm_example
```

### 3. Multimodal LLM Examples

#### HTTP Images (`multimodal_http_example.rs`)
**What it demonstrates:**
- Image analysis using HTTP/HTTPS URLs (recommended approach)
- Single and multiple image processing
- Context integration with visual analysis
- Structured JSON output from image analysis

**Key features:**
```rust
let image_analyzer = LlmNode::new("image_analyst", "step-1o-turbo-vision")
  .with_prompt("Analyze this diagram...")
  .with_images(vec!["https://example.com/image.jpg".to_string()])
  .with_temperature(0.3);
```

**Run it:**
```bash
cargo run --example multimodal_http_example
```

#### Base64 Local Images (`multimodal_base64_example.rs`) 
**What it demonstrates:**
- Converting local images to base64 for API compatibility
- Local file processing and validation
- Image size optimization considerations

**Key features:**
```rust
// Convert local image to base64 data URL
let image_bytes = fs::read("local/image.jpg")?;
let base64_image = BASE64.encode(&image_bytes);
let data_url = format!("data:image/jpeg;base64,{}", base64_image);
```

**Run it:**
```bash
cargo run --example multimodal_base64_example
```

### 4. Structured Response Example (`structured_response_example.rs`)

**What it demonstrates:**
- Strict JSON schema validation
- Complex nested object structures
- Enum constraints and number ranges
- Required vs optional fields
- Loose JSON mode
- Business intelligence workflows

**Key features:**
```rust
let sentiment_node = LlmNode::new("sentiment_analyzer", "gpt-4o")
  .with_json_response(Some(json!({
    "type": "object",
    "properties": {
      "overall_sentiment": {
        "type": "string",
        "enum": ["very_positive", "positive", "neutral", "negative", "very_negative"]
      },
      "sentiment_score": {
        "type": "number", 
        "minimum": -1.0,
        "maximum": 1.0
      }
    }
  })));
```

**Run it:**
```bash
cargo run --example structured_response_example
```

## Parameter Reference

### Standard LLM Parameters

| Parameter | Type | Description | Example |
|-----------|------|-------------|---------|
| `temperature` | `f32` | Creativity/randomness (0.0-2.0) | `0.7` |
| `max_tokens` | `u32` | Maximum response length | `500` |
| `top_p` | `f32` | Nucleus sampling (0.0-1.0) | `0.9` |
| `top_k` | `u32` | Vocabulary restriction | `40` |
| `frequency_penalty` | `f32` | Reduce repetition (-2.0-2.0) | `0.3` |
| `presence_penalty` | `f32` | Encourage topic diversity (-2.0-2.0) | `0.2` |
| `stop` | `Vec<String>` | Stop generation at these sequences | `vec!["END"]` |
| `seed` | `u64` | For reproducible outputs | `12345` |

### Response Formats

```rust
// Text (default)
.with_response_format(ResponseFormat::Text)

// Markdown
.with_response_format(ResponseFormat::Markdown)

// Loose JSON
.with_response_format(ResponseFormat::loose_json())

// Strict JSON with schema
.with_json_response(Some(schema_json))
```

### Multimodal Features

```rust
// Images from shared state
.with_images(vec!["image_key".to_string()])

// Direct image URLs
.with_images(vec!["https://example.com/image.jpg".to_string()])

// Audio (planned)
.with_audio(vec!["audio_key".to_string()])
```

## Troubleshooting

### Common Issues

1. **"Model not found" error:**
   - Verify model name in your `models.yml`
   - Check API key configuration
   - Ensure model is available with your API plan

2. **"Failed to initialize AgentFlow" error:**
   - Run `AgentFlow::generate_config().await?` first
   - Check `~/.agentflow/.env` file exists
   - Verify API keys are valid

3. **Empty responses:**
   - Check your API key limits/quotas
   - Verify internet connectivity
   - Try with a different model

4. **JSON parsing errors:**
   - Some models may not follow JSON schema strictly
   - Try with `loose_json()` format
   - Use models known for structured output (GPT-4, Claude)

### Debug Tips

1. **Enable logging:**
```rust
AgentFlow::init_logging()?;
```

2. **Check shared state:**
```rust
println!("Shared state keys: {:?}", shared.keys());
```

3. **Use mock mode for testing:**
```rust
let node = LlmNode::new("test", "gpt-4o").with_mock_mode();
```

## Model Recommendations

### For Different Use Cases

- **Creative writing:** `gpt-4o` with high temperature (0.8-1.0)
- **Analysis/reasoning:** `claude-3-5-sonnet` with low temperature (0.1-0.3)  
- **Fast responses:** `gpt-4o-mini` or `claude-3-haiku`
- **Structured output:** `gpt-4o` with JSON schema
- **Image analysis:** `step-1o-turbo-vision` (StepFun)
- **Cost-effective:** `gpt-4o-mini` for simple tasks

### Provider Comparison

| Provider | Strengths | Best For |
|----------|-----------|----------|
| OpenAI | JSON mode, function calling | Structured output, tools |
| Anthropic | Reasoning, safety | Analysis, long conversations |
| StepFun | Multimodal, Chinese support | Image analysis, localized content |
| Google | Fast, efficient | High-throughput applications |

## Next Steps

1. **Try the examples** with your own API keys and data
2. **Modify parameters** to see how they affect outputs  
3. **Build workflows** by chaining multiple LLM nodes
4. **Explore multimodal** capabilities with your own images
5. **Create custom schemas** for your specific use cases

For more advanced features like MCP tools integration, see the planned `agentflow-mcp` examples (coming soon).