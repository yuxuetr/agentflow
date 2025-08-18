# AgentFlow Workflow Examples

This directory contains comprehensive workflow examples migrated and adapted from PocketFlow cookbook, now using OpenAI models instead of StepFun models.

## Overview

These workflow examples demonstrate various AgentFlow patterns and use cases, from simple question-answering to complex multi-agent systems. All examples have been converted from Python-based PocketFlow implementations to YAML-based AgentFlow workflows.

## Quick Start

To run any workflow example:

```bash
# Basic usage
agentflow run hello_world.yml --input '{"question": "What is machine learning?"}'

# With custom parameters
agentflow run chat_bot.yml --input '{"user_message": "Hello!", "model": "gpt-4o"}'

# With environment variable
OPENAI_API_KEY=your_key_here agentflow run structured_output.yml --input @resume_data.json
```

## Workflow Categories

### 🟢 Basic Examples (Beginner)

#### 1. Hello World (`hello_world.yml`)
- **Difficulty**: ⭐☆☆
- **Description**: Simple question-answering workflow
- **Use Case**: Learning AgentFlow basics, single LLM call
- **Features**: Basic input/output, OpenAI integration
- **Example**:
  ```bash
  agentflow run hello_world.yml --input '{"question": "What is the meaning of life?"}'
  ```

#### 2. Chat Bot (`chat_bot.yml`)
- **Difficulty**: ⭐☆☆
- **Description**: Conversational AI with message history
- **Use Case**: Interactive chatbots, conversation management
- **Features**: Message history, conversation context
- **Example**:
  ```bash
  agentflow run chat_bot.yml --input '{"user_message": "Tell me about quantum physics"}'
  ```

#### 3. Structured Output (`structured_output.yml`)
- **Difficulty**: ⭐⭐☆
- **Description**: Extract structured data from text (resume parsing)
- **Use Case**: Data extraction, document processing
- **Features**: YAML output parsing, validation, skill matching
- **Example**:
  ```bash
  agentflow run structured_output.yml --input '{"resume_text": "John Smith\\nSoftware Engineer\\n5 years Python..."}'
  ```

### 🟡 Intermediate Examples

#### 4. Batch Translation (`batch_translation.yml`)
- **Difficulty**: ⭐⭐☆
- **Description**: Translate documents into multiple languages
- **Use Case**: Internationalization, content localization
- **Features**: Parallel processing, batch operations, file output
- **Example**:
  ```bash
  agentflow run batch_translation.yml --input '{"source_text": "# Welcome\\nThis is a test", "target_languages": ["Spanish", "French"]}'
  ```

#### 5. Content Creation (`content_creation.yml`)
- **Difficulty**: ⭐⭐☆
- **Description**: AI-powered article writing with outline, content, and styling
- **Use Case**: Content marketing, blog writing, documentation
- **Features**: Multi-step workflow, style application, structured content
- **Example**:
  ```bash
  agentflow run content_creation.yml --input '{"topic": "Future of AI", "writing_style": "conversational"}'
  ```

### 🔴 Advanced Examples

#### 6. Map-Reduce Resume (`map_reduce_resume.yml`)
- **Difficulty**: ⭐⭐⭐
- **Description**: Process multiple resumes in parallel for qualification assessment
- **Use Case**: HR automation, candidate screening, batch document processing
- **Features**: Map-reduce pattern, parallel evaluation, aggregated results
- **Example**:
  ```bash
  agentflow run map_reduce_resume.yml --input '{"resume_files": {"resume1.txt": "John Smith...", "resume2.txt": "Jane Doe..."}}'
  ```

#### 7. RAG System (`rag_system.yml`)
- **Difficulty**: ⭐⭐⭐
- **Description**: Complete Retrieval-Augmented Generation system
- **Use Case**: Knowledge bases, document Q&A, research assistance
- **Features**: Document chunking, embeddings, similarity search, context-aware responses
- **Example**:
  ```bash
  agentflow run rag_system.yml --input '{"documents": ["AI is transforming...", "Machine learning enables..."], "query": "What is AI?"}'
  ```

#### 8. Multi-Agent Game (`multi_agent_game.yml`)
- **Difficulty**: ⭐⭐⭐
- **Description**: Taboo word game between two AI agents
- **Use Case**: Multi-agent systems, game AI, collaborative problem solving
- **Features**: Agent collaboration, iterative workflows, game logic
- **Example**:
  ```bash
  agentflow run multi_agent_game.yml --input '{"target_word": "nostalgic", "forbidden_words": ["memory", "past", "feeling"]}'
  ```

### 🟣 Performance Examples (Migrated from StepFun)

#### 9. OpenAI Simple Text (`stepfun_simple_text.yml`)
- **Difficulty**: ⭐☆☆
- **Description**: Basic text generation (migrated from StepFun)
- **Use Case**: Simple text generation, model comparison
- **Features**: Model selection, temperature control
- **Migration Note**: Originally used StepFun models, now uses OpenAI
- **Example**:
  ```bash
  agentflow run stepfun_simple_text.yml --input '{"prompt": "Write about AI", "model": "gpt-4o"}'
  ```

#### 10. OpenAI Multimodal Chain (`stepfun_multimodal_chain.yml`)
- **Difficulty**: ⭐⭐⭐
- **Description**: Sequential processing with different models (migrated from StepFun)
- **Use Case**: Complex analysis workflows, model chaining
- **Features**: Sequential model usage, comprehensive reporting
- **Migration Note**: Model chain adapted from StepFun to OpenAI models
- **Example**:
  ```bash
  agentflow run stepfun_multimodal_chain.yml --input '{"topic": "AI impact", "analysis_depth": "detailed"}'
  ```

#### 11. OpenAI Parallel Processing (`stepfun_parallel_processing.yml`)
- **Difficulty**: ⭐⭐⭐
- **Description**: Parallel analysis from multiple perspectives (migrated from StepFun)
- **Use Case**: Multi-perspective analysis, parallel processing
- **Features**: Parallel execution, perspective-based analysis
- **Migration Note**: Parallel pattern preserved with OpenAI models
- **Example**:
  ```bash
  agentflow run stepfun_parallel_processing.yml --input '{"base_topic": "Renewable energy"}'
  ```

## Migration Notes

### From PocketFlow to AgentFlow

1. **Architecture Change**: Python-based nodes → YAML-based workflows
2. **Model Provider**: StepFun models → OpenAI models
3. **Execution**: Python runtime → AgentFlow CLI
4. **Configuration**: Code-based → Declarative YAML

### Model Mapping

| Original (StepFun) | Migrated (OpenAI) | Use Case |
|-------------------|------------------|-----------|
| `step-1-8k` | `gpt-3.5-turbo` | Quick tasks, content generation |
| `step-1-32k` | `gpt-4o` | Complex analysis, long context |
| `step-2-16k` | `gpt-4` | Critical analysis, reasoning |
| `step-2-mini` | `gpt-4o-mini` | Simple tasks, cost optimization |

## Environment Setup

### Prerequisites

1. **AgentFlow CLI**: Install the latest version
2. **OpenAI API Key**: Set `OPENAI_API_KEY` environment variable
3. **Dependencies**: All examples use OpenAI models (no additional dependencies)

### Environment Variables

```bash
# Required for all examples
export OPENAI_API_KEY="your-openai-api-key-here"

# Optional: Configure default models
export AGENTFLOW_DEFAULT_MODEL="gpt-4o"
export AGENTFLOW_LOG_LEVEL="info"
```

## Common Patterns

### Input/Output Management

```yaml
inputs:
  user_input:
    type: "string"
    required: true
    description: "User input description"
    example: "Example value"

outputs:
  result:
    source: "{{ step_name.output }}"
    format: "text|json|yaml|markdown"
    file: "output/result.txt"
```

### Model Configuration

```yaml
config:
  model: "{{ inputs.model | default('gpt-4o') }}"
  temperature: "{{ inputs.temperature | default(0.7) }}"
  max_tokens: 1000
  timeout: "3m"
```

### Batch Processing

```yaml
type: "batch"
batch_input: "{{ inputs.items }}"
config:
  # Process each item in batch_input
  prompt: "Process: {{ batch_item }}"
```

### Template Usage

```yaml
type: "template"
config:
  template: |
    {% for item in data %}
    Process {{ item }}
    {% endfor %}
```

## Troubleshooting

### Common Issues

1. **API Key Missing**: Ensure `OPENAI_API_KEY` is set
2. **Model Not Found**: Check OpenAI model availability
3. **Timeout Errors**: Increase timeout in config
4. **Template Syntax**: Verify Jinja2 template syntax

### Performance Tips

1. **Use appropriate models**: gpt-4o for complex tasks, gpt-3.5-turbo for simple ones
2. **Optimize batch sizes**: Balance between parallelism and rate limits
3. **Set reasonable timeouts**: Prevent hanging workflows
4. **Monitor token usage**: Track costs and usage patterns

## Contributing

To add new examples:

1. Follow the naming convention: `category_name.yml`
2. Include comprehensive documentation
3. Add input/output examples
4. Test with different model configurations
5. Update this README with the new example

## Support

- **AgentFlow Documentation**: [Link to docs]
- **Issue Tracker**: [Link to issues]
- **Community Forum**: [Link to discussions]

## License

These examples are provided under the same license as AgentFlow.

---

*Examples migrated from PocketFlow cookbook to AgentFlow workflows using OpenAI models*