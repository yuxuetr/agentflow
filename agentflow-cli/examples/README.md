# AgentFlow CLI Examples

This directory contains comprehensive examples for using AgentFlow CLI with different model types, particularly focusing on StepFun's specialized capabilities.

## Prerequisites

Before running these examples, ensure you have:

1. **Built the CLI**: 
   ```bash
   cargo build --package agentflow-cli
   ```

2. **Set up API keys** (add to your environment or `.env` file):
   ```bash
   export STEP_API_KEY="your-stepfun-api-key-here"
   export OPENAI_API_KEY="your-openai-api-key-here"  # Optional
   export ANTHROPIC_API_KEY="your-anthropic-api-key"  # Optional
   ```

3. **Initialize AgentFlow configuration**:
   ```bash
   agentflow config init
   ```

## Example Categories

### 📝 Text Generation Models
- [Text Models](#text-models) - Basic text completion with various StepFun models
- [Long Context](#long-context) - Working with extended context windows
- [Streaming](#streaming-text) - Real-time text generation

### 🖼️ Vision Models  
- [Image Understanding](#image-understanding) - Analyze and describe images
- [Chart Analysis](#chart-analysis) - Interpret graphs and diagrams
- [Multimodal](#multimodal-analysis) - Combined text and image processing

### 🎨 Image Generation
- [Text-to-Image](#text-to-image) - Generate images from text descriptions
- [Style Transfer](#style-transfer) - Apply artistic styles to generated images
- [Image Editing](#image-editing) - Modify existing images with text instructions

### 🔊 Audio Processing
- [Text-to-Speech](#text-to-speech) - Convert text to natural speech
- [Speech Recognition](#speech-recognition) - Transcribe audio to text
- [Voice Cloning](#voice-cloning) - Create custom voice models

## Quick Start Examples

### Text Models

#### Basic Text Generation
```bash
# Quick response with step-2-mini (fast model)
agentflow llm prompt "解释为什么天空是蓝色的？用简单易懂的语言回答。" \
  --model step-2-mini \
  --temperature 0.5 \
  --max-tokens 300

# Code generation with step-2-16k
agentflow llm prompt "用Python实现一个快速排序算法，要求包含详细注释和测试用例。" \
  --model step-2-16k \
  --temperature 0.7 \
  --max-tokens 1000 \
  --output quicksort.py
```

#### Long Context Processing
```bash
# Process large documents with step-1-256k
agentflow llm prompt "根据文档总结人工智能、机器学习和深度学习之间的关系" \
  --model step-1-256k \
  --file ai_document.txt \
  --temperature 0.7 \
  --max-tokens 600 \
  --output summary.md
```

#### Streaming Text Generation
```bash
# Real-time text generation with step-1-32k
agentflow llm prompt "详细解释量子计算的基本原理，包括量子比特、叠加态、纠缠现象" \
  --model step-1-32k \
  --stream \
  --temperature 0.8 \
  --max-tokens 800
```

### Image Understanding

#### Basic Image Analysis
```bash
# Analyze image content with step-1o-turbo-vision
agentflow llm prompt "请详细描述这张图片中的内容，包括景色、颜色、构图等要素。" \
  --model step-1o-turbo-vision \
  --file landscape.jpg \
  --temperature 0.7 \
  --max-tokens 500
```

#### Chart and Data Analysis
```bash
# Interpret charts with step-1v-8k
agentflow llm prompt "分析这个图表，解释其中的数据趋势、坐标轴含义，以及可能的统计关系。" \
  --model step-1v-8k \
  --file sales_chart.png \
  --temperature 0.6 \
  --max-tokens 600
```

#### Comprehensive Image Analysis
```bash
# Detailed analysis with step-1v-32k
agentflow llm prompt "请从以下几个角度详细分析这张图片：1)场景和地点特征 2)光线和色彩运用 3)人文和社会元素 4)构图和视觉效果 5)可能的拍摄技巧" \
  --model step-1v-32k \
  --file photo.jpg \
  --temperature 0.7 \
  --max-tokens 800 \
  --output detailed_analysis.md
```

### Multi-Image Comparison
```bash
# Compare multiple images with step-3
agentflow llm prompt "请分析这些图片的内容差异，比较它们的特点，并解释为什么这种多样性在视觉内容中很重要。" \
  --model step-3 \
  --file image1.jpg \
  --file image2.jpg \
  --temperature 0.8 \
  --max-tokens 700
```

## Specialized API Examples

> **Note**: The following examples require workflow execution capabilities, which are implemented in Phase 2. For now, they are provided as templates for future CLI integration.

### Text-to-Image Generation
```bash
# Basic image generation (Future CLI integration)
agentflow generate image \
  --model step-2x-large \
  --prompt "未来科技城市夜景，霓虹灯闪烁，高楼大厦林立，赛博朋克风格，4K超高清" \
  --size 1024x1024 \
  --format b64_json \
  --output cyberpunk_city.png

# Quick generation with step-1x-medium
agentflow generate image \
  --model step-1x-medium \
  --prompt "可爱的小猫咪在花园中玩耍，阳光明媚，色彩鲜艳，卡通风格" \
  --size 768x768 \
  --steps 20 \
  --cfg-scale 7.0 \
  --output cute_cat.png
```

### Text-to-Speech
```bash
# Basic TTS with step-tts-vivid (Future CLI integration)
agentflow generate speech \
  --model step-tts-vivid \
  --text "智能阶跃，十倍每一个人的可能。人工智能助力未来发展。" \
  --voice cixingnansheng \
  --format mp3 \
  --speed 1.0 \
  --output welcome_message.mp3

# Fast synthesis with step-tts-mini
agentflow generate speech \
  --model step-tts-mini \
  --text "你好，欢迎使用AgentFlow CLI工具！" \
  --voice default \
  --format wav \
  --speed 1.2 \
  --output greeting.wav
```

### Speech Recognition
```bash
# Transcribe audio with step-asr (Future CLI integration)
agentflow transcribe audio \
  --model step-asr \
  --file meeting_recording.wav \
  --format json \
  --language zh \
  --output transcript.json

# Simple text transcription
agentflow transcribe audio \
  --model step-asr \
  --file voice_memo.mp3 \
  --format text \
  --output memo_text.txt
```

## Workflow Examples

### Complete Multimodal Pipeline
```yaml
# multimodal_analysis.yml - Comprehensive multimodal workflow
name: "Multimodal Content Analysis"
description: "Analyze images and generate comprehensive reports"

inputs:
  image_path:
    type: "string"
    required: true
  analysis_depth:
    type: "string"
    default: "detailed"

workflow:
  type: "sequential"
  nodes:
    - name: "analyze_image"
      type: "llm"
      config:
        model: "step-1v-32k"
        prompt: |
          请详细分析这张图片：{{ inputs.image_path }}
          
          分析深度：{{ inputs.analysis_depth }}
          
          请从技术和艺术角度分析这张图片。
        file: "{{ inputs.image_path }}"
        temperature: 0.7
        max_tokens: 800
      outputs:
        visual_analysis: "$.response"
    
    - name: "generate_summary"
      type: "template"
      depends_on: ["analyze_image"]
      config:
        template: |
          # 图片分析报告
          
          **图片路径**: {{ inputs.image_path }}
          **分析时间**: {{ now() }}
          **分析深度**: {{ inputs.analysis_depth }}
          
          ## 视觉分析结果
          
          {{ outputs.analyze_image.visual_analysis }}
          
          ---
          *报告由 AgentFlow CLI 生成*

outputs:
  report:
    source: "{{ outputs.generate_summary.rendered }}"
    format: "markdown"
    file: "analysis_report.md"
```

Usage:
```bash
agentflow run multimodal_analysis.yml \
  --input image_path=./photos/landscape.jpg \
  --input analysis_depth=comprehensive
```

### Content Generation Pipeline
```yaml
# content_pipeline.yml - Multi-step content creation
name: "AI Content Generation Pipeline"
description: "Generate text, images, and audio from a single topic"

inputs:
  topic:
    type: "string"
    required: true
  content_style:
    type: "string"
    default: "professional"

workflow:
  type: "sequential"
  nodes:
    - name: "research_content"
      type: "llm"
      config:
        model: "step-2-16k"
        prompt: |
          研究主题：{{ inputs.topic }}
          风格：{{ inputs.content_style }}
          
          请提供：
          1. 核心概念和定义
          2. 最新发展趋势
          3. 实际应用案例
          4. 未来发展方向
        temperature: 0.3
        max_tokens: 1500
    
    - name: "generate_article"
      type: "llm"  
      depends_on: ["research_content"]
      config:
        model: "step-1-32k"
        prompt: |
          基于以下研究内容，写一篇关于"{{ inputs.topic }}"的文章：
          
          {{ outputs.research_content.response }}
          
          要求：
          - 风格：{{ inputs.content_style }}
          - 结构清晰，逻辑性强
          - 包含实例和数据支撑
          - 字数约2000字
        temperature: 0.7
        max_tokens: 2500

outputs:
  research:
    source: "{{ outputs.research_content.response }}"
    format: "markdown"
    file: "{{ inputs.topic }}_research.md"
  
  article:
    source: "{{ outputs.generate_article.response }}"
    format: "markdown" 
    file: "{{ inputs.topic }}_article.md"
```

Usage:
```bash
agentflow run content_pipeline.yml \
  --input topic="区块链技术在金融领域的应用" \
  --input content_style="学术性"
```

## File Format Examples

### Working with Different Input Types

#### Text Files
```bash
# Process code files
agentflow llm prompt "代码审查：分析这段代码的性能和安全性" \
  --model step-2-16k \
  --file app.py \
  --output code_review.md

# Analyze documents
agentflow llm prompt "总结这份技术文档的要点" \
  --model step-1-32k \
  --file technical_spec.md \
  --max-tokens 1000
```

#### Image Files
```bash
# Multiple image formats supported
agentflow llm prompt "比较这些产品图片的设计特点" \
  --model step-3 \
  --file product1.jpg \
  --file product2.png \
  --file product3.webp \
  --temperature 0.8
```

#### Audio Files (Future)
```bash
# Transcribe different audio formats
agentflow transcribe audio --file interview.mp3
agentflow transcribe audio --file lecture.wav  
agentflow transcribe audio --file podcast.m4a
```

## Performance Tips

### Model Selection Guide

| Task Type | Recommended Model | Speed | Quality | Context |
|-----------|------------------|-------|---------|---------|
| Quick Q&A | step-2-mini | ⚡⚡⚡ | ⭐⭐⭐ | 8K |
| Code Generation | step-2-16k | ⚡⚡ | ⭐⭐⭐⭐ | 16K |
| Long Documents | step-1-256k | ⚡ | ⭐⭐⭐⭐ | 256K |
| Streaming Chat | step-1-32k | ⚡⚡ | ⭐⭐⭐⭐ | 32K |
| Image Analysis | step-1o-turbo-vision | ⚡⚡ | ⭐⭐⭐⭐ | Vision |
| Chart Analysis | step-1v-8k | ⚡⚡ | ⭐⭐⭐⭐ | Vision |
| Detailed Vision | step-1v-32k | ⚡ | ⭐⭐⭐⭐⭐ | Vision |
| Multimodal | step-3 | ⚡ | ⭐⭐⭐⭐⭐ | Advanced |

### Optimization Tips

1. **Choose appropriate models**: Use faster models for simple tasks
2. **Set reasonable token limits**: Avoid unnecessary costs
3. **Use streaming**: For long responses that need immediate feedback
4. **Batch similar requests**: Group related tasks in workflows
5. **Cache results**: Save outputs for reuse in complex pipelines

## Troubleshooting

### Common Issues

1. **API Key not found**: Ensure `STEP_API_KEY` is set in environment
2. **Model not available**: Check available models with `agentflow llm models`
3. **File not found**: Use absolute paths or ensure files exist
4. **Token limit exceeded**: Reduce `max-tokens` or split content
5. **Rate limiting**: Add delays between requests or use smaller batches

### Debug Mode
```bash
# Enable verbose logging
agentflow llm prompt "test" --model step-2-mini --verbose --log-level debug
```

## Contributing

To add new examples:

1. Create a new example file in the appropriate category directory
2. Include both CLI commands and workflow YAML examples  
3. Add validation and expected outputs
4. Update this README with the new example
5. Test with actual API calls before submitting

---

*For more information, see the [AgentFlow CLI README](../README.md) and the [main AgentFlow documentation](../../README.md).*