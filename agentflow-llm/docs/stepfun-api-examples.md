# StepFun API Examples - Real Test Cases

This document provides comprehensive examples for all StepFun model categories using actual API endpoints and parameters. All examples use the `STEP_API_KEY` environment variable for authentication.

## Table of Contents

- [Text Models (Chat Completion)](#text-models-chat-completion)
- [Function Calling](#function-calling)
- [Image Understanding](#image-understanding)  
- [Multimodal](#multimodal)
- [TTS (Text-to-Speech)](#tts-text-to-speech)
- [ASR (Speech Recognition)](#asr-speech-recognition)
- [Image Generation](#image-generation)
- [Image-to-Image Generation](#image-to-image-generation)
- [Image Editing](#image-editing)
- [Voice Management](#voice-management)
- [Voice Cloning](#voice-cloning)
- [Error Handling & Troubleshooting](#error-handling--troubleshooting)
- [Model Comparison](#model-comparison)
- [AgentFlow LLM Integration](#agentflow-llm-integration)

---

## Text Models (Chat Completion)

**Endpoint:** `https://api.stepfun.com/v1/chat/completions`
**Models:** `step-1-8k`, `step-1-32k`, `step-1-256k`, `step-2-16k`, `step-2-mini`, `step-2-16k-202411`, `step-2-16k-exp`
**Input:** Text → **Output:** Text
**Streaming:** ✅ Supported

### Streaming Request

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-1-32k",
  "messages": [{"role": "user", "content": "解释量子计算"}],
  "stream": true
}'
```

### Non-streaming Request

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-2-16k",
  "messages": [{"role": "user", "content": "用Python写快速排序"}]
}'
```

**Key Features:**
- Supports both streaming and non-streaming modes
- Uses standard OpenAI-compatible chat completions format
- Temperature, max_tokens, and other parameters supported

---

## Function Calling

**Endpoint:** `https://api.stepfun.com/v1/chat/completions`
**Models:** `step-1-8k-functions`
**Input:** Text + Function Schemas → **Output:** Function Calls + Text
**Streaming:** ✅ Supported

### Request Example

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-1-8k-functions",
  "messages": [{"role": "user", "content": "今天北京的天气如何？"}],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "获取指定城市的天气信息",
        "parameters": {
          "type": "object",
          "properties": {
            "city": {"type": "string", "description": "城市名称"},
            "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
          },
          "required": ["city"]
        }
      }
    }
  ],
  "tool_choice": "auto",
  "temperature": 0.1
}'
```

### Response Example

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1699896916,
  "model": "step-1-8k-functions",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": null,
        "tool_calls": [
          {
            "id": "call_abc123",
            "type": "function",
            "function": {
              "name": "get_weather",
              "arguments": "{\"city\": \"北京\", \"unit\": \"celsius\"}"
            }
          }
        ]
      },
      "finish_reason": "tool_calls"
    }
  ]
}
```

**Key Features:**
- Optimized for function calling with lower temperature (0.1)
- Supports OpenAI-compatible tools format
- Can determine when and how to call functions based on user input
- Handles complex function schemas and parameter validation

---

## Image Understanding

**Endpoint:** `https://api.stepfun.com/v1/chat/completions`
**Models:** `step-1o-turbo-vision`, `step-1o-vision-32k`, `step-1v-8k`, `step-1v-32k`
**Input:** Text + Image → **Output:** Text
**Streaming:** ✅ Supported

### Request Example (URL)

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-1o-turbo-vision",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "描述这张图片"},
        {"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}}
      ]
    }
  ]
}'
```

### Request Example (Base64)

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-1o-turbo-vision",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "分析这张图片的内容"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="}}
      ]
    }
  ]
}'
```

**Key Features:**
- Uses chat completions endpoint with multimodal content
- Supports images via URL or base64 encoding
- Content array format with `type` field for different media types

---

## Multimodal

**Endpoint:** `https://api.stepfun.com/v1/chat/completions`
**Models:** `step-3`
**Input:** Text + Image + Audio → **Output:** Text + Multimedia
**Streaming:** ✅ Supported

### Request Example

```bash
curl https://api.stepfun.com/v1/chat/completions \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-3",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "根据图表分析趋势"},
        {"type": "image_url", "image_url": {"url": "https://example.com/chart.png"}}
      ]
    }
  ]
}'
```

**Key Features:**
- Most advanced model supporting multiple input/output modalities
- Same API format as image understanding but with extended capabilities
- Can process and generate various content types

---

## TTS (Text-to-Speech)

**Endpoint:** `https://api.stepfun.com/v1/audio/speech`
**Models:** `step-tts-vivid`, `step-tts-mini`
**Input:** Text → **Output:** Audio (MP3/WAV/OPUS/FLAC)
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/audio/speech \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-tts-vivid",
  "input": "智能阶跃，十倍每一个人的可能",
  "voice": "cixingnansheng",
  "response_format": "mp3"
}' --output speech.mp3
```

### Available Parameters

```json
{
  "model": "step-tts-vivid",
  "input": "Text to synthesize (max 1000 characters)",
  "voice": "cixingnansheng",
  "response_format": "mp3", // mp3, wav, opus, flac
  "speed": 1.0, // 0.5 - 2.0
  "volume": 1.0, // 0.1 - 2.0
  "voice_label": {
    "language": "粤语", // 粤语, 四川话, 日语
    "emotion": "高兴", // 高兴, 非常高兴, 生气
    "style": "慢速" // 慢速, 极慢, 快速, 极快
  },
  "sample_rate": 24000 // 8000, 16000, 22050, 24000
}
```

**Key Features:**
- Dedicated audio synthesis endpoint
- Multiple output formats and sample rates
- Voice customization with emotion and style controls
- Chinese voice optimization

---

## ASR (Speech Recognition)

**Endpoint:** `https://api.stepfun.com/v1/audio/transcriptions`
**Models:** `step-asr`
**Input:** Audio (WAV/MP3/FLAC) → **Output:** Text
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/audio/transcriptions \
-H "Authorization: Bearer $STEP_API_KEY" \
-F file=@"audio.wav" \
-F model="step-asr" \
-F response_format="json"
```

### Response Formats

| Format | Description | Output |
|--------|-------------|---------|
| `json` | JSON with metadata | `{"text": "transcribed text"}` |
| `text` | Plain text | Raw transcribed text |
| `srt` | SubRip format | Subtitle file with timestamps |
| `vtt` | WebVTT format | Web video text tracks |

**Key Features:**
- Multipart form data upload
- Multiple response formats including subtitles
- Optimized for Chinese speech recognition

---

## Image Generation

**Endpoint:** `https://api.stepfun.com/v1/images/generations`
**Models:** `step-2x-large`, `step-1x-medium`
**Input:** Text → **Output:** Image
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/images/generations \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-2x-large",
  "prompt": "未来城市景观，赛博朋克风格",
  "size": "1024x1024"
}'
```

### Available Parameters

```json
{
  "model": "step-2x-large",
  "prompt": "Detailed text description",
  "size": "1024x1024", // "512x512", "768x768", "1024x1024", "1280x800"
  "n": 1, // Currently only supports 1
  "response_format": "b64_json", // "b64_json" or "url"
  "seed": 12345, // For reproducible results
  "steps": 50, // 1-100, generation steps
  "cfg_scale": 7.5, // 1-10, guidance scale
  "style_reference": {
    "source_url": "https://example.com/style.jpg",
    "weight": 1.0 // (0, 2], default 1
  }
}
```

**Key Features:**
- Specialized image generation endpoint
- Style reference support for consistent aesthetics
- Multiple output formats (base64 or URL)
- Fine-grained control over generation parameters

---

## Image-to-Image Generation

**Endpoint:** `https://api.stepfun.com/v1/images/image2image`
**Models:** `step-2x-large`, `step-1x-medium`
**Input:** Image + Text → **Output:** Image
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/images/image2image \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-2x-large",
  "prompt": "将这张图片转换为水彩画风格",
  "source_url": "https://example.com/source.jpg",
  "source_weight": 0.8,
  "size": "1024x1024",
  "response_format": "b64_json"
}'
```

### Available Parameters

```json
{
  "model": "step-2x-large",
  "prompt": "Transformation description",
  "source_url": "https://example.com/source.jpg", // URL or base64
  "source_weight": 0.8, // (0, 1], influence of source image
  "size": "1024x1024", // "512x512", "768x768", "1024x1024"
  "n": 1, // Currently only supports 1
  "response_format": "b64_json", // "b64_json" or "url"
  "seed": 12345, // For reproducible results
  "steps": 50, // 1-100, generation steps
  "cfg_scale": 7.5 // 1-10, guidance scale
}
```

**Key Features:**
- Transform existing images using text prompts
- Control influence of source image with source_weight parameter
- Maintains structural elements while applying style changes
- Same generation parameters as text-to-image

---

## Image Editing

**Endpoint:** `https://api.stepfun.com/v1/images/edits`
**Models:** `step-1x-edit`
**Input:** Image + Text → **Output:** Image
**Streaming:** ❌ Not supported

### Request Example

```bash
curl -X POST https://api.stepfun.com/v1/images/edits \
-H "Authorization: Bearer $STEP_API_KEY" \
-F model="step-1x-edit" \
-F image=@"original.jpg" \
-F prompt="添加彩虹效果" \
-F response_format="url"
```

### Available Parameters

```bash
-F model="step-1x-edit"
-F image=@"original.jpg"  # Source image file
-F prompt="Edit instruction text"
-F seed=12345  # Optional, for reproducible results
-F steps=28  # Optional, default 28
-F cfg_scale=6  # Optional, default 6
-F size="512x512"  # "512x512", "768x768", "1024x1024"
-F response_format="url"  # "b64_json" or "url"
```

**Key Features:**
- Multipart form data for image upload
- Text-guided image editing
- Preserves original image structure while applying modifications

---

## Voice Management

**Endpoint:** `https://api.stepfun.com/v1/audio/voices`
**Method:** GET
**Purpose:** List available custom voices
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/audio/voices \
-H "Authorization: Bearer $STEP_API_KEY" \
-G \
-d limit=20 \
-d order=desc
```

### Query Parameters

| Parameter | Type | Description | Default |
|-----------|------|-------------|---------|
| `limit` | integer | Number of voices to return (1-100) | 20 |
| `order` | string | Sort order: `asc` or `desc` | `desc` |
| `before` | string | Return voices before this ID | - |
| `after` | string | Return voices after this ID | - |

### Response Example

```json
{
  "object": "list",
  "data": [
    {
      "id": "voice-abc123",
      "file_id": "file-xyz789",
      "created_at": 1699896916
    },
    {
      "id": "voice-def456",
      "file_id": "file-uvw012",
      "created_at": 1699896800
    }
  ],
  "has_more": false,
  "first_id": "voice-abc123",
  "last_id": "voice-def456"
}
```

**Key Features:**
- Paginated listing of custom voices
- Returns voice IDs for use in TTS requests
- Includes creation timestamps and associated file IDs
- Default system voices (like `cixingnansheng`) are always available

---

## Voice Cloning

**Endpoint:** `https://api.stepfun.com/v1/audio/voices`
**Models:** `step-tts-vivid`, `step-tts-mini`
**Input:** Audio Sample + Text → **Output:** Cloned Voice ID
**Streaming:** ❌ Not supported

### Request Example

```bash
curl https://api.stepfun.com/v1/audio/voices \
-H "Content-Type: application/json" \
-H "Authorization: Bearer $STEP_API_KEY" \
-d '{
  "model": "step-tts-vivid",
  "file_id": "file-abc123",
  "text": "这是用于训练的声音样本"
}'
```

### Response Format

```json
{
  "id": "voice-xyz789",
  "object": "audio.voice",
  "duplicated": false,
  "sample_text": "这是用于训练的声音样本",
  "sample_audio": "base64_encoded_wav_data"
}
```

**Key Features:**
- Creates custom voice profiles from audio samples
- Returns voice ID for use in TTS requests
- Includes sample audio playback for verification

---

## Error Handling & Troubleshooting

### Common Error Responses

#### Authentication Errors
```json
{
  "error": {
    "message": "Incorrect API key provided",
    "type": "invalid_api_key"
  }
}
```
**Status Code:** 401  
**Solution:** Verify `STEP_API_KEY` is correct and active

#### Input Validation Errors
```json
{
  "error": {
    "message": "url invalid. url: https://example.com/image.jpg. msg: Cannot fetch the content of url(s) you provided.",
    "type": "input_invalid"
  }
}
```
**Status Code:** 400  
**Solution:** Use accessible URLs or base64 data URIs instead

#### Rate Limit Errors
```json
{
  "error": {
    "message": "Rate limit exceeded",
    "type": "rate_limit_exceeded"
  }
}
```
**Status Code:** 429  
**Solution:** Implement exponential backoff retry logic

#### Model Not Found
```json
{
  "error": {
    "message": "The model 'step-invalid-model' does not exist",
    "type": "invalid_request_error"
  }
}
```
**Status Code:** 404  
**Solution:** Check model name spelling and availability

### Best Practices

1. **Image URLs**: Use base64 data URIs instead of external URLs to avoid fetch failures
   ```json
   {"url": "data:image/png;base64,iVBORw0KGgoAAAA..."}
   ```

2. **File Uploads**: Ensure proper multipart form encoding for ASR and image editing
   ```bash
   curl -F file=@"audio.wav" -F model="step-asr"
   ```

3. **Unicode Handling**: StepFun handles Chinese text well, but ensure UTF-8 encoding
   ```json
   {"input": "智能阶跃，十倍每一个人的可能"}
   ```

4. **Timeout Settings**: Set appropriate timeouts for different operations:
   - Text/Chat: 30-60 seconds
   - Image Generation: 60-120 seconds
   - Audio Processing: 30-90 seconds

5. **Response Parsing**: Always check HTTP status before parsing JSON
   ```bash
   if response.status_code == 200:
       result = response.json()
   else:
       error = response.json()
       handle_error(error)
   ```

### Troubleshooting Checklist

- ✅ API key is valid and properly formatted
- ✅ Model name is spelled correctly
- ✅ Request format matches endpoint requirements
- ✅ Image URLs are accessible or using base64
- ✅ Audio files are in supported formats (WAV, MP3, FLAC)
- ✅ Text input is within character limits (1000 for TTS)
- ✅ Network connectivity is stable
- ✅ Request timeout is sufficient for operation type

---

## Model Comparison

### Text Models

| Model | Context Length | Use Case | Performance | Cost |
|-------|----------------|----------|-------------|------|
| `step-1-8k` | 8K tokens | General chat, simple tasks | Balanced | Low |
| `step-1-32k` | 32K tokens | Long documents, extended conversations | High quality | Medium |
| `step-1-256k` | 256K tokens | Very long documents, complex analysis | Highest quality | High |
| `step-2-16k` | 16K tokens | Latest model, balanced performance | High quality | Medium |
| `step-2-mini` | 8K tokens | Fast responses, simple tasks | Good speed | Low |
| `step-2-16k-202411` | 16K tokens | November 2024 version | Latest features | Medium |
| `step-2-16k-exp` | 16K tokens | Experimental features | Cutting edge | Medium |
| `step-1-8k-functions` | 8K tokens | Function calling optimized | Precise tools | Medium |

### Vision Models

| Model | Context | Image Support | Use Case |
|-------|---------|---------------|----------|
| `step-1o-turbo-vision` | 16K tokens | Multiple images | Fast image understanding |
| `step-1o-vision-32k` | 32K tokens | Multiple images | Complex visual analysis |
| `step-1v-8k` | 8K tokens | Single/multiple images | General vision tasks |
| `step-1v-32k` | 32K tokens | Single/multiple images | Detailed image analysis |
| `step-3` | 8K tokens | Multimodal content | Advanced multimodal tasks |

### Audio Models

| Model | Type | Input | Output | Quality |
|-------|------|-------|--------|---------|
| `step-tts-vivid` | TTS | Text (1000 chars) | High-quality audio | Premium |
| `step-tts-mini` | TTS | Text (1000 chars) | Fast audio synthesis | Good |
| `step-asr` | ASR | Audio files | Text transcription | High accuracy |

### Image Models

| Model | Type | Resolution | Speed | Quality |
|-------|------|------------|-------|---------|
| `step-2x-large` | Generation | Up to 1280x800 | Slower | Highest |
| `step-1x-medium` | Generation | Up to 1024x1024 | Faster | Good |
| `step-1x-edit` | Editing | Up to 1024x1024 | Medium | High |

---

## AgentFlow LLM Integration

The AgentFlow LLM library provides easy integration with all StepFun models through a unified interface.

### Setup

```bash
cargo add agentflow-llm
```

```rust
use agentflow_llm::AgentFlow;

// Initialize with config file
let agent = AgentFlow::init_with_config("config.yml").await?;

// Or use built-in defaults
let agent = AgentFlow::init().await?;
```

### Configuration Example

```yaml
# config.yml
providers:
  step:
    api_key: "${STEP_API_KEY}"
    base_url: "https://api.stepfun.com/v1"

models:
  step-2-16k:
    vendor: step
    type: text
    max_tokens: 16384
    supports_streaming: true
  
  step-1o-turbo-vision:
    vendor: step
    type: imageunderstand
    supports_multimodal: true
    
  step-tts-vivid:
    vendor: step
    type: tts
    supports_streaming: false
```

### Usage Examples

#### Text Generation
```rust
let response = agent.generate_text("step-2-16k", "解释量子计算").await?;
println!("{}", response.content);
```

#### Image Understanding
```rust
use agentflow_llm::MultimodalMessage;

let message = MultimodalMessage::new()
    .add_text("描述这张图片")
    .add_image_url("data:image/png;base64,iVBORw0...");

let response = agent.generate_multimodal("step-1o-turbo-vision", message).await?;
```

#### Streaming
```rust
let mut stream = agent.stream_text("step-2-16k", "写一首诗").await?;

while let Some(chunk) = stream.next().await {
    print!("{}", chunk.content);
}
```

#### Function Calling
```rust
use serde_json::json;

let tools = json!([{
    "type": "function",
    "function": {
        "name": "get_weather",
        "description": "获取天气信息",
        "parameters": {
            "type": "object",
            "properties": {
                "city": {"type": "string"}
            }
        }
    }
}]);

let response = agent.generate_with_tools(
    "step-1-8k-functions",
    "北京天气如何？",
    tools
).await?;
```

### Error Handling
```rust
match agent.generate_text("step-2-16k", prompt).await {
    Ok(response) => println!("{}", response.content),
    Err(LLMError::HttpError { status_code: 401, .. }) => {
        eprintln!("Invalid API key");
    },
    Err(LLMError::HttpError { status_code: 429, .. }) => {
        eprintln!("Rate limited - implement retry logic");
    },
    Err(e) => eprintln!("Error: {}", e),
}
```

### Available Model Types
- `text`: Standard text generation models
- `imageunderstand`: Vision + text models  
- `multimodal`: Advanced multimodal models
- `tts`: Text-to-speech models
- `asr`: Speech recognition models
- `generateimage`: Image generation models
- `editimage`: Image editing models
- `functioncalling`: Function calling optimized models

---

## API Pattern Summary

| Model Type | Endpoint | Method | Content-Type | Input Format | Streaming |
|------------|----------|--------|--------------|--------------|-----------|
| **Text** | `/chat/completions` | POST | `application/json` | JSON messages | ✅ |
| **Function Calling** | `/chat/completions` | POST | `application/json` | JSON with tools | ✅ |
| **Image Understanding** | `/chat/completions` | POST | `application/json` | JSON with image URLs | ✅ |
| **Multimodal** | `/chat/completions` | POST | `application/json` | JSON multimedia content | ✅ |
| **TTS** | `/audio/speech` | POST | `application/json` | JSON with text | ❌ |
| **ASR** | `/audio/transcriptions` | POST | `multipart/form-data` | Form with audio file | ❌ |
| **Image Generation** | `/images/generations` | POST | `application/json` | JSON with prompt | ❌ |
| **Image-to-Image** | `/images/image2image` | POST | `application/json` | JSON with image + prompt | ❌ |
| **Image Editing** | `/images/edits` | POST | `multipart/form-data` | Form with image + prompt | ❌ |
| **Voice Management** | `/audio/voices` | GET | - | Query parameters | ❌ |
| **Voice Cloning** | `/audio/voices` | POST | `application/json` | JSON with file reference | ❌ |

---

## Authentication

All requests require the `STEP_API_KEY` environment variable:

```bash
export STEP_API_KEY="your-stepfun-api-key-here"
```

**Header Format:**
```
Authorization: Bearer $STEP_API_KEY
```

---

## Rate Limits & Best Practices

1. **Chat Completions**: High throughput, supports streaming for real-time applications
2. **Audio Processing**: May have longer processing times, use appropriate timeouts
3. **Image Generation**: Computationally intensive, expect 10-30 second response times
4. **File Uploads**: Use multipart/form-data for binary content (audio, images)
5. **Error Handling**: Check HTTP status codes and response format for proper error handling

These examples provide a comprehensive reference for integrating StepFun's APIs across all model categories in your applications.