# StepFun Text Models - CLI Examples

This document provides comprehensive CLI examples for all StepFun text generation models, converted from the original Rust examples.

## Prerequisites

```bash
export STEP_API_KEY="your-stepfun-api-key-here"
agentflow config init
```

## Model Overview

| Model | Context | Speed | Best For |
|-------|---------|-------|----------|
| step-2-mini | 8K | ⚡⚡⚡ | Quick Q&A, simple tasks |
| step-2-16k | 16K | ⚡⚡ | Code generation, analysis |
| step-1-32k | 32K | ⚡⚡ | Detailed explanations, streaming |
| step-1-256k | 256K | ⚡ | Long documents, research |

## Examples

### 1. step-2-16k - Code Generation (Non-streaming)

**Original Task**: Python quicksort with detailed comments and test cases

```bash
agentflow llm prompt "用Python实现一个快速排序算法，要求包含详细注释和测试用例。" \
  --model step-2-16k \
  --temperature 0.7 \
  --max-tokens 1000 \
  --output quicksort_implementation.py
```

**Expected Output**: Complete Python quicksort implementation with comments and test cases saved to `quicksort_implementation.py`

**Validation**: Check that output contains:
- Function definition (`def`)
- Chinese comments (`#`)
- Quicksort logic
- Test cases

### 2. step-1-32k - Streaming Explanation

**Original Task**: Detailed quantum computing explanation with streaming

```bash
agentflow llm prompt "详细解释量子计算的基本原理，包括量子比特、叠加态、纠缠现象，以及它与经典计算的主要区别。" \
  --model step-1-32k \
  --stream \
  --temperature 0.8 \
  --max-tokens 800
```

**Expected Output**: Real-time streaming response explaining quantum computing concepts

**Validation**: Response should include:
- Quantum concepts (量子)
- Qubits (量子比特)  
- Superposition (叠加)
- Entanglement (纠缠)

### 3. step-2-mini - Quick Simple Explanation

**Original Task**: Fast explanation of why sky is blue

```bash
agentflow llm prompt "解释为什么天空是蓝色的？用简单易懂的语言回答。" \
  --model step-2-mini \
  --temperature 0.5 \
  --max-tokens 300 \
  --output sky_explanation.txt
```

**Expected Output**: Simple, clear explanation saved to `sky_explanation.txt`

**Validation**: Response should mention:
- Light (光)
- Scattering (散射)  
- Blue color (蓝)

### 4. step-1-256k - Long Context Analysis

**Original Task**: Analyze long document about AI, ML, and Deep Learning

First, create a sample long document:
```bash
cat > ai_context.txt << 'EOF'
人工智能（Artificial Intelligence，简称AI）是计算机科学的一个分支，它试图理解智能的实质，
并生产出一种新的能以人类智能相似的方式做出反应的智能机器。该领域的研究包括机器人、
语言识别、图像识别、自然语言处理和专家系统等。自从人工智能诞生以来，理论和技术日益成熟，
应用领域也不断扩大。可以设想，未来人工智能带来的科技产品，将会是人类智慧的"容器"。

机器学习是人工智能的核心，是使计算机具有智能的根本途径。机器学习是一门多领域交叉学科，
涉及概率论、统计学、逼近论、凸分析、算法复杂度理论等多门学科。专门研究计算机怎样模拟或实现人类的学习行为，
以获取新的知识或技能，重新组织已有的知识结构使之不断改善自身的性能。

深度学习是机器学习的一个分支，它基于人工神经网络。深度学习的概念由Hinton等人于2006年提出。
深度学习通过建立、模拟人脑进行分析学习的神经网络，它模仿人脑的机制来解释数据，例如图像、声音和文本。

$(printf '%s\n%.0s' "$(cat)" {1..3})
EOF
```

Then analyze it:
```bash
agentflow llm prompt "根据上述文档，总结人工智能、机器学习和深度学习之间的关系，并解释它们的发展脉络。" \
  --model step-1-256k \
  --file ai_context.txt \
  --temperature 0.7 \
  --max-tokens 600 \
  --output ai_relationships_analysis.md
```

**Expected Output**: Comprehensive analysis of AI, ML, and DL relationships saved to `ai_relationships_analysis.md`

**Validation**: Response should discuss:
- AI concepts (人工智能)
- Machine Learning (机器学习)
- Deep Learning (深度学习)  
- Relationships (关系)

## Batch Processing Examples

### Process Multiple Questions
```bash
# Create questions file
cat > questions.txt << 'EOF'
什么是区块链技术？
如何实现可持续发展？
人工智能的伦理问题有哪些？
EOF

# Process each question
while IFS= read -r question; do
  echo "Processing: $question"
  agentflow llm prompt "$question" \
    --model step-2-mini \
    --temperature 0.6 \
    --max-tokens 200 \
    --output "answer_$(echo "$question" | md5sum | cut -d' ' -f1).txt"
done < questions.txt
```

### Comparative Analysis
```bash
# Compare model responses for same prompt
prompt="解释机器学习的基本概念"

for model in step-2-mini step-2-16k step-1-32k; do
  echo "Testing model: $model"
  agentflow llm prompt "$prompt" \
    --model "$model" \
    --temperature 0.7 \
    --max-tokens 500 \
    --output "ml_explanation_${model}.md"
done
```

## Advanced Usage

### With System Context (Note: System prompts not yet supported)
```bash
# This will show warning about system prompt support
agentflow llm prompt "写一个Python函数来处理CSV文件" \
  --model step-2-16k \
  --system "你是一个专业的Python程序员，擅长编写高质量、易读的代码。" \
  --temperature 0.7 \
  --max-tokens 800
```

### Chain Multiple Requests
```bash
# Step 1: Research topic
agentflow llm prompt "研究主题：可再生能源技术。提供关键概念、发展趋势和应用案例。" \
  --model step-2-16k \
  --temperature 0.3 \
  --max-tokens 1000 \
  --output renewable_research.md

# Step 2: Create summary based on research
agentflow llm prompt "基于以下研究内容，写一份执行摘要：$(cat renewable_research.md)" \
  --model step-1-32k \
  --temperature 0.5 \
  --max-tokens 600 \
  --output renewable_summary.md
```

### Performance Testing
```bash
# Test response times for different models
for model in step-2-mini step-2-16k step-1-32k; do
  echo "Testing speed for $model..."
  time agentflow llm prompt "什么是人工智能？" \
    --model "$model" \
    --temperature 0.5 \
    --max-tokens 200 \
    > "speed_test_${model}.txt"
done
```

## Error Handling Examples

### Handle Rate Limits
```bash
# Add delay between requests
for i in {1..5}; do
  agentflow llm prompt "Generate example $i for machine learning" \
    --model step-2-mini \
    --temperature 0.7 \
    --max-tokens 300 \
    --output "example_${i}.txt"
  
  sleep 2  # Wait 2 seconds between requests
done
```

### Validate Responses
```bash
# Generate code and validate
agentflow llm prompt "写一个Python排序函数" \
  --model step-2-16k \
  --temperature 0.7 \
  --max-tokens 500 \
  --output generated_code.py

# Check if output is valid Python
if python -m py_compile generated_code.py; then
  echo "✅ Generated valid Python code"
else
  echo "❌ Generated code has syntax errors"
fi
```

## Integration with Other Tools

### With Git Workflow
```bash
# Generate commit message
git diff --cached | agentflow llm prompt "Generate a clear commit message for these changes:" \
  --model step-2-mini \
  --temperature 0.3 \
  --max-tokens 100 \
  --file -  # Read from stdin
```

### With Documentation Generation
```bash
# Generate API documentation
agentflow llm prompt "Create API documentation for this Python module:" \
  --model step-2-16k \
  --file my_module.py \
  --temperature 0.5 \
  --max-tokens 1000 \
  --output api_docs.md
```

## Troubleshooting

### Common Issues

1. **Chinese text encoding**: Ensure terminal supports UTF-8
2. **Long responses**: Increase `--max-tokens` for detailed answers  
3. **Rate limiting**: Add delays or reduce request frequency
4. **Context overflow**: Use step-1-256k for very long inputs

### Debug Commands
```bash
# Check model availability
agentflow llm models --provider stepfun

# Test with minimal request
agentflow llm prompt "Hello" --model step-2-mini --verbose

# Validate configuration
agentflow config show
agentflow config validate
```

---

*These examples demonstrate the power of StepFun's text models through AgentFlow CLI. Each model is optimized for different use cases - choose based on your speed vs. quality requirements.*