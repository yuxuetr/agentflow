#!/bin/bash

# AgentFlow CLI Quick Start Script
# Demonstrates basic usage of all StepFun model types

set -e

echo "🚀 AgentFlow CLI Quick Start Demo"
echo "================================="
echo

# Check if agentflow binary exists
if ! command -v agentflow &> /dev/null; then
    echo "❌ agentflow CLI not found. Please build it first:"
    echo "   cargo build --package agentflow-cli"
    exit 1
fi

# Check API key
if [ -z "$STEP_API_KEY" ]; then
    echo "❌ STEP_API_KEY environment variable not set"
    echo "   export STEP_API_KEY=\"your-api-key-here\""
    exit 1
fi

# Set the API key with the correct variable name expected by AgentFlow
export STEPFUN_API_KEY="$STEP_API_KEY"

echo "✅ Environment check passed"
echo

# Create output directory
mkdir -p agentflow_demo_output
cd agentflow_demo_output

echo "📁 Created demo output directory: $(pwd)"
echo

# Demo 1: Quick Q&A with step-2-mini
echo "🔥 Demo 1: Quick Q&A with step-2-mini"
echo "-------------------------------------"
agentflow llm prompt "解释为什么天空是蓝色的？用简单易懂的语言回答。" \
  --model step-2-mini \
  --temperature 0.5 \
  --max-tokens 300 \
  --output sky_explanation.txt

echo "✅ Generated explanation saved to: sky_explanation.txt"
echo "   Preview: $(head -n 3 sky_explanation.txt | tr '\n' ' ')..."
echo

# Demo 2: Code generation with step-2-16k
echo "💻 Demo 2: Code Generation with step-2-16k"
echo "--------------------------------------------"
agentflow llm prompt "用Python实现一个二分查找算法，包含详细注释。" \
  --model step-2-16k \
  --temperature 0.7 \
  --max-tokens 800 \
  --output binary_search.py

echo "✅ Generated Python code saved to: binary_search.py"
echo "   Lines of code: $(wc -l < binary_search.py)"
echo

# Demo 3: Detailed explanation with step-1-32k
echo "📚 Demo 3: Detailed Explanation with step-1-32k"  
echo "------------------------------------------------"
agentflow llm prompt "详细解释区块链技术的工作原理，包括共识机制、加密技术和分布式存储。" \
  --model step-1-32k \
  --temperature 0.8 \
  --max-tokens 800 \
  --output blockchain_explanation.md

echo "✅ Generated detailed explanation saved to: blockchain_explanation.md"
echo "   Word count: $(wc -w < blockchain_explanation.md)"
echo

# Demo 4: Model comparison
echo "📊 Demo 4: Model Performance Comparison"
echo "---------------------------------------"
prompt="什么是人工智能？简要解释其主要应用领域。"

models=("step-2-mini" "step-2-16k" "step-1-32k")
for model in "${models[@]}"; do
    echo "Testing $model..."
    start_time=$(date +%s.%N)
    
    agentflow llm prompt "$prompt" \
      --model "$model" \
      --temperature 0.7 \
      --max-tokens 200 \
      --output "ai_explanation_${model}.txt"
    
    end_time=$(date +%s.%N)
    duration=$(echo "$end_time - $start_time" | bc)
    word_count=$(wc -w < "ai_explanation_${model}.txt")
    
    echo "   ⏱️  Duration: ${duration}s | Words: $word_count"
done

echo "✅ Model comparison completed"
echo

# Demo 5: Image analysis (if image available)
echo "🖼️  Demo 5: Image Analysis with Vision Models"
echo "---------------------------------------------"

# Create a simple test image or check for existing ones
test_image=""
for ext in jpg jpeg png; do
    if ls ../*.$ext >/dev/null 2>&1; then
        test_image=$(ls ../*.$ext | head -1)
        break
    fi
done

if [ -n "$test_image" ]; then
    echo "Found test image: $test_image"
    
    # Basic image description
    agentflow llm prompt "请详细描述这张图片中的内容，包括主要对象、颜色和构图特点。" \
      --model step-1o-turbo-vision \
      --file "$test_image" \
      --temperature 0.7 \
      --max-tokens 400 \
      --output image_description.md
    
    echo "✅ Image analysis saved to: image_description.md"
    
    # Technical analysis
    agentflow llm prompt "从摄影技术角度分析这张图片：光线、构图、色彩运用等。" \
      --model step-1v-8k \
      --file "$test_image" \
      --temperature 0.6 \
      --max-tokens 500 \
      --output technical_analysis.md
    
    echo "✅ Technical analysis saved to: technical_analysis.md"
else
    echo "ℹ️  No image files found for vision demo"
    echo "   Place a .jpg/.png file in parent directory to test vision models"
fi
echo

# Demo 6: Batch processing example
echo "⚡ Demo 6: Batch Processing Example"
echo "----------------------------------"

# Create multiple prompts
cat > prompts.txt << 'EOF'
什么是机器学习？
什么是深度学习？
什么是神经网络？
什么是自然语言处理？
什么是计算机视觉？
EOF

echo "Processing 5 AI concept questions..."
counter=1
while IFS= read -r question; do
    echo "  Processing question $counter: $question"
    agentflow llm prompt "$question" \
      --model step-2-mini \
      --temperature 0.6 \
      --max-tokens 150 \
      --output "concept_${counter}.txt" &
    
    ((counter++))
    
    # Limit concurrent requests
    if (( counter % 3 == 0 )); then
        wait  # Wait for current batch to complete
    fi
done < prompts.txt

wait  # Wait for all remaining processes
echo "✅ Batch processing completed - 5 concepts explained"
echo

# Demo 7: Create a summary report
echo "📋 Demo 7: Generate Summary Report"
echo "----------------------------------"

cat > demo_summary.md << EOF
# AgentFlow CLI Demo Summary

**Demo Date**: $(date)
**Models Tested**: step-2-mini, step-2-16k, step-1-32k, step-1o-turbo-vision, step-1v-8k

## Generated Files

### Text Generation
- \`sky_explanation.txt\` - Quick explanation (step-2-mini)
- \`binary_search.py\` - Python code generation (step-2-16k)  
- \`blockchain_explanation.md\` - Detailed explanation (step-1-32k)

### Model Comparison
- \`ai_explanation_step-2-mini.txt\` ($(wc -w < ai_explanation_step-2-mini.txt) words)
- \`ai_explanation_step-2-16k.txt\` ($(wc -w < ai_explanation_step-2-16k.txt) words)
- \`ai_explanation_step-1-32k.txt\` ($(wc -w < ai_explanation_step-1-32k.txt) words)

EOF

if [ -f image_description.md ]; then
cat >> demo_summary.md << EOF

### Vision Analysis
- \`image_description.md\` - Basic image analysis (step-1o-turbo-vision)
- \`technical_analysis.md\` - Technical image analysis (step-1v-8k)

EOF
fi

cat >> demo_summary.md << EOF

### Batch Processing
- \`concept_1.txt\` to \`concept_5.txt\` - AI concept explanations

## Performance Observations

| Model | Speed | Quality | Best For |
|-------|-------|---------|----------|
| step-2-mini | ⚡⚡⚡ | ⭐⭐⭐ | Quick answers, simple tasks |
| step-2-16k | ⚡⚡ | ⭐⭐⭐⭐ | Code generation, analysis |
| step-1-32k | ⚡ | ⭐⭐⭐⭐⭐ | Detailed explanations, complex tasks |
| step-1o-turbo-vision | ⚡⚡ | ⭐⭐⭐⭐ | General image analysis |
| step-1v-8k | ⚡⚡ | ⭐⭐⭐⭐ | Technical image analysis |

## Next Steps

1. Try workflow examples: \`agentflow run workflow.yml\`
2. Explore specialized APIs (image generation, TTS, ASR)
3. Create custom workflows for your use cases
4. Check model availability: \`agentflow llm models --detailed\`

---

*Generated by AgentFlow CLI Quick Start Demo*
EOF

echo "✅ Demo summary report created: demo_summary.md"
echo

# Final summary
echo "🎉 Quick Start Demo Completed!"
echo "=============================="
echo
echo "📊 Summary:"
echo "   • Generated files: $(ls -1 | wc -l)"
echo "   • Total output size: $(du -sh . | cut -f1)"
echo "   • Models tested: 5 different StepFun models"
echo
echo "📖 Next steps:"
echo "   • Review generated files in: $(pwd)"
echo "   • Read demo_summary.md for detailed results"
echo "   • Try advanced examples in ../examples/"
echo "   • Explore workflow capabilities with .yml files"
echo
echo "🔗 Useful commands:"
echo "   agentflow llm models --detailed"
echo "   agentflow config show"
echo "   agentflow --help"
echo
echo "✨ Happy experimenting with AgentFlow CLI!"
