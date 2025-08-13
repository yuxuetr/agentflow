# AgentFlow CLI Commands Reference

Complete reference for all AgentFlow CLI commands, parameters, and usage patterns.

## 🎨 Image Commands

### `agentflow image generate` (alias: `gen`)

Generate images from text descriptions using StepFun's image generation models.

#### Syntax
```bash
agentflow image generate <PROMPT> --output <OUTPUT_FILE> [OPTIONS]
```

#### Required Parameters
- `<PROMPT>`: Text description of the image to generate
- `--output`, `-o`: Output file path (e.g., `image.png`)

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | `step-1x-medium` | Model name (`step-1x-medium`, `step-2x-large`) |
| `--size`, `-s` | `1024x1024` | Image dimensions (`512x512`, `768x768`, `1024x1024`, `1280x800`) |
| `--format`, `-f` | `b64_json` | Response format (`b64_json`, `url`) |
| `--steps` | `30` | Number of inference steps (10-100, higher = better quality) |
| `--cfg-scale` | `7.5` | CFG scale for prompt adherence (1.0-20.0) |
| `--seed` | Random | Seed for reproducible generation (integer) |

#### Examples
```bash
# Basic image generation
agentflow image generate "A sunset over mountains" --output sunset.png

# High quality with specific parameters
agentflow image generate "Cyberpunk cityscape with neon lights" \
  --model step-2x-large \
  --size 1280x800 \
  --steps 50 \
  --cfg-scale 8.0 \
  --seed 42 \
  --output cyberpunk.png

# Using alias
agentflow image gen "Abstract art" --output abstract.png
```

### `agentflow image understand` (alias: `analyze`)

Analyze and describe images using StepFun's vision models.

#### Syntax
```bash
agentflow image understand <IMAGE_PATH> <PROMPT> [OPTIONS]
```

#### Required Parameters
- `<IMAGE_PATH>`: Path to image file (jpg, png, gif, webp, bmp)
- `<PROMPT>`: Question or instruction for analysis

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | `step-1v-8k` | Vision model name |
| `--temperature`, `-t` | `0.7` | Response creativity (0.0-1.0) |
| `--max-tokens` | `800` | Maximum response length |
| `--output`, `-o` | Console | Save analysis to file |

#### Examples
```bash
# Basic image understanding
agentflow image understand photo.jpg "What do you see in this image?"

# Detailed analysis with output file
agentflow image understand artwork.png \
  "Analyze the artistic style, composition, and color palette" \
  --model step-1v-8k \
  --temperature 0.8 \
  --max-tokens 1000 \
  --output detailed_analysis.md

# Using alias
agentflow image analyze document.png "Extract the text from this document"
```

## 🎧 Audio Commands

### `agentflow audio tts` (alias: `audio text-to-speech`)

Convert text to speech using StepFun's TTS models.

#### Syntax
```bash
agentflow audio tts <TEXT> --output <OUTPUT_FILE> [OPTIONS]
```

#### Required Parameters
- `<TEXT>`: Text to convert to speech
- `--output`, `-o`: Output audio file path

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | `step-tts-mini` | TTS model name |
| `--voice`, `-v` | `default` | Voice name (see Voice Options below) |
| `--format`, `-f` | `mp3` | Audio format (`mp3`, `wav`, `flac`) |
| `--speed` | `1.0` | Speech speed (0.5-2.0) |
| `--emotion` | None | Voice emotion/style |

#### Voice Options
- `cixingnansheng`: Male voice (tested)
- `default`: Default voice (may need adjustment)
- Other voices: Check StepFun documentation

#### Examples
```bash
# Basic text-to-speech
agentflow audio tts "Hello from AgentFlow!" --output hello.mp3

# With specific voice and parameters
agentflow audio tts "Welcome to our presentation" \
  --voice cixingnansheng \
  --format wav \
  --speed 0.9 \
  --output welcome.wav

# Emotional speech (if supported by model)
agentflow audio tts "This is exciting news!" \
  --voice cixingnansheng \
  --emotion "高兴" \
  --output excited.mp3
```

### `agentflow audio asr` (alias: `audio speech-to-text`)

Convert speech to text using StepFun's ASR models.

#### Syntax
```bash
agentflow audio asr <AUDIO_FILE> [OPTIONS]
```

#### Required Parameters
- `<AUDIO_FILE>`: Path to audio file (mp3, wav, flac)

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | `step-asr` | ASR model name |
| `--format`, `-f` | `text` | Output format (`text`, `json`, `srt`, `vtt`) |
| `--language`, `-l` | Auto-detect | Language code (`zh`, `en`, `ja`) |
| `--output`, `-o` | Console | Save transcript to file |

#### Examples
```bash
# Basic transcription
agentflow audio asr recording.wav --output transcript.txt

# JSON format with language specification
agentflow audio asr interview.mp3 \
  --format json \
  --language zh \
  --output interview_transcript.json

# Generate subtitles
agentflow audio asr lecture.wav \
  --format srt \
  --output subtitles.srt
```

### `agentflow audio clone` (alias: `audio voice-clone`)

Clone voice from reference audio (currently shows implementation status).

#### Syntax
```bash
agentflow audio clone <REFERENCE_AUDIO> <TEXT> --output <OUTPUT_FILE> [OPTIONS]
```

#### Status
⚠️ **Implementation Note**: Voice cloning requires file upload functionality that's not yet implemented in the StepFun client. The command will show an informative message about the implementation status.

## 💬 LLM Commands

### `agentflow llm prompt`

Send text prompts to language models.

#### Syntax
```bash
agentflow llm prompt <TEXT> [OPTIONS]
```

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | Auto | Model name |
| `--temperature`, `-t` | `0.7` | Response creativity (0.0-1.0) |
| `--max-tokens` | Model default | Maximum response length |
| `--output`, `-o` | Console | Save response to file |
| `--stream` | false | Enable streaming output |
| `--system` | None | System prompt |

#### Examples
```bash
# Basic prompt
agentflow llm prompt "Explain quantum computing"

# With specific model and parameters
agentflow llm prompt "Write a Python function to sort a list" \
  --model step-2-16k \
  --temperature 0.5 \
  --max-tokens 500 \
  --output code_example.py
```

### `agentflow llm chat`

Start an interactive chat session.

#### Syntax
```bash
agentflow llm chat [OPTIONS]
```

#### Optional Parameters
| Parameter | Default | Description |
|-----------|---------|-------------|
| `--model`, `-m` | Auto | Model name |
| `--system` | None | System prompt |
| `--save` | None | Save conversation to file |
| `--load` | None | Load conversation from file |

## ⚙️ Configuration Commands

### `agentflow config init`

Initialize AgentFlow configuration files.

#### Syntax
```bash
agentflow config init [OPTIONS]
```

#### Optional Parameters
| Parameter | Description |
|-----------|-------------|
| `--force`, `-f` | Force overwrite existing configuration |

### `agentflow config show`

Display current configuration.

#### Syntax
```bash
agentflow config show [SECTION]
```

#### Optional Parameters
- `[SECTION]`: Specific configuration section to show

## 🌐 Global Options

These options work with all commands:

| Option | Description |
|--------|-------------|
| `--log-level` | Set log level (`error`, `warn`, `info`, `debug`, `trace`) |
| `--output-format` | Output format (`json`, `yaml`, `text`) |
| `--no-color` | Disable colored output |
| `--verbose`, `-v` | Verbose output |
| `--help`, `-h` | Show help |
| `--version`, `-V` | Show version |

## 🔍 Command Aliases

For faster usage, many commands have shorter aliases:

| Full Command | Alias |
|-------------|-------|
| `image generate` | `image gen` |
| `image understand` | `image analyze` |
| `audio text-to-speech` | `audio tts` |
| `audio speech-to-text` | `audio asr` |
| `audio voice-clone` | `audio clone` |
| `llm prompt` | `llm p` |
| `llm chat` | `llm c` |

## 💡 Usage Tips

### File Paths
- Always use quotes for paths with spaces: `"My Documents/image.png"`
- Relative paths work: `./output/result.png`
- Absolute paths are recommended for scripts

### Output Files
- Parent directories must exist
- File extensions determine format where applicable
- Use `-` as output to write to stdout (where supported)

### Error Codes
- `0`: Success
- `1`: General error (API error, file not found, etc.)
- Check error messages for specific details

### Performance Tips
- Use smaller image sizes for testing
- Lower `--steps` for faster generation
- Use appropriate models for your use case

### Scripting
All commands are script-friendly:
```bash
#!/bin/bash
set -e  # Exit on error

agentflow image generate "Logo design" --output logo.png
if [ -f "logo.png" ]; then
    echo "Logo generated successfully"
    agentflow image understand logo.png "Describe this logo" --output description.txt
fi
```

---

**Need more help?** Use `agentflow [command] --help` for detailed help on any specific command!