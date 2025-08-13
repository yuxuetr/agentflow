# StepFun CLI Examples

This directory contains comprehensive examples demonstrating all StepFun specialized API capabilities through the AgentFlow CLI.

## 🚀 Quick Start

1. **Set up your API key:**
   ```bash
   export STEP_API_KEY="your-stepfun-api-key-here"
   ```

2. **Run the complete demo:**
   ```bash
   ./stepfun_complete_demo.sh
   ```

## 📂 Available Examples

### 🎯 Complete Demonstration
- **`stepfun_complete_demo.sh`** - Comprehensive demo showcasing all StepFun capabilities
- **`quick_start.sh`** - Basic text model examples (already working)

### 🎨 Image Generation
- **`stepfun_image_generation_cli.sh`** - AI-powered image creation
  - Text-to-image generation
  - Style-inspired generation  
  - Image editing
  - Batch processing
  - Advanced parameter control

### 👁️ Image Understanding
- **`stepfun_image_understanding_cli.sh`** - Vision and multimodal analysis
  - Scene description
  - Chart and data analysis
  - OCR and text extraction
  - Multimodal comparison
  - Scientific image analysis

### 🔊 Text-to-Speech
- **`stepfun_tts_cli.sh`** - Natural voice synthesis
  - High-quality speech generation
  - Emotional voice control
  - Multilingual support
  - Voice customization
  - Batch audio processing

### 🎤 Speech Recognition
- **`stepfun_asr_cli.sh`** - Audio transcription
  - Multiple output formats (JSON, text, SRT, VTT)
  - Multi-format audio support
  - Batch processing
  - Real-time pipelines
  - Quality optimization

### 👥 Voice Cloning
- **`stepfun_voice_cloning_cli.sh`** - Custom voice creation
  - Voice cloning from audio samples
  - Voice management and organization
  - Quality assessment
  - Multi-speaker content creation
  - Analytics and insights

## 🛠️ Available Models

### Text Models (LLM)
- **step-2-mini** - Fast, efficient text generation
- **step-2-16k** - Balanced quality and speed
- **step-1-32k** - High context length
- **step-3** - Advanced reasoning capabilities

### Image Generation Models
- **step-1x-medium** - Fast, balanced image generation
- **step-2x-large** - High-quality, detailed images
- **step-1x-edit** - Image editing and modification

### Vision Models
- **step-1o-turbo-vision** - General image understanding
- **step-1v-8k** - Chart and data analysis
- **step-1v-32k** - Detailed comprehensive analysis
- **step-3** - Advanced multimodal reasoning

### Audio Models
- **step-tts-vivid** - High-quality TTS with emotion
- **step-tts-mini** - Fast TTS for real-time use
- **step-asr** - Accurate speech recognition
- **step-voice-clone** - Custom voice creation

## 📋 Prerequisites

1. **AgentFlow CLI installed and built:**
   ```bash
   cargo build --package agentflow-cli --release
   export PATH="$PATH:$HOME/.target/release"
   ```

2. **StepFun API Key:**
   - Get your API key from StepFun platform
   - Set the environment variable: `export STEP_API_KEY="your-key"`

3. **Optional dependencies for some examples:**
   - `ffmpeg` - For audio processing examples
   - `python3` - For some sample file generation
   - `bc` - For mathematical calculations in scripts

## 🚦 Usage Examples

### Basic Text Generation
```bash
agentflow llm prompt \
  --model step-2-mini \
  --max-tokens 200 \
  "请写一个关于人工智能的短文。"
```

### Image Generation
```bash
# Note: These are example commands showing planned CLI syntax
agentflow stepfun image-generate \
  --model step-1x-medium \
  --prompt "未来科技城市夜景，赛博朋克风格" \
  --size 1024x1024 \
  --output cyberpunk_city.png
```

### Text-to-Speech
```bash
# Note: These are example commands showing planned CLI syntax
agentflow stepfun tts \
  --model step-tts-vivid \
  --text "智能阶跃，十倍每一个人的可能。" \
  --voice cixingnansheng \
  --format mp3 \
  --output message.mp3
```

### Speech Recognition
```bash
# Note: These are example commands showing planned CLI syntax
agentflow stepfun asr \
  --model step-asr \
  --audio recording.wav \
  --format json \
  --output transcription.json
```

## 🔧 Configuration

### Environment Variables
- `STEP_API_KEY` - Your StepFun API key (required)
- `STEPFUN_API_KEY` - Alternative variable name (auto-set by scripts)
- `STEPFUN_BASE_URL` - Custom API endpoint (optional)

### Model Configuration
The examples use built-in model configurations. You can customize:
- Temperature settings for creativity
- Max tokens for response length
- Output formats for different use cases
- Batch processing for efficiency

## 📊 Current Status

### ✅ Working Examples
- **Text Models** - Fully functional through existing LLM CLI
- **Image Understanding** - Working via stepfun_vision_demo.rs program
- **Image Generation** - Working via stepfun_image_demo.rs program

### 🚧 Planned CLI Integration
The remaining specialized StepFun APIs (TTS, ASR, voice cloning) are demonstrated through example commands showing the intended CLI syntax. These will be implemented as the AgentFlow CLI is extended with StepFun specialized API support.

### 🔮 Implementation Progress
1. ✅ Text model integration (completed)
2. ✅ Vision model integration (completed via stepfun_vision_demo.rs)
3. ✅ Image generation functionality (completed via stepfun_image_demo.rs)
4. 🚧 TTS CLI commands (planned) 
5. 🚧 ASR CLI commands (planned)
6. 🚧 Voice cloning CLI commands (planned)

## 🎯 Use Cases

### Content Creation
- Generate images for marketing materials
- Create voice-overs for videos
- Transcribe interviews and meetings
- Generate multilingual content

### Development & Automation
- Build voice-enabled applications
- Create automated content pipelines
- Implement accessibility features
- Develop multimodal AI systems

### Business Applications
- Customer service voice bots
- Document analysis and OCR
- Meeting transcription and summarization
- Marketing content generation

## 🆘 Troubleshooting

### Common Issues

1. **API Key Not Set**
   ```bash
   export STEP_API_KEY="your-actual-api-key-here"
   ```

2. **CLI Not Found**
   ```bash
   # Make sure AgentFlow CLI is built and in PATH
   cargo build --package agentflow-cli --release
   export PATH="$PATH:$HOME/.target/release"
   ```

3. **Permission Errors**
   ```bash
   # Make scripts executable
   chmod +x *.sh
   ```

4. **Sample File Issues**
   - Scripts create sample/mock files for demonstration
   - Replace with your actual files for real usage
   - Check file formats and sizes

### Getting Help

1. **Check AgentFlow documentation**
2. **Review error messages in script output**
3. **Test with simple examples first**
4. **Verify API key and network connectivity**

## 🤝 Contributing

To add new examples or improve existing ones:

1. Follow the existing script structure
2. Include comprehensive error handling
3. Provide clear documentation
4. Test with sample data
5. Update this README

## 📚 Additional Resources

- **AgentFlow Documentation** - Main CLI documentation
- **StepFun API Documentation** - Detailed API reference
- **Model Specifications** - Model capabilities and limits
- **Best Practices Guide** - Optimization tips and tricks

## ⚖️ License and Usage

Please ensure you comply with:
- StepFun API terms of service
- AgentFlow license requirements
- Appropriate usage of AI-generated content
- Privacy and consent for voice cloning

---

🌟 **Happy exploring with StepFun and AgentFlow!** 🌟

For questions or improvements, please refer to the AgentFlow project documentation.