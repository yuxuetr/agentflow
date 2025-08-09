# StepFun Specialized APIs

AgentFlow provides comprehensive support for StepFun's specialized APIs beyond standard chat completions, including image generation, text-to-speech, automatic speech recognition, and voice cloning capabilities.

## Overview

The StepFun specialized APIs enable you to:

- **Generate images from text** (text2image)
- **Transform images using other images** (image2image) 
- **Edit images with text instructions** (imageedit)
- **Synthesize speech from text** (TTS)
- **Transcribe audio to text** (ASR)
- **Clone voices from audio samples** (voice cloning)
- **Manage voice collections** (voice management)

## Quick Start

```rust
use agentflow_llm::{AgentFlow, Text2ImageBuilder, TTSBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create StepFun specialized client
    let api_key = std::env::var("STEPFUN_API_KEY")?;
    let stepfun_client = AgentFlow::stepfun_client(&api_key).await?;

    // Generate an image
    let image_request = Text2ImageBuilder::new("step-1x-medium", "A beautiful sunset")
        .size("1024x1024")
        .response_format("b64_json")
        .build();
    
    let image_response = stepfun_client.text_to_image(image_request).await?;
    println!("Generated {} images", image_response.data.len());

    // Synthesize speech
    let tts_request = TTSBuilder::new("step-tts-mini", "Hello world!", "default_voice")
        .response_format("mp3")
        .speed(1.2)
        .build();
    
    let audio_data = stepfun_client.text_to_speech(tts_request).await?;
    println!("Generated {} bytes of audio", audio_data.len());

    Ok(())
}
```

## Image Generation APIs

### Text-to-Image (text2image)

Generate images from text descriptions using StepFun's image generation models.

```rust
use agentflow_llm::{AgentFlow, Text2ImageBuilder};

// Basic usage
let image_request = Text2ImageBuilder::new("step-1x-medium", "A cyberpunk cityscape")
    .size("1280x800")
    .response_format("b64_json")
    .build();

let response = stepfun_client.text_to_image(image_request).await?;
for image in response.data {
    println!("Generated image with seed: {}", image.seed);
    if let Some(b64_data) = image.b64_json {
        // Process base64 image data
        println!("Image data length: {}", b64_data.len());
    }
}

// Advanced usage with style reference
let advanced_request = Text2ImageBuilder::new("step-1x-medium", "Portrait of a warrior")
    .size("1024x1024")
    .seed(42)
    .steps(50)
    .cfg_scale(7.5)
    .style_reference("https://example.com/reference.jpg", Some(0.8))
    .build();

let styled_response = stepfun_client.text_to_image(advanced_request).await?;
```

**Supported Parameters:**
- `model`: Image generation model (e.g., "step-1x-medium")
- `prompt`: Text description of desired image
- `size`: Image dimensions ("1024x1024", "512x512", "1280x800", etc.)
- `response_format`: Output format ("b64_json" or "url")
- `seed`: Random seed for reproducible results
- `steps`: Generation steps (1-100)
- `cfg_scale`: Classifier-free guidance scale (1-10)
- `style_reference`: Reference image for style transfer

### Image-to-Image (image2image)

Transform existing images using text prompts and reference images.

```rust
use agentflow_llm::Image2ImageRequest;

let request = Image2ImageRequest {
    model: "step-1x-medium".to_string(),
    prompt: "Transform into watercolor painting style".to_string(),
    source_url: "https://example.com/source.jpg".to_string(),
    source_weight: 0.7, // Influence of source image (0-1)
    size: Some("1024x1024".to_string()),
    response_format: Some("b64_json".to_string()),
    seed: Some(123),
    steps: Some(40),
    cfg_scale: Some(6.0),
    n: None,
};

let response = stepfun_client.image_to_image(request).await?;
```

### Image Editing (imageedit)

Edit images using natural language instructions via multipart form upload.

```rust
use agentflow_llm::ImageEditRequest;
use std::fs;

let image_data = fs::read("input_image.jpg")?;

let request = ImageEditRequest {
    model: "step-1x-medium".to_string(),
    image_data,
    image_filename: "input_image.jpg".to_string(),
    prompt: "Remove the background and make it transparent".to_string(),
    seed: Some(456),
    steps: Some(28),
    cfg_scale: Some(6.0),
    size: Some("1024x1024".to_string()),
    response_format: Some("b64_json".to_string()),
};

let response = stepfun_client.edit_image(request).await?;
```

## Audio APIs

### Text-to-Speech (TTS)

Convert text to natural-sounding speech with various voice options and emotional controls.

```rust
use agentflow_llm::TTSBuilder;

// Basic TTS
let basic_request = TTSBuilder::new("step-tts-mini", "Hello world!", "default_voice")
    .response_format("mp3")
    .speed(1.0)
    .build();

let audio_data = stepfun_client.text_to_speech(basic_request).await?;

// Advanced TTS with Chinese and emotions
let chinese_request = TTSBuilder::new(
    "step-tts-vivid",
    "你好！欢迎使用语音合成！",
    "chinese_voice_01"
)
.response_format("wav")
.speed(1.2)
.volume(1.5)
.language("中文")
.emotion("高兴")    // Happy emotion
.style("正常")       // Normal style
.sample_rate(24000)
.build();

let chinese_audio = stepfun_client.text_to_speech(chinese_request).await?;
```

**Supported Parameters:**
- `model`: TTS model ("step-tts-mini", "step-tts-vivid")
- `input`: Text to synthesize (max 1000 characters)
- `voice`: Voice ID
- `response_format`: Audio format ("wav", "mp3", "flac", "opus")
- `speed`: Speaking speed (0.5-2.0)
- `volume`: Audio volume (0.1-2.0)
- `language`: Language setting (粤语, 四川话, 日语, etc.)
- `emotion`: Emotional tone (高兴, 生气, 悲伤, etc.)
- `style`: Speaking style (慢速, 快速, etc.)
- `sample_rate`: Audio quality (8000, 16000, 22050, 24000)

### Automatic Speech Recognition (ASR)

Transcribe audio files to text with multiple output formats.

```rust
use agentflow_llm::ASRRequest;
use std::fs;

let audio_data = fs::read("speech.mp3")?;

// JSON transcription
let json_request = ASRRequest {
    model: "step-asr".to_string(),
    response_format: "json".to_string(),
    audio_data: audio_data.clone(),
    filename: "speech.mp3".to_string(),
};

let transcription = stepfun_client.speech_to_text(json_request).await?;
println!("Transcription: {}", transcription);

// SRT subtitles
let srt_request = ASRRequest {
    model: "step-asr".to_string(),
    response_format: "srt".to_string(),
    audio_data,
    filename: "speech.mp3".to_string(),
};

let subtitles = stepfun_client.speech_to_text(srt_request).await?;
println!("SRT subtitles:\n{}", subtitles);
```

**Supported Output Formats:**
- `json`: JSON with transcript and metadata
- `text`: Plain text transcript
- `srt`: SubRip subtitle format
- `vtt`: WebVTT subtitle format

### Voice Cloning

Create custom voice models from audio samples for personalized speech synthesis.

```rust
use agentflow_llm::VoiceCloningRequest;

// Clone a voice (requires pre-uploaded audio file)
let clone_request = VoiceCloningRequest {
    model: "step-voice-clone".to_string(),
    text: "This is a test of voice cloning.".to_string(),
    file_id: "uploaded-sample-123".to_string(), // From file upload API
    sample_text: Some("Original voice sample text".to_string()),
};

let cloned_voice = stepfun_client.clone_voice(clone_request).await?;
println!("Created voice with ID: {}", cloned_voice.id);

if cloned_voice.duplicated.unwrap_or(false) {
    println!("⚠️ This voice was detected as duplicate");
}
```

### Voice Management

List and manage your collection of cloned voices.

```rust
// List all voices with pagination
let voices = stepfun_client.list_voices(
    Some(10),           // limit
    Some("desc".to_string()), // order 
    None,               // before cursor
    None                // after cursor
).await?;

println!("Found {} voices (has_more: {})", voices.data.len(), voices.has_more);

for voice in voices.data {
    println!("Voice ID: {} (Created: {})", voice.id, voice.created_at);
}
```

## Convenience Methods

AgentFlow provides convenient builder methods for common operations:

```rust
// Text-to-image convenience
let image_request = AgentFlow::text2image("step-1x-medium", "Serene mountain lake")
    .size("1024x1024")
    .cfg_scale(7.0)
    .response_format("b64_json")
    .build();

// TTS convenience  
let tts_request = AgentFlow::text_to_speech("step-tts-mini", "Welcome!", "default")
    .response_format("mp3")
    .speed(1.1)
    .emotion("friendly")
    .build();
```

## Error Handling

All StepFun specialized APIs return detailed error information for failed requests:

```rust
match stepfun_client.text_to_image(invalid_request).await {
    Ok(response) => {
        // Process successful response
        for image in response.data {
            if image.finish_reason == "success" {
                // Process successful image
            } else if image.finish_reason == "content_filtered" {
                println!("Image was filtered due to content policy");
            }
        }
    },
    Err(e) => {
        eprintln!("Image generation failed: {}", e);
        // Handle specific error types
    }
}
```

## Model Types Integration

StepFun specialized APIs integrate with AgentFlow's granular model type system:

```rust
// Models are automatically classified with appropriate types
// - step-1x-medium: ModelType::Text2Image
// - step-tts-mini: ModelType::Tts  
// - step-asr: ModelType::Asr

// This enables automatic capability validation
let config = ModelRegistry::global().get_model("step-1x-medium")?;
let model_type = config.granular_type(); // ModelType::Text2Image
println!("Supports images: {}", model_type.supports_images());
println!("Output type: {:?}", model_type.primary_output());
```

## Best Practices

### Image Generation
- Use descriptive, specific prompts for better results
- Experiment with different `cfg_scale` values (6-8 works well)
- Set consistent seeds for reproducible results  
- Consider image size based on use case (1024x1024 for general use)

### Text-to-Speech
- Keep input text under 1000 characters for optimal quality
- Choose appropriate sample rates (24000Hz for high quality)
- Test different emotions and styles for your use case
- Use consistent voices for coherent multi-part audio

### Speech Recognition
- Use high-quality audio files (16kHz+ sample rate)
- Choose appropriate output format (JSON for metadata, SRT for subtitles)
- Handle different audio formats (mp3, wav, m4a, ogg, webm, etc.)

### Voice Cloning
- Provide clean, clear audio samples (3-10 minutes recommended)
- Include diverse speech patterns in training samples
- Use sample_text parameter to improve cloning accuracy
- Check for duplicates to avoid redundant voice models

## Examples

See the comprehensive example at `examples/stepfun_specialized_apis.rs` for complete demonstrations of all StepFun specialized APIs.

```bash
# Run the example (requires STEPFUN_API_KEY environment variable)
cargo run --example stepfun_specialized_apis
```

## Configuration

Configure StepFun specialized models in your `models.yml`:

```yaml
models:
  # Image generation models
  step-1x-medium:
    vendor: step
    type: text2image
    supports_streaming: false
    supports_tools: false
    
  # TTS models  
  step-tts-mini:
    vendor: step
    type: tts
    supports_streaming: true
    supports_tools: false
    
  # ASR models
  step-asr:
    vendor: step 
    type: asr
    supports_streaming: false
    supports_tools: false
    supports_multimodal: true  # Audio input
```

## API Reference

### StepFunSpecializedClient Methods

- `text_to_image(request: Text2ImageRequest) -> Result<ImageGenerationResponse>`
- `image_to_image(request: Image2ImageRequest) -> Result<ImageGenerationResponse>`
- `edit_image(request: ImageEditRequest) -> Result<ImageGenerationResponse>`
- `text_to_speech(request: TTSRequest) -> Result<Vec<u8>>`
- `clone_voice(request: VoiceCloningRequest) -> Result<VoiceCloningResponse>`
- `list_voices(limit, order, before, after) -> Result<VoiceListResponse>`
- `speech_to_text(request: ASRRequest) -> Result<String>`

### Builder Patterns

- `Text2ImageBuilder`: Fluent API for image generation requests
- `TTSBuilder`: Fluent API for text-to-speech requests

### Request Types

- `Text2ImageRequest`: Text-to-image generation parameters
- `Image2ImageRequest`: Image-to-image transformation parameters  
- `ImageEditRequest`: Image editing with instructions
- `TTSRequest`: Text-to-speech synthesis parameters
- `ASRRequest`: Automatic speech recognition parameters
- `VoiceCloningRequest`: Voice cloning parameters

### Response Types

- `ImageGenerationResponse`: Image generation results
- `VoiceCloningResponse`: Voice cloning results
- `VoiceListResponse`: Voice management listings

This comprehensive specialized API support makes AgentFlow a powerful platform for multimodal AI applications using StepFun's advanced capabilities.