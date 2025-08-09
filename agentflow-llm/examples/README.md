# StepFun API Examples

This directory contains comprehensive real API test examples for all StepFun model types, demonstrating actual HTTP requests and responses with the StepFun API.

## Quick Start

```bash
# Set your API key
export STEP_API_KEY="your-stepfun-api-key-here"

# Run all tests
cargo run --example stepfun_comprehensive_test_runner

# Run specific model category
cargo run --example stepfun_text_models
```

## Available Examples

### 📝 Text Models
- **File:** `stepfun_text_models.rs`
- **Models:** step-2-16k, step-1-32k, step-2-mini, step-1-256k
- **Features:** Streaming and non-streaming chat completions
- **Tests:** Code generation, explanations, long-context processing

### 🖼️ Image Understanding
- **File:** `stepfun_image_understanding.rs`
- **Models:** step-1o-turbo-vision, step-1v-8k, step-1v-32k, step-3
- **Features:** Vision analysis, chart interpretation, multimodal reasoning
- **Tests:** Image description, analytical capabilities, comprehensive analysis

### 🔊 Text-to-Speech (TTS)
- **File:** `stepfun_tts_models.rs`
- **Models:** step-tts-vivid, step-tts-mini
- **Features:** Voice synthesis, emotional control, multilingual support
- **Tests:** Basic synthesis, emotional voices, Cantonese/Sichuan dialects, advanced parameters
- **Output:** MP3, WAV, OPUS audio files

### 🎤 Speech Recognition (ASR)
- **File:** `stepfun_asr_models.rs`
- **Models:** step-asr
- **Features:** Multi-format transcription, subtitle generation
- **Tests:** JSON, text, SRT, VTT output formats
- **Input:** WAV, MP3, FLAC audio files

### 🎨 Image Generation
- **File:** `stepfun_image_generation.rs`
- **Models:** step-2x-large, step-1x-medium, step-1x-edit
- **Features:** Text-to-image, style reference, image editing
- **Tests:** Basic generation, advanced parameters, style transfer, batch processing
- **Output:** PNG images via URL or base64

### 🚀 Comprehensive Test Runner
- **File:** `stepfun_comprehensive_test_runner.rs`
- **Features:** Runs all tests, performance monitoring, detailed reporting
- **Usage:**
  ```bash
  # Run all tests
  cargo run --example stepfun_comprehensive_test_runner
  
  # Run specific categories
  cargo run --example stepfun_comprehensive_test_runner -- --categories text,tts
  
  # Enable benchmarking
  cargo run --example stepfun_comprehensive_test_runner -- --benchmark
  ```

### 📚 Complete API Demo
- **File:** `stepfun_comprehensive_demo.rs`
- **Features:** Shows correct usage patterns for each model type
- **Purpose:** Reference implementation for integrating with agentflow-llm

## API Endpoint Summary

| Model Type | Endpoint | Method | Streaming | Input | Output |
|------------|----------|--------|-----------|-------|--------|
| **Text** | `/chat/completions` | POST | ✅ | Text messages | Text responses |
| **Image Understanding** | `/chat/completions` | POST | ✅ | Text + Images | Text analysis |
| **Multimodal** | `/chat/completions` | POST | ✅ | Mixed content | Text responses |
| **TTS** | `/audio/speech` | POST | ❌ | Text | Audio files |
| **ASR** | `/audio/transcriptions` | POST | ❌ | Audio files | Text/Subtitles |
| **Image Generation** | `/images/generations` | POST | ❌ | Text prompts | Images |
| **Image Editing** | `/images/edits` | POST | ❌ | Images + Text | Edited images |

## Environment Setup

```bash
# Required environment variable
export STEP_API_KEY="your-stepfun-api-key-here"

# Optional: Enable debug logging
export RUST_LOG=debug

# Optional: Benchmark mode (for test runner)
export STEPFUN_BENCHMARK_MODE=1
```

## Features Demonstrated

✅ **Real API Calls** - All examples make actual HTTP requests to StepFun API  
✅ **Response Validation** - Comprehensive validation of API responses  
✅ **Error Handling** - Proper error handling and reporting  
✅ **Performance Metrics** - Timing and efficiency measurements  
✅ **File I/O** - Audio/image file generation and processing  
✅ **Multiple Formats** - Support for various input/output formats  
✅ **Batch Processing** - Parallel request handling examples  
✅ **Parameter Tuning** - Advanced parameter configuration  

## Generated Files

When running the examples, various output files are generated:

### Audio Files (TTS)
- `stepfun_tts_basic.mp3` - Basic voice synthesis
- `stepfun_tts_mini_fast.wav` - Fast generation
- `stepfun_tts_emotional.mp3` - Emotional voice
- `stepfun_tts_cantonese.mp3` - Cantonese synthesis
- `stepfun_tts_advanced.opus` - High-quality output

### Transcription Files (ASR)
- `transcription.srt` - SubRip subtitle format
- `transcription.vtt` - WebVTT format
- `sample_audio.wav` - Generated test audio

### Image Files (Generation)
- `stepfun_generated_*.png` - Various generated images
- `stepfun_edited_*.png` - Image editing results
- `base_image_for_edit.png` - Source image for editing

### Reports
- `stepfun_test_report.md` - Comprehensive test results (from test runner)

## Dependencies

The examples require these additional dependencies:

```toml
[dependencies]
reqwest = { version = "0.11", features = ["json", "multipart", "stream"] }
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
base64 = "0.21"
futures-util = "0.3"
env_logger = "0.10"
chrono = { version = "0.4", features = ["serde"] }
```

## Performance Expectations

Based on real testing:

- **Text Models:** 1-5 seconds per request
- **Image Understanding:** 2-8 seconds per request  
- **TTS:** 3-10 seconds per request
- **ASR:** 2-6 seconds per request
- **Image Generation:** 15-60 seconds per request

## Troubleshooting

### Common Issues

1. **API Key Error**
   ```
   Error: STEP_API_KEY environment variable is required
   ```
   Solution: Set your API key with `export STEP_API_KEY="your-key"`

2. **Network Timeouts**
   - Image generation may take longer - this is normal
   - Increase timeout if needed in production

3. **File Permissions**
   - Ensure write permissions for output files
   - Check disk space for audio/image files

4. **Rate Limits**
   - The test runner includes delays between requests
   - Adjust timing if you encounter rate limiting

### Debug Mode

Run with detailed logging:
```bash
RUST_LOG=debug cargo run --example stepfun_text_models
```

## Contributing

When adding new examples:

1. Follow the naming pattern: `stepfun_[category]_[type].rs`
2. Include comprehensive error handling
3. Add performance timing and validation
4. Generate appropriate output files
5. Update this README with new examples

## Documentation

For detailed API documentation, see:
- `../docs/stepfun-api-examples.md` - Complete API reference
- Individual example files contain inline documentation
- Test runner generates detailed reports

## License

These examples are provided as reference implementations for the StepFun API integration.