# AgentFlow CLI: New Image and Audio Commands

This document describes the newly implemented image and audio commands that provide a unified, discoverable CLI interface for StepFun's specialized APIs.

## 🎯 Overview

Previously, image generation, image understanding, and audio processing required using separate Rust binaries or shell scripts. Now everything is integrated into the main `agentflow` CLI with consistent patterns and comprehensive help.

## 🚀 Quick Start

### Prerequisites
```bash
# Set your StepFun API key
export STEP_API_KEY="your-stepfun-api-key"

# Install AgentFlow CLI (if not already installed)
cargo install --path agentflow-cli
```

### Discovery
```bash
# See all available commands
agentflow --help

# Explore image commands
agentflow image --help

# Explore audio commands  
agentflow audio --help
```

## 🖼️ Image Commands

### Generate Images
```bash
# Basic image generation
agentflow image generate "A serene mountain landscape at sunset" --output landscape.png

# Advanced generation with parameters
agentflow image generate "A cyberpunk cityscape with neon lights" \
  --model step-1x-medium \
  --size 1024x1024 \
  --steps 50 \
  --cfg-scale 7.5 \
  --seed 42 \
  --output cyberpunk.png

# Using alias
agentflow image gen "Abstract art" --output abstract.png
```

### Understand Images
```bash
# Basic image analysis
agentflow image understand photo.jpg "Describe this image in detail"

# Advanced analysis with output
agentflow image understand artwork.png "Analyze the artistic style and composition" \
  --model step-1v-8k \
  --temperature 0.7 \
  --max-tokens 500 \
  --output analysis.md

# Using alias
agentflow image analyze landscape.jpg "What time of day is this?"
```

## 🎧 Audio Commands

### Text-to-Speech (TTS)
```bash
# Basic text-to-speech
agentflow audio tts "Hello, welcome to AgentFlow!" --output welcome.mp3

# Advanced TTS with parameters
agentflow audio tts "This is a professional announcement." \
  --model step-tts-mini \
  --voice professional_male \
  --format wav \
  --speed 0.9 \
  --emotion "formal" \
  --output announcement.wav

# Using alias
agentflow audio tts "Quick test" --output test.mp3
```

### Speech Recognition (ASR)
```bash
# Basic transcription
agentflow audio asr recording.wav --output transcript.txt

# Advanced transcription with JSON output
agentflow audio asr interview.mp3 \
  --model step-asr \
  --format json \
  --language zh \
  --output interview_transcript.json

# Generate subtitles
agentflow audio asr lecture.wav --format srt --output subtitles.srt

# Using alias
agentflow audio asr meeting.wav --output meeting_notes.txt
```

### Voice Cloning
```bash
# Note: Voice cloning requires file upload functionality
# Currently shows informative implementation message
agentflow audio clone reference.wav "Hello from the cloned voice!" --output cloned.mp3

# Using alias
agentflow audio clone sample.wav "Test message" --output test_clone.mp3
```

## 📊 Command Comparison

| Feature | Before | After |
|---------|--------|-------|
| **Image Generation** | `cargo run --example stepfun_image_demo -- args...` | `agentflow image generate "prompt" --output image.png` |
| **Image Understanding** | `agentflow llm prompt "text" --file image.jpg` | `agentflow image understand image.jpg "question"` |
| **Text-to-Speech** | `./stepfun_tts_cli.sh` (limited) | `agentflow audio tts "text" --output audio.mp3` |
| **Speech Recognition** | `./stepfun_asr_cli.sh` (limited) | `agentflow audio asr audio.wav --output transcript.txt` |
| **Voice Cloning** | `./stepfun_voice_cloning_cli.sh` | `agentflow audio clone ref.wav "text" --output clone.mp3` |

## 🎛️ Available Parameters

### Image Generate
- `--model`: Model name (default: step-1x-medium)
- `--size`: Image dimensions (default: 1024x1024)
- `--steps`: Inference steps (default: 30)
- `--cfg-scale`: Guidance scale (default: 7.5)
- `--seed`: Random seed for reproducibility
- `--format`: Output format (b64_json, url)

### Image Understand
- `--model`: Model name (default: step-1v-8k)
- `--temperature`: Creativity (0.0-1.0)
- `--max-tokens`: Response length
- `--output`: Save analysis to file

### Audio TTS
- `--model`: Model name (default: step-tts-mini)
- `--voice`: Voice name (default: default)
- `--format`: Audio format (mp3, wav, flac)
- `--speed`: Speech rate (0.5-2.0)
- `--emotion`: Voice emotion/style

### Audio ASR
- `--model`: Model name (default: step-asr)
- `--format`: Output format (text, json, srt, vtt)
- `--language`: Language code (auto-detect if not specified)

## 🧪 Testing

### Run Tests
```bash
# Test CLI structure (no API calls)
./test_cli_structure.sh

# Full test with API calls (requires valid API key)
./test_new_commands.sh

# Quick API validation
./quick_api_test.sh
```

### View Comparisons
```bash
# See before/after comparison
./comparison_demo.sh
```

## 🔧 Troubleshooting

### API Key Issues
```bash
# Error: "STEPFUN_API_KEY or STEP_API_KEY environment variable must be set"
export STEP_API_KEY="your-actual-api-key"
```

### Command Not Found
```bash
# Install or reinstall AgentFlow CLI
cargo install --path agentflow-cli --force
```

### Voice Cloning Not Working
Voice cloning requires file upload functionality that's not yet implemented in the StepFun client. The command will show an informative error message explaining the next steps.

## 📝 Command Aliases

All commands support shorter aliases for convenience:

| Full Command | Alias |
|-------------|-------|
| `image generate` | `image gen` |
| `image understand` | `image analyze` |
| `audio text-to-speech` | `audio tts` |
| `audio speech-to-text` | `audio asr` |
| `audio voice-clone` | `audio clone` |

## 🎉 Benefits

✅ **Unified Discovery**: All commands discoverable through `agentflow --help`  
✅ **Consistent Interface**: Same parameter patterns across all commands  
✅ **No Compilation**: No need for separate Rust binaries  
✅ **Rich Help**: Comprehensive help for every command and parameter  
✅ **Better Error Messages**: Clear, actionable error information  
✅ **File I/O**: Consistent input/output file handling  
✅ **Aliases**: Short forms for faster usage  
✅ **Modern UX**: Professional CLI experience matching industry standards

## 🚀 Next Steps

1. **Set your API key**: Get a StepFun API key and export it
2. **Try the commands**: Start with simple examples
3. **Read the help**: Use `--help` on any command for details
4. **Integrate workflows**: Use these commands in your automation scripts
5. **Provide feedback**: Report issues or suggestions

---

*This implementation transforms AgentFlow from a collection of separate tools into a unified, professional CLI experience that users can easily discover and use.*