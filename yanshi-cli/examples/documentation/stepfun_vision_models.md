# StepFun Vision Models - CLI Examples

This document provides comprehensive CLI examples for StepFun's vision models, converted from the original Rust examples for image understanding and multimodal processing.

## Prerequisites

```bash
export STEP_API_KEY="your-stepfun-api-key-here"  
yanshi config init
```

## Model Overview

| Model | Context | Capabilities | Best For |
|-------|---------|-------------|----------|
| step-1o-turbo-vision | Vision | General image analysis | Photo descriptions, content analysis |
| step-1v-8k | 8K Vision | Chart/data interpretation | Graphs, diagrams, technical images |
| step-1v-32k | 32K Vision | Detailed visual analysis | Professional photography, art analysis |
| step-3 | Advanced | Multimodal reasoning | Multiple images, complex comparisons |

## Image Preparation

First, let's create some sample images for testing:

```bash
# Download sample images (or use your own)
curl -s "https://picsum.photos/800/600" > landscape.jpg
curl -s "https://picsum.photos/400/300" > chart_sample.png  
curl -s "https://picsum.photos/600/400" > detailed_photo.jpg

# Or create simple test images using ImageMagick (if available)
# convert -size 800x600 xc:skyblue -draw "fill green polygon 0,400 400,200 800,400 800,600 0,600" landscape.jpg
```

## Examples

### 1. step-1o-turbo-vision - General Image Description

**Original Task**: Describe image content including scenery, colors, and composition

```bash
yanshi llm prompt "请详细描述这张图片中的内容，包括景色、颜色、构图等要素。" \
  --model step-1o-turbo-vision \
  --file landscape.jpg \
  --temperature 0.7 \
  --max-tokens 500 \
  --output image_description.md
```

**Expected Output**: Detailed description of the image including visual elements

**Validation Checks**: Response should mention:
- Natural elements (自然)
- Colors (颜色, 绿, 蓝)
- Composition (构图, 景)
- Visual elements (视觉)

### 2. step-1v-8k - Chart and Data Analysis

**Original Task**: Analyze charts and interpret data relationships

```bash
yanshi llm prompt "分析这个图表，解释其中的数据趋势、坐标轴含义，以及可能的统计关系。" \
  --model step-1v-8k \
  --file chart_sample.png \
  --temperature 0.6 \
  --max-tokens 600 \
  --output chart_analysis.md
```

**Expected Output**: Professional analysis of data visualization

**Validation Checks**: Response should discuss:
- Data points (数据, 点)
- Trends (趋势, 关系)
- Axes (轴, 坐标)
- Statistical insights (回归, 线性)

### 3. step-1v-32k - Comprehensive Image Analysis

**Original Task**: Multi-dimensional professional image analysis

```bash
yanshi llm prompt "请从以下几个角度详细分析这张图片：1) 场景和地点特征 2) 光线和色彩运用 3) 人文和社会元素 4) 构图和视觉效果 5) 可能的拍摄技巧" \
  --model step-1v-32k \
  --file detailed_photo.jpg \
  --temperature 0.7 \
  --max-tokens 800 \
  --output comprehensive_analysis.md
```

**Expected Output**: Professional-level multi-aspect image analysis

**Validation Checks**: Response should cover:
- Scene analysis (场景, 地点)
- Lighting (光线, 光)
- Composition (构图, 视觉)
- Technical aspects (技巧, 摄影, 拍摄)
- Cultural elements (人文, 社会, 文化)

### 4. step-3 - Multimodal Comparison

**Original Task**: Compare multiple images and analyze diversity

```bash
# First, create another sample image for comparison
curl -s "https://picsum.photos/500/500" > comparison_image.jpg

yanshi llm prompt "请分析这些图片的内容差异，比较它们的特点，并解释为什么这种多样性在视觉内容中很重要。" \
  --model step-3 \
  --file landscape.jpg \
  --file comparison_image.jpg \
  --temperature 0.8 \
  --max-tokens 700 \
  --output multimodal_comparison.md
```

**Expected Output**: Sophisticated comparison analysis of multiple images

**Validation Checks**: Response should include:
- Comparisons (比较, 对比)
- Diversity (多样, 差异, 不同)  
- Reasoning (因为, 原因, 重要)
- Integration (综合, 整体, 总体)

## Advanced Use Cases

### Batch Image Processing

```bash
# Process multiple images in a directory
for image in images/*.jpg; do
  basename=$(basename "$image" .jpg)
  echo "Processing $image..."
  
  yanshi llm prompt "Analyze this image and describe its main elements:" \
    --model step-1o-turbo-vision \
    --file "$image" \
    --temperature 0.7 \
    --max-tokens 300 \
    --output "analysis_${basename}.md"
done
```

### Professional Photography Analysis

```bash
# Detailed technical analysis for photographers
yanshi llm prompt "作为摄影专家，请分析这张照片的技术参数和艺术价值：
1. 曝光和光线处理
2. 构图原则的应用  
3. 色彩理论体现
4. 情感表达效果
5. 可能的改进建议" \
  --model step-1v-32k \
  --file professional_photo.jpg \
  --temperature 0.6 \
  --max-tokens 1000 \
  --output photography_critique.md
```

### Document and Text Analysis

```bash
# Analyze documents containing text and graphics
yanshi llm prompt "Extract and summarize all text content from this document image, including any charts or diagrams:" \
  --model step-1v-8k \
  --file document_scan.png \
  --temperature 0.3 \
  --max-tokens 800 \
  --output document_extraction.txt
```

### Art and Design Analysis

```bash
# Analyze artistic compositions
yanshi llm prompt "请从艺术史和设计理论角度分析这幅作品：
- 艺术风格和流派特征
- 设计元素运用（线条、形状、色彩）
- 视觉平衡和节奏感
- 文化和历史背景
- 情感和象征意义" \
  --model step-1v-32k \
  --file artwork.jpg \
  --temperature 0.8 \
  --max-tokens 900 \
  --output art_analysis.md
```

## Real-World Applications

### Medical Image Consultation (Educational Only)

```bash
# Note: For educational purposes only, not for actual medical diagnosis
yanshi llm prompt "From an educational perspective, describe the anatomical structures visible in this medical image:" \
  --model step-1v-8k \
  --file medical_scan_example.jpg \
  --temperature 0.3 \
  --max-tokens 500 \
  --output educational_anatomy.md
```

### Architecture and Urban Planning

```bash
yanshi llm prompt "分析这张建筑或城市规划图片：
1. 建筑风格和设计特色
2. 空间布局和功能性
3. 环境整合和可持续性
4. 人文关怀和社会功能
5. 美学价值和创新元素" \
  --model step-1v-32k \
  --file architecture.jpg \
  --temperature 0.7 \
  --max-tokens 800 \
  --output architecture_analysis.md
```

### Product Design Evaluation

```bash
yanshi llm prompt "评估这个产品设计：
- 功能性和实用性
- 美学和视觉吸引力  
- 用户体验和人机交互
- 制造工艺和材料选择
- 市场竞争力和创新点" \
  --model step-1v-32k \
  --file product_design.jpg \
  --temperature 0.6 \
  --max-tokens 700 \
  --output product_evaluation.md
```

## Image Format Support

### Different Image Formats
```bash
# Test various image formats
formats=("jpg" "png" "jpeg" "webp" "bmp")

for format in "${formats[@]}"; do
  if [ -f "test_image.$format" ]; then
    echo "Testing $format format..."
    yanshi llm prompt "Describe this image:" \
      --model step-1o-turbo-vision \
      --file "test_image.$format" \
      --max-tokens 200 \
      --output "description_$format.txt"
  fi
done
```

### Base64 Image Processing (Future)
```bash
# Convert image to base64 for API testing
base64_image=$(base64 -w 0 < image.jpg)
echo "data:image/jpeg;base64,$base64_image" > image_base64.txt

# This would be used in future specialized API integration
```

## Quality Control and Validation

### Automated Quality Checks
```bash
# Create a validation script
cat > validate_analysis.sh << 'EOF'
#!/bin/bash
analysis_file=$1
image_file=$2

echo "Validating analysis for $image_file..."

# Check if analysis contains key visual elements
if grep -qi "color\|颜色\|色彩" "$analysis_file"; then
  echo "✅ Color analysis present"
else
  echo "⚠️ Missing color analysis"
fi

if grep -qi "composition\|构图\|布局" "$analysis_file"; then
  echo "✅ Composition analysis present"
else
  echo "⚠️ Missing composition analysis"
fi

if grep -qi "light\|lighting\|光线\|明暗" "$analysis_file"; then
  echo "✅ Lighting analysis present"
else
  echo "⚠️ Missing lighting analysis"
fi

word_count=$(wc -w < "$analysis_file")
echo "📊 Analysis length: $word_count words"

if [ $word_count -lt 50 ]; then
  echo "⚠️ Analysis might be too brief"
elif [ $word_count -gt 500 ]; then
  echo "ℹ️ Comprehensive analysis detected"
fi
EOF

chmod +x validate_analysis.sh

# Use the validation script
./validate_analysis.sh comprehensive_analysis.md detailed_photo.jpg
```

### Comparative Model Testing
```bash
# Test same image with different models
image="test_photo.jpg"
prompt="详细分析这张图片的视觉元素和美学特征"

models=("step-1o-turbo-vision" "step-1v-8k" "step-1v-32k")

for model in "${models[@]}"; do
  echo "Testing with $model..."
  yanshi llm prompt "$prompt" \
    --model "$model" \
    --file "$image" \
    --temperature 0.7 \
    --max-tokens 500 \
    --output "analysis_${model}.md"
    
  echo "Model: $model" >> comparison_results.txt
  wc -w "analysis_${model}.md" >> comparison_results.txt
  echo "---" >> comparison_results.txt
done
```

## Specialized Analysis Workflows

### Sequential Analysis Pipeline
```bash
# Step 1: Basic description
yanshi llm prompt "简要描述这张图片的主要内容" \
  --model step-1o-turbo-vision \
  --file input_image.jpg \
  --max-tokens 200 \
  --output step1_basic.md

# Step 2: Technical analysis based on basic description
yanshi llm prompt "基于以下基本描述，请提供技术性分析：$(cat step1_basic.md)" \
  --model step-1v-8k \
  --file input_image.jpg \
  --max-tokens 400 \
  --output step2_technical.md

# Step 3: Comprehensive evaluation
yanshi llm prompt "综合以下分析，提供最终评估：
基本描述：$(cat step1_basic.md)
技术分析：$(cat step2_technical.md)" \
  --model step-1v-32k \
  --file input_image.jpg \
  --max-tokens 600 \
  --output step3_comprehensive.md
```

### Cross-Reference Analysis
```bash
# Analyze multiple related images
reference_image="original.jpg"
comparison_images=("variant1.jpg" "variant2.jpg" "variant3.jpg")

# First analyze the reference
yanshi llm prompt "分析这张参考图片的关键特征" \
  --model step-1v-32k \
  --file "$reference_image" \
  --max-tokens 400 \
  --output reference_analysis.md

# Then compare each variant
for img in "${comparison_images[@]}"; do
  basename=$(basename "$img" .jpg)
  yanshi llm prompt "比较这张图片与参考图片的相似性和差异：
  参考分析：$(cat reference_analysis.md)" \
    --model step-3 \
    --file "$reference_image" \
    --file "$img" \
    --max-tokens 500 \
    --output "comparison_${basename}.md"
done
```

## Troubleshooting

### Common Issues

1. **Large image files**: Compress images to reduce processing time
2. **Unsupported formats**: Convert to JPG/PNG for best compatibility
3. **Low quality analysis**: Try step-1v-32k for more detailed results
4. **Multiple images**: Use step-3 model for multi-image analysis

### Debug Commands
```bash
# Check image file properties
file image.jpg
ls -lh image.jpg

# Test with minimal request
yanshi llm prompt "What do you see?" \
  --model step-1o-turbo-vision \
  --file image.jpg \
  --max-tokens 50 \
  --verbose

# Verify image can be read
if file image.jpg | grep -q "image"; then
  echo "✅ Valid image file"
else
  echo "❌ Invalid or corrupted image file"
fi
```

### Performance Optimization
```bash
# For large batch processing, add delays
process_images_batch() {
  local batch_size=5
  local delay=3
  local count=0
  
  for image in images/*.jpg; do
    echo "Processing $image ($(($count + 1)))"
    
    yanshi llm prompt "Analyze this image:" \
      --model step-1o-turbo-vision \
      --file "$image" \
      --max-tokens 300 \
      --output "batch_$(basename "$image" .jpg).md"
    
    ((count++))
    
    if (( count % batch_size == 0 )); then
      echo "Processed $count images, waiting ${delay}s..."
      sleep $delay
    fi
  done
}

process_images_batch
```

---

*These examples showcase StepFun's advanced vision capabilities through Yanshi CLI. Each model specializes in different aspects of visual understanding - from quick descriptions to professional analysis.*