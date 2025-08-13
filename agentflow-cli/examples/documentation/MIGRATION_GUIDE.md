# Migration Guide: Old Examples → New CLI

This guide helps you migrate from the old fragmented examples to the unified AgentFlow CLI.

## 📋 Overview of Changes

| **Old Approach** | **New Approach** | **Status** |
|------------------|------------------|------------|
| Separate Rust binaries | Unified CLI commands | ✅ **Migrated** |
| Complex shell scripts | Simple CLI calls | ✅ **Migrated** |
| Inconsistent interfaces | Unified parameter patterns | ✅ **Migrated** |
| Manual compilation needed | No compilation needed | ✅ **Improved** |
| Limited discoverability | Full help system | ✅ **Improved** |

## 🔄 Command Migrations

### Image Generation

#### ❌ Old Way (Rust Binary)
```bash
# Required compiling separate binary
cargo run --example stepfun_image_demo -- \
  step-1x-medium \
  "A beautiful sunset" \
  1024x1024 \
  sunset.png \
  b64_json
```

#### ✅ New Way (Unified CLI)  
```bash
# No compilation needed, intuitive parameters
agentflow image generate "A beautiful sunset" \
  --model step-1x-medium \
  --size 1024x1024 \
  --output sunset.png \
  --format b64_json
```

**Migration Benefits:**
- 🎯 **Intuitive**: Named parameters instead of positional arguments
- 🔍 **Discoverable**: `agentflow image generate --help` shows all options
- ⚡ **Fast**: No compilation step required

### Image Understanding

#### ❌ Old Way (Inconsistent)
```bash
# Used different command pattern than other image operations
agentflow llm prompt "Describe this image" \
  --file image.jpg \
  --model step-1v-8k
```

#### ✅ New Way (Consistent)
```bash
# Dedicated image understanding command
agentflow image understand image.jpg "Describe this image" \
  --model step-1v-8k \
  --output description.txt
```

**Migration Benefits:**
- 🎨 **Consistent**: Matches other image commands
- 📝 **Clear**: Dedicated command for image analysis
- 💾 **Output**: Built-in file output support

### Text-to-Speech

#### ❌ Old Way (Shell Script)
```bash
# Required editing shell script variables
./stepfun_tts_cli.sh
# Limited parameter options
```

#### ✅ New Way (Full CLI)
```bash
# Rich parameter support
agentflow audio tts "Hello world" \
  --voice cixingnansheng \
  --format mp3 \
  --speed 1.0 \
  --output hello.mp3
```

**Migration Benefits:**
- 🎛️ **Comprehensive**: All parameters available as CLI options
- 🚀 **No Scripts**: Direct command execution
- 📊 **Flexible**: Multiple output formats supported

### Speech Recognition

#### ❌ Old Way (Shell Script)
```bash
# Required manual script setup
./stepfun_asr_cli.sh
# Limited format options
```

#### ✅ New Way (Rich Options)
```bash
# Multiple output formats and languages
agentflow audio asr recording.wav \
  --format json \
  --language zh \
  --output transcript.json
```

**Migration Benefits:**
- 📊 **Multiple Formats**: text, json, srt, vtt support
- 🌍 **Language Support**: Explicit language specification
- 🔧 **No Setup**: Direct execution without script editing

## 🗂️ File Organization Migration

### Old Structure Problems
```
examples/
├── stepfun_image_demo.rs          # Separate binary
├── stepfun_vision_demo.rs         # Another separate binary  
├── stepfun_tts_cli.sh            # Shell script
├── stepfun_asr_cli.sh            # Another shell script
├── stepfun_complete_demo.sh      # 467-line monolithic script
├── various_readme_files.md       # Scattered documentation
└── mixed_sample_files/           # Disorganized assets
```

### New Organized Structure
```
examples/
├── README.md                     # Clear main overview
├── documentation/               # Organized guides
│   ├── GETTING_STARTED.md      # Step-by-step setup
│   ├── COMMANDS_REFERENCE.md   # Complete reference
│   └── TROUBLESHOOTING.md      # Solutions
├── tests/                      # Validation scripts
│   ├── test_all_commands.sh   # Comprehensive testing
│   └── quick_validation.sh    # Fast verification
├── tutorials/                  # Learning examples
│   ├── 01_quick_start.sh      # Basic tutorial
│   ├── 02_image_workflows.sh  # Image processing
│   └── 03_audio_workflows.sh  # Audio processing
└── assets/                     # Sample files
    ├── sample_images/         # Test images
    └── generated_examples/    # Example outputs
```

## 📝 Script Migration Examples

### Old Script Pattern
```bash
#!/bin/bash
# Old approach - complex setup

# Set variables
MODEL="step-1x-medium"
PROMPT="A landscape"
SIZE="1024x1024"
OUTPUT="output.png"

# Compile and run
cargo run --example stepfun_image_demo -- "$MODEL" "$PROMPT" "$SIZE" "$OUTPUT" "b64_json"

# Limited error handling
if [ $? -ne 0 ]; then
    echo "Failed"
    exit 1
fi
```

### New Script Pattern
```bash
#!/bin/bash
# New approach - simple and robust

set -e  # Exit on error

# Generate image with clear parameters
agentflow image generate "A landscape" \
    --model step-1x-medium \
    --size 1024x1024 \
    --output landscape.png

# Analyze the generated image
agentflow image understand landscape.png \
    "Describe the landscape and mood" \
    --output analysis.txt

echo "✅ Workflow completed successfully!"
echo "Generated: landscape.png, analysis.txt"
```

**Migration Benefits:**
- 🛡️ **Better Errors**: Rich error messages with actionable solutions
- 🔧 **No Compilation**: Direct command execution
- 📋 **Clear Intent**: Self-documenting parameter names

## ⚡ Quick Migration Checklist

### For Image Generation Users
- [ ] Replace `cargo run --example stepfun_image_demo` with `agentflow image generate`
- [ ] Convert positional arguments to named parameters (`--model`, `--size`, `--output`)
- [ ] Remove compilation steps from scripts
- [ ] Add error handling for API key issues

### For Image Understanding Users
- [ ] Replace `agentflow llm prompt ... --file` with `agentflow image understand`
- [ ] Update parameter order: `understand image.jpg "prompt"`
- [ ] Add `--output` parameter for saving analysis

### For Audio Processing Users
- [ ] Replace shell scripts with `agentflow audio tts` and `agentflow audio asr`
- [ ] Convert script variables to CLI parameters
- [ ] Update voice names (use `cixingnansheng` for reliable results)
- [ ] Add format specifications (`--format json` for structured output)

### For Workflow Automation
- [ ] Update shell scripts to use unified CLI commands
- [ ] Add proper error handling with `set -e`
- [ ] Use the new organized directory structure
- [ ] Replace complex demo scripts with focused examples

## 🚨 Breaking Changes

### API Key Environment Variables
**Old**: Various provider-specific variables  
**New**: Use `STEP_API_KEY` (or `STEPFUN_API_KEY`)

```bash
# Update your environment setup
export STEP_API_KEY="your-stepfun-api-key"
```

### Command Structure
**Old**: Inconsistent patterns across different operations  
**New**: Consistent `agentflow [category] [action]` pattern

### File Paths
**Old**: Sometimes required absolute paths  
**New**: Relative paths work consistently, quotes handle spaces

### Error Messages
**Old**: Generic error messages  
**New**: Specific, actionable error messages

## 🔧 Migration Tools

### Automatic Structure Validation
Run this to ensure your new setup works:
```bash
./tests/test_cli_structure.sh
```

### Compare Old vs New
See the differences:
```bash
./tests/comparison_demo.sh
```

### Validate Migration
Test your migrated workflows:
```bash
export STEP_API_KEY="your-key"
./tests/test_all_commands.sh
```

## 💡 Migration Tips

### 1. Start with Structure Test
Always run `./tests/test_cli_structure.sh` first to validate CLI installation.

### 2. Use Help System
Discover parameters with `agentflow [command] --help` instead of guessing.

### 3. Incremental Migration
Migrate one workflow at a time, test each step.

### 4. Leverage Aliases
Use short forms (`gen`, `analyze`, `tts`, `asr`) for interactive use.

### 5. Script with Full Names
Use full command names in scripts for clarity.

## 🎯 What You Gain

✅ **Unified Experience**: One command for everything  
✅ **Better Discovery**: Rich help system  
✅ **Consistent Interface**: Same patterns across all operations  
✅ **No Compilation**: Direct execution  
✅ **Rich Error Messages**: Clear guidance when things go wrong  
✅ **Professional UX**: Industry-standard CLI patterns  
✅ **Future-Proof**: Extensible architecture for new features  

---

🎉 **Migration Complete!** You now have access to a professional, unified CLI experience.

**Next Step**: Try `./tutorials/01_quick_start.sh` to see everything in action!