# AgentFlow CLI Examples

Welcome to the AgentFlow CLI examples directory! This collection demonstrates all capabilities of the unified AgentFlow CLI, including text generation, image creation, image understanding, audio processing, and more.

## 📁 Directory Structure

```
examples/
├── README.md                     # This file - main overview and quick start
├── assets/                      # Sample files and generated outputs
│   ├── sample_images/          # Test images for understanding
│   ├── stepfun_image_examples/ # Generated image samples
│   ├── stepfun_vision_examples/
│   └── agentflow_demo_output/  # Generated demo files
├── documentation/              # Detailed guides and references
│   ├── GETTING_STARTED.md     # Step-by-step setup guide
│   ├── COMMANDS_REFERENCE.md  # Complete command reference
│   ├── MIGRATION_GUIDE.md     # Migrating from old examples
│   └── TROUBLESHOOTING.md     # Common issues and solutions
├── tests/                      # Test scripts and validation
│   ├── test_all_commands.sh   # Comprehensive functionality test
│   ├── test_cli_structure.sh  # CLI structure validation
│   └── quick_validation.sh    # Quick API connectivity test
└── tutorials/                  # Step-by-step learning examples
    ├── 01_quick_start.sh      # Basic usage tutorial
    ├── 02_image_workflows.sh  # Image generation and understanding
    └── 03_audio_workflows.sh  # Audio processing examples
```

## 🚀 Quick Start

### Prerequisites

1. **Install AgentFlow CLI**:
   ```bash
   cargo install --path agentflow-cli
   ```

2. **Set your StepFun API key**:
   ```bash
   export STEP_API_KEY="your-stepfun-api-key-here"
   ```

3. **Verify installation**:
   ```bash
   agentflow --help
   ```

### Try Your First Commands

```bash
# Test connectivity first (recommended)
./tests/test_api_connectivity.sh

# Generate an image
agentflow image generate "A serene mountain landscape at sunset" --output mountain.png

# Understand an image
agentflow image understand mountain.png "Describe the mood and atmosphere"

# Create speech from text
agentflow audio tts "Welcome to AgentFlow CLI!" --output welcome.mp3

# Transcribe audio
agentflow audio asr welcome.mp3 --output transcript.txt
```

## 🧪 Testing and Validation

### Quick Validation
```bash
# Step 1: Test CLI structure (no API calls needed)
./tests/test_cli_structure.sh

# Step 2: Test API connectivity and key validity
export STEP_API_KEY="your-key" && ./tests/test_api_connectivity.sh

# Step 3: Quick functionality test
./tests/quick_validation.sh
```

### Comprehensive Testing
```bash
# Full functionality test with real API calls
export STEP_API_KEY="your-key" && ./tests/test_all_commands.sh
```

## 📚 Learning Path

### 1. **Getting Started** → [`documentation/GETTING_STARTED.md`](documentation/GETTING_STARTED.md)
Complete setup guide from installation to first API call

### 2. **Basic Tutorial** → [`tutorials/01_quick_start.sh`](tutorials/01_quick_start.sh)
Learn core commands with hands-on examples

### 3. **Image Workflows** → [`tutorials/02_image_workflows.sh`](tutorials/02_image_workflows.sh)
Master image generation and understanding

### 4. **Audio Processing** → [`tutorials/03_audio_workflows.sh`](tutorials/03_audio_workflows.sh)
Explore text-to-speech and speech recognition

### 5. **Commands Reference** → [`documentation/COMMANDS_REFERENCE.md`](documentation/COMMANDS_REFERENCE.md)
Complete parameter guide for all commands

## 🎯 Key Features Demonstrated

### ✅ **Unified CLI Experience**
- All StepFun capabilities in one command
- Consistent parameter patterns
- Comprehensive help system

### ✅ **Image Processing**
```bash
# Generate: Text → Image
agentflow image generate "cyberpunk city" --size 1024x1024 --output city.png

# Understand: Image → Text  
agentflow image understand photo.jpg "What's happening here?"
```

### ✅ **Audio Processing**
```bash
# Text → Speech
agentflow audio tts "Hello world" --voice cixingnansheng --output hello.mp3

# Speech → Text
agentflow audio asr recording.wav --format json --output transcript.json
```

### ✅ **Developer-Friendly**
- Rich error messages
- File I/O support  
- Command aliases (`gen`, `analyze`, `tts`, `asr`)
- Scriptable and automation-ready

## 🔧 Command Categories

| Command | Purpose | Example |
|---------|---------|---------|
| `agentflow llm` | Text generation and chat | `agentflow llm prompt "Explain AI"` |
| `agentflow image generate` | Create images from text | `agentflow image gen "sunset" -o sunset.png` |
| `agentflow image understand` | Analyze and describe images | `agentflow image analyze photo.jpg "What is this?"` |
| `agentflow audio tts` | Convert text to speech | `agentflow audio tts "Hello" -o hello.mp3` |
| `agentflow audio asr` | Convert speech to text | `agentflow audio asr audio.wav -o transcript.txt` |
| `agentflow config` | Manage configuration | `agentflow config show` |

## 🆘 Need Help?

- **Quick Help**: `agentflow --help` or `agentflow [command] --help`
- **Setup Issues**: See [`documentation/GETTING_STARTED.md`](documentation/GETTING_STARTED.md)
- **Command Issues**: See [`documentation/TROUBLESHOOTING.md`](documentation/TROUBLESHOOTING.md)
- **Migration**: See [`documentation/MIGRATION_GUIDE.md`](documentation/MIGRATION_GUIDE.md)

## 🎉 What's New

**AgentFlow CLI now provides a unified interface for all StepFun capabilities!**

❌ **Before**: Separate Rust binaries, complex shell scripts, inconsistent interfaces  
✅ **After**: One CLI, discoverable commands, professional UX

Experience the difference with:
```bash
./tests/comparison_demo.sh
```

---

**Ready to explore?** Start with `./tutorials/01_quick_start.sh` and dive in! 🚀