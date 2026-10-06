# Getting Started with Yanshi CLI

This guide will take you from zero to running your first Yanshi commands in just a few minutes.

## 📋 Prerequisites

### System Requirements
- **Rust**: Version 1.70+ (for installation from source)
- **Operating System**: macOS, Linux, or Windows
- **Internet Connection**: For API calls to StepFun services

### StepFun Account
1. Visit [StepFun](https://www.stepfun.com/) and create an account
2. Navigate to API settings and generate an API key
3. Copy your API key - you'll need it shortly

## 🔧 Installation

### Option 1: Install from Source (Recommended)
```bash
# Clone the repository (if you haven't already)
git clone https://github.com/your-org/yanshi
cd yanshi

# Install Yanshi CLI
cargo install --path yanshi-cli

# Verify installation
yanshi --version
```

### Option 2: Build for Development
```bash
# For development and testing
cargo build --package yanshi-cli

# Use the development binary
./target/debug/yanshi --help
```

## 🔑 API Key Setup

### Method 1: Environment Variable (Recommended)
```bash
# Set API key for current session
export STEP_API_KEY="your-stepfun-api-key-here"

# Add to your shell profile for persistence
echo 'export STEP_API_KEY="your-stepfun-api-key-here"' >> ~/.bashrc
# or for zsh:
echo 'export STEP_API_KEY="your-stepfun-api-key-here"' >> ~/.zshrc
```

### Method 2: Environment File
```bash
# Create .env file in your project directory
echo "STEP_API_KEY=your-stepfun-api-key-here" > .env

# Yanshi will automatically load this
```

### Verify API Key Setup
```bash
# This should not show an error about missing API key
yanshi image generate --help
```

## ✅ First Commands

### 1. Check CLI Structure
```bash
# Discover all available commands
yanshi --help

# Explore image commands
yanshi image --help

# Explore audio commands
yanshi audio --help
```

### 2. Test with Simple Commands
```bash
# Generate a simple image (this makes an API call)
yanshi image generate "A red circle on white background" \
  --size 512x512 \
  --output my_first_image.png

# Check if the image was created
ls -la my_first_image.png

# Create a simple audio file
yanshi audio tts "Hello from Yanshi!" \
  --voice cixingnansheng \
  --output my_first_audio.mp3

# Check if the audio was created  
ls -la my_first_audio.mp3
```

### 3. Try Image Understanding
```bash
# Analyze the image you just created
yanshi image understand my_first_image.png \
  "What do you see in this image?" \
  --output image_analysis.txt

# Read the analysis
cat image_analysis.txt
```

## 🚦 Validation Steps

### Step 1: CLI Installation Check
```bash
# Should show Yanshi CLI help
yanshi --help | head -5
```

Expected output:
```
Yanshi CLI provides a unified interface for workflow execution and LLM interaction.
Supports YAML-based workflow configuration, direct LLM commands, and comprehensive
multimodal input handling for automation and development workflows.

Usage: yanshi [OPTIONS] <COMMAND>
```

### Step 2: API Key Check
```bash
# Should show proper error message (not "API key missing for provider 'step'")
yanshi image generate "test" --output test.png 2>&1 | grep -E "(API key|HTTP|error)"
```

With correct API key, you should see image generation progress.  
With incorrect/missing key, you should see: `HTTP request failed: 401 - {"error":{"message":"Incorrect API key provided"...}`

### Step 3: Functionality Check
Run the comprehensive validation:
```bash
# Navigate to examples directory
cd yanshi-cli/examples

# Run structure validation (no API calls)
./tests/test_cli_structure.sh

# Run quick API validation (requires valid API key)
./tests/quick_validation.sh
```

## 🎯 Next Steps

### Ready to Learn More?
1. **Basic Tutorial**: Run `./tutorials/01_quick_start.sh`
2. **Image Workflows**: Run `./tutorials/02_image_workflows.sh`  
3. **Audio Processing**: Run `./tutorials/03_audio_workflows.sh`
4. **Full Reference**: Read [`COMMANDS_REFERENCE.md`](COMMANDS_REFERENCE.md)

### Ready for Production?
1. **Test Everything**: Run `./tests/test_all_commands.sh`
2. **Integration**: See examples in `tutorials/` directory
3. **Automation**: All commands are script-friendly and return proper exit codes

## 🆘 Troubleshooting Quick Fixes

### "command not found: yanshi"
- **Solution**: Run `cargo install --path yanshi-cli` again
- **Check**: Ensure `~/.cargo/bin` is in your `$PATH`

### "API key missing for provider 'step'"
- **Old Error**: This was from the previous version - you shouldn't see this
- **If you see this**: You may have an old version installed
- **Solution**: Reinstall with `cargo install --path yanshi-cli --force`

### "Incorrect API key provided"
- **Solution**: Double-check your StepFun API key
- **Check**: Ensure no extra spaces or quotes in the key
- **Verify**: Log into StepFun dashboard and regenerate if needed

### Command works but no output file
- **Check**: Permissions in the output directory
- **Try**: Use absolute paths for output files
- **Debug**: Add `--verbose` flag to commands for more details

### Image generation is very slow
- **Normal**: First generation may take 15-30 seconds
- **Optimize**: Use smaller sizes for testing (`--size 512x512`)
- **Reduce**: Lower `--steps` parameter for faster generation

---

🎉 **You're Ready!** With Yanshi CLI installed and configured, you can now explore all the examples and tutorials.

**Next recommended step**: Try `./tutorials/01_quick_start.sh` for a hands-on tour!