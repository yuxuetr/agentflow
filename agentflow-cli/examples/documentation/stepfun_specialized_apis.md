# StepFun Specialized APIs - CLI Examples

This document provides CLI examples for StepFun's specialized APIs including image generation, text-to-speech, speech recognition, and voice cloning. These features will be available through workflow integration in Phase 2.

## Prerequisites

```bash
export STEP_API_KEY="your-stepfun-api-key-here"
agentflow config init
```

## Current Status

⚠️ **Note**: The specialized API features shown below require workflow execution capabilities (Phase 2). Currently, only text and vision models work directly with `agentflow llm prompt`. The examples below show the intended CLI interface for future implementation.

## Image Generation Models

### Model Overview

| Model | Quality | Speed | Best For |
|-------|---------|-------|----------|
| step-2x-large | ⭐⭐⭐⭐⭐ | ⚡ | High-quality artistic images |
| step-1x-medium | ⭐⭐⭐⭐ | ⚡⚡ | Balanced quality/speed |
| step-1x-edit | ⭐⭐⭐⭐ | ⚡⚡ | Image editing and modifications |

### Text-to-Image Generation

#### Basic Image Generation (Future Implementation)
```bash
# High-quality generation with step-2x-large
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
  --output cute_cat_garden.png
```

#### Advanced Generation with Style Reference
```bash
# Generate with style reference
agentflow generate image \
  --model step-1x-medium \
  --prompt "A majestic mountain landscape at sunset" \
  --size 1280x800 \
  --style-reference reference_style.jpg \
  --style-strength 0.8 \
  --seed 42 \
  --steps 50 \
  --cfg-scale 7.5 \
  --output styled_landscape.png
```

#### Batch Image Generation
```bash
# Generate multiple variations
prompts=("春天的樱花" "夏日海滩" "秋天枫叶" "冬日雪景")

for i in "${!prompts[@]}"; do
  agentflow generate image \
    --model step-1x-medium \
    --prompt "${prompts[$i]}" \
    --size 512x512 \
    --seed $((42 + i)) \
    --output "season_${i}_$(echo "${prompts[$i]}" | tr ' ' '_').png"
done
```

### Image-to-Image Transformation

```bash
# Transform existing image
agentflow generate image \
  --model step-1x-medium \
  --input-image source_photo.jpg \
  --prompt "Transform into impressionist painting style" \
  --strength 0.7 \
  --steps 30 \
  --output impressionist_transform.png

# Style transfer
agentflow generate image \
  --model step-1x-edit \
  --input-image portrait.jpg \
  --prompt "Convert to anime style illustration" \
  --strength 0.8 \
  --cfg-scale 8.0 \
  --output anime_portrait.png
```

### Image Editing

```bash
# Edit specific parts of image
agentflow edit image \
  --model step-1x-edit \
  --input-image room_photo.jpg \
  --mask room_mask.png \
  --prompt "Replace the furniture with modern minimalist style" \
  --steps 25 \
  --output modern_room.png

# Inpainting (fill missing parts)
agentflow edit image \
  --model step-1x-edit \
  --input-image damaged_photo.jpg \
  --mask damage_mask.png \
  --prompt "Restore the missing parts naturally" \
  --output restored_photo.png
```

## Text-to-Speech Models

### Model Overview

| Model | Quality | Speed | Features |
|-------|---------|-------|----------|
| step-tts-vivid | ⭐⭐⭐⭐⭐ | ⚡⚡ | Emotional, multilingual |
| step-tts-mini | ⭐⭐⭐ | ⚡⚡⚡ | Fast, basic synthesis |

### Basic TTS

```bash
# High-quality synthesis with step-tts-vivid
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
  --text "你好，欢迎使用AgentFlow CLI工具！这是一个快速语音合成示例。" \
  --voice default \
  --format wav \
  --speed 1.2 \
  --output greeting_fast.wav
```

### Emotional TTS

```bash
# Generate with different emotions
emotions=("高兴" "悲伤" "愤怒" "惊讶" "平静")
text="今天的天气真的很不错，适合出门散步。"

for emotion in "${emotions[@]}"; do
  agentflow generate speech \
    --model step-tts-vivid \
    --text "$text" \
    --voice cixingnansheng \
    --emotion "$emotion" \
    --format mp3 \
    --speed 1.0 \
    --output "emotional_${emotion}.mp3"
done
```

### Multilingual TTS

```bash
# Chinese
agentflow generate speech \
  --model step-tts-vivid \
  --text "欢迎使用多语言语音合成功能" \
  --voice chinese_voice \
  --language zh \
  --output chinese_welcome.mp3

# English  
agentflow generate speech \
  --model step-tts-vivid \
  --text "Welcome to multilingual text-to-speech synthesis" \
  --voice english_voice \
  --language en \
  --output english_welcome.mp3

# Japanese
agentflow generate speech \
  --model step-tts-vivid \
  --text "多言語音声合成へようこそ" \
  --voice japanese_voice \
  --language ja \
  --output japanese_welcome.mp3
```

### Advanced TTS Parameters

```bash
# Fine-tuned synthesis
agentflow generate speech \
  --model step-tts-vivid \
  --text "这是一个高质量的语音合成示例，展示了各种参数的调节效果。" \
  --voice professional_male \
  --speed 0.9 \
  --pitch 1.1 \
  --volume 0.8 \
  --pause-after-punctuation 500ms \
  --format flac \
  --sample-rate 48000 \
  --output high_quality_speech.flac
```

## Speech Recognition (ASR)

### Basic Transcription

```bash
# Transcribe audio file
agentflow transcribe audio \
  --model step-asr \
  --file meeting_recording.wav \
  --format json \
  --language zh \
  --output transcript.json

# Simple text transcription
agentflow transcribe audio \
  --model step-asr \
  --file interview.mp3 \
  --format text \
  --output interview_transcript.txt

# Subtitle generation
agentflow transcribe audio \
  --model step-asr \
  --file lecture.wav \
  --format srt \
  --output lecture_subtitles.srt
```

### Advanced ASR Features

```bash
# Transcription with speaker identification
agentflow transcribe audio \
  --model step-asr \
  --file multi_speaker.wav \
  --format json \
  --identify-speakers \
  --min-speakers 2 \
  --max-speakers 5 \
  --output speaker_transcript.json

# Real-time transcription (future)
agentflow transcribe realtime \
  --model step-asr \
  --input-device microphone \
  --language auto-detect \
  --output-stream stdout
```

### Batch Transcription

```bash
# Process multiple audio files
audio_files=(*.wav *.mp3 *.m4a)

for audio in "${audio_files[@]}"; do
  if [ -f "$audio" ]; then
    basename=$(basename "$audio" | cut -d. -f1)
    echo "Transcribing $audio..."
    
    agentflow transcribe audio \
      --model step-asr \
      --file "$audio" \
      --format text \
      --output "transcript_${basename}.txt"
  fi
done
```

## Voice Cloning

### Create Custom Voice

```bash
# Upload training audio for voice cloning
agentflow voice create \
  --name "my_custom_voice" \
  --training-audio voice_samples.wav \
  --description "Personal voice clone for TTS" \
  --language zh \
  --output voice_id.txt

# List available voices
agentflow voice list \
  --limit 20 \
  --output voices.json

# Get voice details
agentflow voice info \
  --voice-id "voice_12345" \
  --output voice_details.json
```

### Use Custom Voice

```bash
# Generate speech with custom voice
custom_voice_id=$(cat voice_id.txt)

agentflow generate speech \
  --model step-tts-vivid \
  --text "这是使用我的自定义语音生成的音频" \
  --voice "$custom_voice_id" \
  --format mp3 \
  --output custom_voice_sample.mp3
```

### Voice Management

```bash
# Delete custom voice
agentflow voice delete \
  --voice-id "voice_12345" \
  --confirm

# Update voice description
agentflow voice update \
  --voice-id "voice_12345" \
  --name "Updated Voice Name" \
  --description "Updated description"
```

## Workflow Integration Examples

### Complete Content Creation Pipeline

```yaml
# content_pipeline.yml - Multi-modal content creation
name: "Multi-modal Content Pipeline"
description: "Generate text, image, and audio content from a single topic"

inputs:
  topic:
    type: "string" 
    required: true
    description: "Content topic"
  style:
    type: "string"
    default: "professional"
    description: "Content style"

workflow:
  type: "sequential"
  nodes:
    # Step 1: Generate text content
    - name: "generate_text"
      type: "llm"
      config:
        model: "step-2-16k"
        prompt: |
          为主题"{{ inputs.topic }}"创作内容，风格：{{ inputs.style }}
          要求：结构清晰，内容丰富，约500字
        temperature: 0.7
        max_tokens: 800
      outputs:
        content: "$.response"
    
    # Step 2: Generate accompanying image
    - name: "generate_image"
      type: "image_generation"
      depends_on: ["generate_text"]
      config:
        model: "step-1x-medium"
        prompt: "Professional illustration for: {{ inputs.topic }}, {{ inputs.style }} style"
        size: "1024x768"
        format: "png"
      outputs:
        image_path: "$.image_path"
    
    # Step 3: Generate audio narration
    - name: "generate_audio"
      type: "text_to_speech"
      depends_on: ["generate_text"]
      config:
        model: "step-tts-vivid"
        text: "{{ outputs.generate_text.content }}"
        voice: "professional_narrator"
        format: "mp3"
        speed: 1.0
      outputs:
        audio_path: "$.audio_path"
    
    # Step 4: Create final package
    - name: "package_content"
      type: "template"
      depends_on: ["generate_text", "generate_image", "generate_audio"]
      config:
        template: |
          # {{ inputs.topic }}
          
          **风格**: {{ inputs.style }}
          **生成时间**: {{ now() }}
          
          ## 文本内容
          {{ outputs.generate_text.content }}
          
          ## 多媒体文件
          - 配图: {{ outputs.generate_image.image_path }}
          - 音频: {{ outputs.generate_audio.audio_path }}
          
          ---
          *由 AgentFlow 自动生成*

outputs:
  content_package:
    source: "{{ outputs.package_content.rendered }}"
    format: "markdown"
    file: "{{ inputs.topic | slugify }}_package.md"
  
  multimedia_files:
    image: "{{ outputs.generate_image.image_path }}"
    audio: "{{ outputs.generate_audio.audio_path }}"
```

Usage:
```bash
agentflow run content_pipeline.yml \
  --input topic="人工智能的未来发展" \
  --input style="科普"
```

### Podcast Creation Workflow

```yaml
# podcast_creation.yml
name: "Automated Podcast Creation"
description: "Generate podcast content with speech and background music"

inputs:
  episode_topic:
    type: "string"
    required: true
  host_voice:
    type: "string"
    default: "professional_host"
  episode_length:
    type: "string"
    default: "10 minutes"

workflow:
  type: "sequential"
  nodes:
    - name: "write_script"
      type: "llm"
      config:
        model: "step-1-32k"
        prompt: |
          为播客节目写一个关于"{{ inputs.episode_topic }}"的脚本
          要求：
          - 时长约{{ inputs.episode_length }}
          - 包含开场白和结束语
          - 内容有趣且有教育意义
          - 适合音频播放的语言风格
        temperature: 0.8
        max_tokens: 2000
      outputs:
        script: "$.response"
    
    - name: "generate_speech"
      type: "text_to_speech"
      depends_on: ["write_script"]
      config:
        model: "step-tts-vivid"
        text: "{{ outputs.write_script.script }}"
        voice: "{{ inputs.host_voice }}"
        format: "wav"
        speed: 0.95
        emotion: "友好"
      outputs:
        audio_file: "$.audio_path"
    
    - name: "create_show_notes"
      type: "llm"
      depends_on: ["write_script"]
      config:
        model: "step-2-16k"
        prompt: |
          基于以下播客脚本，创建节目备注：
          {{ outputs.write_script.script }}
          
          格式要求：
          - 节目简介
          - 主要讨论点
          - 相关链接和资源
          - 时间戳标记
        temperature: 0.5
        max_tokens: 800
      outputs:
        show_notes: "$.response"

outputs:
  podcast_package:
    script: "{{ outputs.write_script.script }}"
    audio: "{{ outputs.generate_speech.audio_file }}"
    notes: "{{ outputs.create_show_notes.show_notes }}"
```

## Real-World Application Examples

### Educational Content Creation

```bash
# Generate educational materials
agentflow run educational_content.yml \
  --input subject="量子物理基础" \
  --input grade_level="大学" \
  --input include_audio=true \
  --input include_diagrams=true
```

### Marketing Content Pipeline

```bash
# Create marketing materials
agentflow run marketing_pipeline.yml \
  --input product="智能手表" \
  --input target_audience="年轻专业人士" \
  --input campaign_style="科技时尚"
```

### Accessibility Content

```bash
# Generate accessible content versions
agentflow run accessibility_converter.yml \
  --input source_document="technical_manual.pdf" \
  --input output_formats="audio,large_print,simplified"
```

## Error Handling and Validation

### Validate Generated Content

```bash
# Check image generation results
validate_image() {
  local image_file=$1
  if [ -f "$image_file" ]; then
    local file_size=$(stat -f%z "$image_file" 2>/dev/null || stat -c%s "$image_file")
    if [ $file_size -gt 1000 ]; then
      echo "✅ Image generated successfully: $image_file ($file_size bytes)"
    else
      echo "⚠️ Image file seems too small: $image_file"
    fi
  else
    echo "❌ Image generation failed: $image_file not found"
  fi
}

# Check audio generation results  
validate_audio() {
  local audio_file=$1
  if [ -f "$audio_file" ]; then
    local duration=$(ffprobe -v quiet -show_entries format=duration -of csv=p=0 "$audio_file" 2>/dev/null)
    if [ -n "$duration" ] && (( $(echo "$duration > 0" | bc -l) )); then
      echo "✅ Audio generated successfully: $audio_file (${duration}s)"
    else
      echo "⚠️ Audio file may be corrupted: $audio_file"
    fi
  else
    echo "❌ Audio generation failed: $audio_file not found"
  fi
}
```

### Batch Processing with Error Recovery

```bash
# Robust batch processing
process_content_batch() {
  local topics_file=$1
  local max_retries=3
  
  while IFS= read -r topic; do
    local retry_count=0
    local success=false
    
    while [ $retry_count -lt $max_retries ] && [ "$success" = false ]; do
      echo "Processing: $topic (attempt $((retry_count + 1)))"
      
      if agentflow run content_pipeline.yml --input topic="$topic"; then
        echo "✅ Successfully processed: $topic"
        success=true
      else
        echo "⚠️ Failed to process: $topic (attempt $((retry_count + 1)))"
        ((retry_count++))
        sleep 5  # Wait before retry
      fi
    done
    
    if [ "$success" = false ]; then
      echo "❌ Failed to process after $max_retries attempts: $topic" >> failed_topics.txt
    fi
    
  done < "$topics_file"
}
```

---

*These examples demonstrate the future capabilities of StepFun's specialized APIs through AgentFlow CLI. Full implementation will be available in Phase 2 with workflow execution support.*