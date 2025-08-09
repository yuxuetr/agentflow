# AgentFlow CLI Design Document

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Command Structure](#command-structure)
4. [Workflow Configuration Format](#workflow-configuration-format)
5. [Node Types](#node-types)
6. [Integration with AgentFlow Core](#integration-with-agentflow-core)
7. [File Input/Output Support](#file-inputoutput-support)
8. [Implementation Plan](#implementation-plan)
9. [Examples](#examples)
10. [Future Extensions](#future-extensions)

## Overview

### Purpose

The AgentFlow CLI (`agentflow-cli`) provides a command-line interface that unifies workflow execution and LLM interaction capabilities, making AgentFlow accessible to users who prefer command-line tools or need to integrate AgentFlow into scripts and CI/CD pipelines.

### Goals

- **Unified Interface**: Single `agentflow` binary for all operations
- **Workflow Execution**: YAML-based workflow configuration with support for sequential, parallel, conditional flows, and loops
- **Direct LLM Access**: Simple commands for single LLM interactions
- **File I/O Support**: Handle text, image, and audio files seamlessly
- **Developer Experience**: Rich error messages, progress indicators, and validation
- **Integration Ready**: Designed for scripts, automation, and CI/CD workflows

### Key Features

- YAML-based workflow configuration
- Built-in node types (LLM, batch processing, templates, file I/O)
- Multimodal input support (text, images, audio)
- Streaming output capabilities
- Interactive chat mode
- Configuration validation and model discovery
- Comprehensive error handling and logging

## Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        AgentFlow CLI                            │
├─────────────────────────────────────────────────────────────────┤
│  Command Layer                                                  │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │   Workflow      │  │      LLM        │  │     Config      │ │
│  │   Commands      │  │   Commands      │  │   Commands      │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  Configuration Layer                                            │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │   Workflow      │  │   Template      │  │    Schema       │ │
│  │    Parser       │  │    Engine       │  │  Validation     │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  Execution Layer                                                │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │   Workflow      │  │     Node        │  │    Context      │ │
│  │    Runner       │  │   Factory       │  │   Management    │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│  Core Integration                                               │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │  AgentFlow      │  │  AgentFlow      │  │   File I/O      │ │
│  │     Core        │  │     LLM         │  │   Utilities     │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Component Structure

```
agentflow-cli/
├── src/
│   ├── main.rs              # CLI entry point and argument parsing
│   ├── commands/            # Command implementations
│   │   ├── mod.rs
│   │   ├── workflow/        # Workflow execution commands
│   │   │   ├── mod.rs
│   │   │   ├── run.rs       # agentflow run
│   │   │   ├── validate.rs  # agentflow validate
│   │   │   └── list.rs      # agentflow list
│   │   ├── llm/            # LLM interaction commands
│   │   │   ├── mod.rs
│   │   │   ├── prompt.rs    # agentflow llm prompt
│   │   │   ├── chat.rs      # agentflow llm chat
│   │   │   └── models.rs    # agentflow llm models
│   │   └── config/         # Configuration commands
│   │       ├── mod.rs
│   │       ├── init.rs      # agentflow config init
│   │       └── show.rs      # agentflow config show
│   ├── config/             # Configuration parsing and validation
│   │   ├── mod.rs
│   │   ├── workflow.rs      # Workflow configuration schema
│   │   ├── parser.rs        # YAML/JSON parsers
│   │   └── validation.rs    # Schema validation
│   ├── executor/           # Workflow execution engine
│   │   ├── mod.rs
│   │   ├── runner.rs        # Main workflow execution logic
│   │   ├── context.rs       # Execution context management
│   │   └── nodes/          # Built-in node implementations
│   │       ├── mod.rs
│   │       ├── llm.rs       # LLM node type
│   │       ├── batch.rs     # Batch processing node
│   │       ├── template.rs  # Template rendering node
│   │       ├── file.rs      # File I/O node
│   │       └── http.rs      # HTTP request node
│   └── utils/              # Shared utilities
│       ├── mod.rs
│       ├── file.rs          # File handling utilities
│       ├── output.rs        # Output formatting
│       └── progress.rs      # Progress indicators
├── templates/              # Workflow templates
│   ├── simple.yml          # Basic sequential workflow
│   ├── llm-chain.yml       # LLM processing chain
│   ├── parallel.yml        # Parallel processing example
│   └── conditional.yml     # Conditional branching example
├── tests/                  # Integration tests
│   ├── workflows/          # Test workflow configurations
│   └── integration/        # End-to-end tests
└── Cargo.toml
```

## Command Structure

### Primary Commands

#### Workflow Commands
```bash
agentflow run <workflow_file>           # Execute workflow from file
agentflow run <workflow_file> [OPTIONS] # Execute with options
agentflow validate <workflow_file>      # Validate workflow configuration
agentflow list workflows                # List available workflow templates
```

#### LLM Commands
```bash
agentflow llm prompt <text> [OPTIONS]   # Send prompt to LLM
agentflow llm chat [OPTIONS]            # Interactive chat session
agentflow llm models [OPTIONS]          # List available models
```

#### Configuration Commands
```bash
agentflow config init                   # Initialize configuration files
agentflow config show                   # Display current configuration
agentflow config validate               # Validate configuration
```

### Command Options

#### Workflow Run Options
```bash
--watch, -w              # Watch for file changes and rerun
--output, -o <file>      # Save execution results to file
--input, -i <key=value>  # Set input parameters
--verbose, -v            # Verbose output
--dry-run               # Validate without executing
--timeout <duration>    # Set execution timeout
--max-retries <count>   # Set maximum retry attempts
```

#### LLM Prompt Options
```bash
--model, -m <model>     # Specify model name
--temperature <float>   # Set temperature (0.0-1.0)
--max-tokens <int>      # Maximum tokens to generate
--file, -f <path>       # Input file (text, image, audio)
--output, -o <path>     # Output file
--stream                # Enable streaming output
--system <text>         # System prompt
```

#### LLM Chat Options
```bash
--model, -m <model>     # Specify model name
--system <text>         # System prompt
--save <file>           # Save conversation to file
--load <file>           # Load conversation from file
```

## Workflow Configuration Format

### Schema Definition

```yaml
# workflow.yml - Complete schema example
name: "Article Generation Pipeline"
version: "1.0.0"
description: "Generate comprehensive articles from topics using multi-stage LLM processing"
author: "AgentFlow User"

# Metadata
metadata:
  created: "2024-01-01T00:00:00Z"
  tags: ["content-generation", "llm-chain", "article-writing"]
  category: "content"

# Global configuration
config:
  timeout: "5m"           # Execution timeout
  max_retries: 3          # Maximum retry attempts
  output_format: "json"   # Output format (json, yaml, text)
  log_level: "info"       # Logging level
  
# Input parameter definitions
inputs:
  topic:
    type: "string"
    required: true
    description: "Main article topic"
    example: "Artificial Intelligence Ethics"
  
  style:
    type: "string"
    required: false
    default: "conversational"
    enum: ["formal", "conversational", "technical", "academic"]
    description: "Writing style for the article"
  
  max_sections:
    type: "integer"
    required: false
    default: 5
    min: 3
    max: 10
    description: "Maximum number of article sections"
  
  target_audience:
    type: "string"
    required: false
    default: "general"
    enum: ["general", "technical", "academic", "beginner"]

# Environment variables
environment:
  OPENAI_API_KEY: "required"
  ANTHROPIC_API_KEY: "optional"
  AGENTFLOW_LOG_LEVEL: "optional"

# Workflow definition
workflow:
  type: "sequential"  # sequential | parallel | conditional | loop
  
  # Sequential workflow nodes
  nodes:
    # Stage 1: Research and outline generation
    - name: "research_topic"
      type: "llm"
      description: "Research the topic and gather key information"
      config:
        model: "gpt-4o"
        prompt: |
          Research the topic: {{ inputs.topic }}
          
          Provide:
          1. Key concepts and definitions
          2. Current trends and developments  
          3. Important subtopics to cover
          4. Target audience considerations for {{ inputs.target_audience }} level
          
          Format as structured JSON with sections: concepts, trends, subtopics, audience_notes
        temperature: 0.7
        max_tokens: 1000
        timeout: "2m"
      outputs:
        research_data: "$.response"
        concepts: "$.parsed.concepts"
        trends: "$.parsed.trends"
        subtopics: "$.parsed.subtopics"
      error_handling:
        retry_on: ["timeout", "rate_limit"]
        fallback_model: "gpt-3.5-turbo"
    
    - name: "generate_outline"
      type: "llm"
      depends_on: ["research_topic"]
      description: "Create detailed article outline"
      config:
        model: "claude-3-sonnet"
        prompt: |
          Based on this research: {{ outputs.research_topic.research_data }}
          
          Create an outline for {{ inputs.topic }} with:
          - Maximum {{ inputs.max_sections }} sections
          - Style: {{ inputs.style }}
          - Audience: {{ inputs.target_audience }}
          
          Output as YAML:
          sections:
            - title: "Section Title"
              key_points: ["point1", "point2"]
              word_count: 300
        temperature: 0.6
        max_tokens: 800
      outputs:
        outline_yaml: "$.response"
        sections: "$.parsed.sections"
        total_word_count: "$.parsed.total_word_count"
    
    # Stage 2: Parallel content generation
    - name: "write_sections"
      type: "batch_llm"
      depends_on: ["generate_outline"]
      description: "Write content for each section in parallel"
      config:
        model: "gpt-4o"
        batch_input: "{{ outputs.generate_outline.sections }}"
        batch_size: 3           # Maximum parallel executions
        concurrency: 2          # Concurrent batch processing
        prompt: |
          Write {{ inputs.style }} content for this section:
          
          Title: {{ item.title }}
          Key Points: {{ item.key_points | join(', ') }}
          Target Word Count: {{ item.word_count }}
          Audience: {{ inputs.target_audience }}
          
          Requirements:
          - Engaging and informative
          - Match the specified style
          - Include examples where appropriate
          - Maintain consistency with overall topic: {{ inputs.topic }}
        temperature: 0.8
        max_tokens: 600
      outputs:
        section_contents: "$.batch_results"
        sections_completed: "$.batch_count"
        total_words: "$.batch_total_words"
    
    # Stage 3: Article assembly and refinement
    - name: "assemble_article"
      type: "template"
      depends_on: ["write_sections", "generate_outline"]
      description: "Combine sections into cohesive article"
      config:
        template_engine: "tera"  # tera | handlebars | jinja2
        template: |
          # {{ inputs.topic }}
          
          *Generated with AgentFlow - {{ metadata.created }}*
          
          ## Introduction
          
          Welcome to our exploration of {{ inputs.topic }}. This {{ inputs.style }} guide is designed for {{ inputs.target_audience }} readers.
          
          {% for section in outputs.write_sections.section_contents %}
          ## {{ section.title }}
          
          {{ section.content }}
          
          {% endfor %}
          
          ## Conclusion
          
          This comprehensive overview of {{ inputs.topic }} covers {{ outputs.write_sections.sections_completed }} key areas. For more information, consult the latest research and developments in this rapidly evolving field.
          
          ---
          *Article Statistics:*
          - Sections: {{ outputs.write_sections.sections_completed }}
          - Approximate word count: {{ outputs.write_sections.total_words }}
          - Style: {{ inputs.style }}
          - Target audience: {{ inputs.target_audience }}
      outputs:
        assembled_article: "$.rendered"
        article_stats: "$.stats"
    
    # Stage 4: Final review and polish
    - name: "review_and_polish"
      type: "llm"
      depends_on: ["assemble_article"]
      description: "Review article for consistency and polish"
      condition: "{{ inputs.style != 'draft' }}"  # Conditional execution
      config:
        model: "claude-3-sonnet"
        prompt: |
          Review and polish this article about {{ inputs.topic }}:
          
          {{ outputs.assemble_article.assembled_article }}
          
          Improvements needed:
          1. Ensure consistent tone and style ({{ inputs.style }})
          2. Check for smooth transitions between sections
          3. Verify appropriate level for {{ inputs.target_audience }}
          4. Add engaging hooks and conclusions
          5. Correct any grammatical issues
          
          Return the polished version maintaining the original structure.
        temperature: 0.3
        max_tokens: 2000
      outputs:
        polished_article: "$.response"
        improvements_made: "$.improvements"

# Output configuration
outputs:
  # Primary output: the final article
  article:
    source: |
      {% if nodes.review_and_polish.executed %}
        {{ outputs.review_and_polish.polished_article }}
      {% else %}
        {{ outputs.assemble_article.assembled_article }}
      {% endif %}
    format: "markdown"
    file: "{{ inputs.topic | slugify }}_article.md"
    
  # Secondary outputs
  outline:
    source: "{{ outputs.generate_outline.outline_yaml }}"
    format: "yaml"
    file: "{{ inputs.topic | slugify }}_outline.yml"
  
  research:
    source: "{{ outputs.research_topic.research_data }}"
    format: "json"
    file: "{{ inputs.topic | slugify }}_research.json"
  
  # Execution report
  execution_report:
    source: "$"  # Full execution context
    format: "json"
    file: "execution_report_{{ timestamp | date('%Y%m%d_%H%M%S') }}.json"
    include:
      - execution_time
      - nodes_executed
      - token_usage
      - costs
      - errors
      - performance_metrics

# Error handling and recovery
error_handling:
  global_retry: 3
  timeout_action: "fail"  # fail | skip | retry
  on_failure:
    - action: "save_state"
      file: "workflow_state_{{ timestamp }}.json"
    - action: "notify"
      method: "log"
      level: "error"

# Performance and resource limits
limits:
  max_execution_time: "30m"
  max_memory_usage: "2GB"
  max_tokens_per_request: 4000
  max_concurrent_requests: 5
  rate_limits:
    openai: "60/min"
    anthropic: "50/min"
```

### Workflow Types

#### 1. Sequential Workflows
```yaml
workflow:
  type: "sequential"
  nodes:
    - name: "step1"
      type: "llm"
      # ... configuration
    - name: "step2"
      depends_on: ["step1"]
      type: "template"
      # ... configuration
```

#### 2. Parallel Workflows
```yaml
workflow:
  type: "parallel"
  nodes:
    - name: "parallel_task_1"
      type: "llm"
      # ... runs concurrently
    - name: "parallel_task_2" 
      type: "llm"
      # ... runs concurrently
    - name: "combine_results"
      depends_on: ["parallel_task_1", "parallel_task_2"]
      type: "template"
      # ... waits for both parallel tasks
```

#### 3. Conditional Workflows
```yaml
workflow:
  type: "conditional"
  nodes:
    - name: "analyze_input"
      type: "llm"
      # ... 
    - name: "path_a"
      condition: "{{ outputs.analyze_input.category == 'technical' }}"
      type: "llm"
      # ... executed only if condition is true
    - name: "path_b"
      condition: "{{ outputs.analyze_input.category == 'general' }}"
      type: "llm"
      # ... alternative path
```

#### 4. Loop Workflows
```yaml
workflow:
  type: "loop"
  loop_config:
    max_iterations: 5
    break_condition: "{{ outputs.current_node.confidence > 0.9 }}"
  nodes:
    - name: "iterative_refinement"
      type: "llm"
      config:
        prompt: |
          Iteration {{ loop.current_iteration }} of {{ loop.max_iterations }}
          Previous result: {{ outputs.iterative_refinement.result | default('') }}
          
          Improve the following: {{ inputs.content }}
```

## Node Types

### Built-in Node Types

#### 1. LLM Node
```yaml
- name: "llm_task"
  type: "llm"
  config:
    model: "gpt-4o"
    prompt: "{{ template_string }}"
    temperature: 0.7
    max_tokens: 1000
    top_p: 0.9
    frequency_penalty: 0.0
    presence_penalty: 0.0
    timeout: "2m"
    system_prompt: "You are a helpful assistant"
    response_format: "json"  # json | text
  outputs:
    response: "$.response"
    tokens_used: "$.usage.total_tokens"
    model_used: "$.model"
```

#### 2. Batch LLM Node
```yaml
- name: "batch_process"
  type: "batch_llm"
  config:
    model: "claude-3-sonnet"
    batch_input: "{{ array_of_items }}"
    batch_size: 3
    concurrency: 2
    prompt: "Process this item: {{ item }}"
    temperature: 0.8
  outputs:
    batch_results: "$.batch_results"
    batch_count: "$.batch_count"
    total_tokens: "$.total_tokens"
```

#### 3. Template Node
```yaml
- name: "render_template"
  type: "template"
  config:
    template_engine: "tera"
    template: |
      # Report for {{ title }}
      Generated on: {{ timestamp }}
      {% for item in items %}
      - {{ item.name }}: {{ item.value }}
      {% endfor %}
    context:
      title: "{{ inputs.report_title }}"
      timestamp: "{{ now() }}"
      items: "{{ outputs.previous_step.results }}"
  outputs:
    rendered: "$.rendered"
    template_used: "$.template"
```

#### 4. File I/O Node
```yaml
- name: "read_file"
  type: "file"
  config:
    operation: "read"  # read | write | append
    path: "{{ inputs.file_path }}"
    encoding: "utf-8"
    format: "text"  # text | json | yaml | csv
  outputs:
    content: "$.content"
    file_info: "$.file_info"

- name: "write_output"
  type: "file"
  config:
    operation: "write"
    path: "output/{{ inputs.filename }}"
    content: "{{ outputs.generate_content.result }}"
    format: "markdown"
    create_dirs: true
  outputs:
    file_path: "$.written_path"
    bytes_written: "$.bytes_written"
```

#### 5. HTTP Request Node
```yaml
- name: "api_call"
  type: "http"
  config:
    method: "POST"
    url: "https://api.example.com/endpoint"
    headers:
      Content-Type: "application/json"
      Authorization: "Bearer {{ env.API_TOKEN }}"
    body:
      query: "{{ inputs.search_query }}"
      limit: 10
    timeout: "30s"
  outputs:
    response: "$.response"
    status_code: "$.status_code"
    headers: "$.headers"
```

#### 6. Conditional Node
```yaml
- name: "conditional_logic"
  type: "conditional"
  config:
    conditions:
      - condition: "{{ inputs.mode == 'fast' }}"
        node: "quick_process"
      - condition: "{{ inputs.mode == 'thorough' }}"
        node: "detailed_process"
      - default: "standard_process"
  outputs:
    selected_path: "$.selected_path"
    result: "$.result"
```

#### 7. Custom Node
```yaml
- name: "custom_processing"
  type: "custom"
  config:
    implementation: "my_custom_nodes::DataProcessor"
    parameters:
      algorithm: "advanced"
      threshold: 0.85
  outputs:
    processed_data: "$.result"
    confidence: "$.confidence"
```

## Integration with AgentFlow Core

### Workflow Runner Implementation

```rust
// agentflow-cli/src/executor/runner.rs
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use crate::config::WorkflowConfig;

pub struct WorkflowRunner {
    config: WorkflowConfig,
    shared_state: SharedState,
    metrics_collector: Option<Arc<MetricsCollector>>,
}

impl WorkflowRunner {
    pub async fn new(config: WorkflowConfig) -> Result<Self> {
        let shared_state = SharedState::new();
        
        // Initialize with input parameters
        for (key, value) in config.inputs {
            shared_state.insert(key, value);
        }
        
        Ok(Self {
            config,
            shared_state,
            metrics_collector: None,
        })
    }
    
    pub async fn run(&self) -> Result<ExecutionReport> {
        let start_time = std::time::Instant::now();
        
        // Build execution graph
        let execution_graph = self.build_execution_graph().await?;
        
        // Execute workflow based on type
        let result = match self.config.workflow.workflow_type {
            WorkflowType::Sequential => self.run_sequential(execution_graph).await?,
            WorkflowType::Parallel => self.run_parallel(execution_graph).await?,
            WorkflowType::Conditional => self.run_conditional(execution_graph).await?,
            WorkflowType::Loop => self.run_loop(execution_graph).await?,
        };
        
        let duration = start_time.elapsed();
        
        // Generate outputs
        self.generate_outputs().await?;
        
        Ok(ExecutionReport {
            duration,
            nodes_executed: result.nodes_executed,
            outputs_generated: result.outputs_generated,
            errors: result.errors,
            token_usage: result.token_usage,
            performance_metrics: result.performance_metrics,
        })
    }
    
    async fn build_execution_graph(&self) -> Result<ExecutionGraph> {
        let mut graph = ExecutionGraph::new();
        
        for node_config in &self.config.workflow.nodes {
            let node = self.create_node_from_config(node_config).await?;
            graph.add_node(node_config.name.clone(), node, node_config.depends_on.clone());
        }
        
        graph.validate()?;
        Ok(graph)
    }
    
    async fn create_node_from_config(&self, config: &NodeConfig) -> Result<Box<dyn AsyncNode>> {
        match config.node_type.as_str() {
            "llm" => Ok(Box::new(LLMNode::from_config(config).await?)),
            "batch_llm" => Ok(Box::new(BatchLLMNode::from_config(config).await?)),
            "template" => Ok(Box::new(TemplateNode::from_config(config).await?)),
            "file" => Ok(Box::new(FileNode::from_config(config).await?)),
            "http" => Ok(Box::new(HTTPNode::from_config(config).await?)),
            "conditional" => Ok(Box::new(ConditionalNode::from_config(config).await?)),
            "custom" => Ok(Box::new(CustomNode::from_config(config).await?)),
            _ => Err(AgentFlowError::UnsupportedNodeType {
                node_type: config.node_type.clone(),
            }),
        }
    }
}
```

### Node Implementation Pattern

```rust
// agentflow-cli/src/executor/nodes/llm.rs
use agentflow_core::{AsyncNode, SharedState, Result};
use agentflow_llm::AgentFlow;
use async_trait::async_trait;

pub struct LLMNode {
    name: String,
    model: String,
    prompt_template: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    timeout: Option<Duration>,
}

impl LLMNode {
    pub async fn from_config(config: &NodeConfig) -> Result<Self> {
        Ok(Self {
            name: config.name.clone(),
            model: config.get_required_string("model")?,
            prompt_template: config.get_required_string("prompt")?,
            temperature: config.get_optional_f32("temperature"),
            max_tokens: config.get_optional_u32("max_tokens"),
            timeout: config.get_optional_duration("timeout"),
        })
    }
}

#[async_trait]
impl AsyncNode for LLMNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
        // Render prompt template with current context
        let context = self.build_template_context(shared)?;
        let rendered_prompt = self.render_template(&self.prompt_template, &context)?;
        
        Ok(json!({
            "prompt": rendered_prompt,
            "model": self.model,
            "parameters": {
                "temperature": self.temperature,
                "max_tokens": self.max_tokens
            }
        }))
    }
    
    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        let prompt = prep_result["prompt"].as_str().unwrap();
        
        // Build LLM request
        let mut request = AgentFlow::model(&self.model)
            .prompt(prompt);
            
        if let Some(temp) = self.temperature {
            request = request.temperature(temp);
        }
        
        if let Some(tokens) = self.max_tokens {
            request = request.max_tokens(tokens);
        }
        
        // Execute with timeout if specified
        let response = if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, request.execute()).await??
        } else {
            request.execute().await?
        };
        
        Ok(json!({
            "response": response,
            "model": self.model,
            "tokens_used": 0, // TODO: Extract from response
            "timestamp": chrono::Utc::now()
        }))
    }
    
    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        // Store outputs in shared state with node prefix
        let node_prefix = format!("outputs.{}", self.name);
        shared.insert(format!("{}.response", node_prefix), exec["response"].clone());
        shared.insert(format!("{}.tokens_used", node_prefix), exec["tokens_used"].clone());
        shared.insert(format!("{}.model", node_prefix), exec["model"].clone());
        
        // No specific next node (handled by workflow runner)
        Ok(None)
    }
    
    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}
```

## File Input/Output Support

### File Input Handling

#### Text Files
```bash
# Read text file as prompt input
agentflow llm prompt "Analyze this code" --file src/main.rs --model gpt-4o

# Use in workflow
- name: "analyze_code"
  type: "llm"
  config:
    model: "claude-3-sonnet"
    prompt: |
      Analyze this code for potential improvements:
      
      {{ file_content('src/main.rs') }}
```

#### Image Files
```bash
# Image analysis
agentflow llm prompt "Describe this image" --file image.jpg --model step-1o-turbo-vision

# Batch image processing in workflow
- name: "process_images"
  type: "batch_llm"
  config:
    model: "step-1o-turbo-vision"
    batch_input: "{{ glob('images/*.jpg') }}"
    prompt: "Analyze this image: {{ item }}"
```

#### Audio Files
```bash
# Audio transcription
agentflow llm prompt "Transcribe this audio" --file audio.mp3 --model whisper-1

# Audio analysis workflow
- name: "transcribe_audio"
  type: "llm"
  config:
    model: "whisper-1"
    prompt: "Transcribe: {{ audio_file('meeting.mp3') }}"
```

### File Output Handling

#### Single File Output
```bash
# Save to specific file
agentflow llm prompt "Generate report" --model gpt-4o --output report.md

# Workflow output configuration
outputs:
  report:
    source: "{{ outputs.generate_report.response }}"
    format: "markdown"
    file: "reports/{{ inputs.topic | slugify }}.md"
```

#### Multiple File Outputs
```yaml
outputs:
  summary:
    source: "{{ outputs.summarize.response }}"
    format: "text"
    file: "summary.txt"
  
  detailed_report:
    source: "{{ outputs.detailed_analysis.response }}"
    format: "markdown"
    file: "detailed_report.md"
  
  data:
    source: "{{ outputs.extract_data.structured_data }}"
    format: "json"
    file: "extracted_data.json"
```

### File Utility Functions

Available in templates:
- `file_content(path)` - Read file content as string
- `file_json(path)` - Parse JSON file
- `file_yaml(path)` - Parse YAML file
- `file_csv(path)` - Parse CSV file
- `glob(pattern)` - List files matching pattern
- `audio_file(path)` - Process audio file for transcription
- `image_file(path)` - Process image file for analysis

## Implementation Plan

### Phase 1: Foundation (2-3 weeks)

#### 1.1 Project Setup
- [ ] Create `agentflow-cli` crate in workspace
- [ ] Configure `Cargo.toml` to produce `agentflow` binary
- [ ] Set up project structure and dependencies
- [ ] Configure CI/CD for CLI testing

#### 1.2 Core CLI Framework
- [ ] Implement main CLI with `clap` argument parsing
- [ ] Create command structure and routing
- [ ] Add basic error handling and logging
- [ ] Implement configuration loading

#### 1.3 Basic LLM Commands
- [ ] `agentflow llm prompt` - text prompting
- [ ] `agentflow llm models` - list available models
- [ ] File input support (text files)
- [ ] Basic output formatting

### Phase 2: Workflow Engine (3-4 weeks)

#### 2.1 Configuration Parser
- [ ] YAML workflow parser with schema validation
- [ ] Template engine integration (Tera)
- [ ] Input parameter handling
- [ ] Configuration validation and error reporting

#### 2.2 Core Node Types
- [ ] LLM node implementation
- [ ] Template node implementation
- [ ] File I/O node implementation
- [ ] HTTP request node implementation

#### 2.3 Workflow Runner
- [ ] Sequential execution engine
- [ ] Dependency resolution
- [ ] Context management and state handling
- [ ] Error handling and recovery

#### 2.4 Basic Workflow Commands
- [ ] `agentflow run` - execute workflows
- [ ] `agentflow validate` - validate configurations
- [ ] Progress indicators and status reporting

### Phase 3: Advanced Features (3-4 weeks)

#### 3.1 Advanced Workflow Types
- [ ] Parallel execution support
- [ ] Conditional branching
- [ ] Loop workflows
- [ ] Batch processing node

#### 3.2 Enhanced File Support
- [ ] Image file processing
- [ ] Audio file processing
- [ ] Multiple output file generation
- [ ] File utility functions in templates

#### 3.3 Advanced LLM Features
- [ ] Interactive chat mode
- [ ] Streaming output support
- [ ] Multimodal input handling
- [ ] Configuration management commands

### Phase 4: Polish and Integration (2-3 weeks)

#### 4.1 User Experience
- [ ] Rich error messages and suggestions
- [ ] Progress bars and status indicators
- [ ] Help documentation and examples
- [ ] Configuration wizards

#### 4.2 Performance and Reliability
- [ ] Execution optimization
- [ ] Memory usage optimization
- [ ] Comprehensive test suite
- [ ] Performance benchmarking

#### 4.3 Documentation and Examples
- [ ] Complete user documentation
- [ ] Workflow template library
- [ ] Video tutorials and guides
- [ ] Integration examples

### Phase 5: Future Extensions

#### 5.1 RAG Integration (Future)
- [ ] `agentflow rag` command group
- [ ] Document indexing and retrieval
- [ ] Vector database integration
- [ ] Knowledge base management

#### 5.2 Advanced Features
- [ ] Workflow debugging tools
- [ ] Performance profiling
- [ ] Cloud deployment support
- [ ] Plugin system for custom nodes

## Examples

### Example 1: Simple Text Generation
```bash
# Direct LLM usage
agentflow llm prompt "Write a haiku about programming" --model gpt-4o

# Equivalent workflow
agentflow run examples/simple-haiku.yml --input topic="programming"
```

### Example 2: Document Analysis Pipeline
```yaml
# document-analysis.yml
name: "Document Analysis Pipeline"
description: "Analyze documents and generate insights"

inputs:
  document_path:
    type: "string"
    required: true

workflow:
  type: "sequential"
  nodes:
    - name: "extract_text"
      type: "file"
      config:
        operation: "read"
        path: "{{ inputs.document_path }}"
      outputs:
        content: "$.content"
    
    - name: "analyze_content"
      type: "llm"
      config:
        model: "claude-3-sonnet"
        prompt: |
          Analyze this document and provide:
          1. Main topics covered
          2. Key insights
          3. Sentiment analysis
          4. Summary
          
          Document content:
          {{ outputs.extract_text.content }}
      outputs:
        analysis: "$.response"
    
    - name: "generate_report"
      type: "template"
      config:
        template: |
          # Document Analysis Report
          
          **Document:** {{ inputs.document_path }}
          **Analysis Date:** {{ now() }}
          
          ## Analysis Results
          
          {{ outputs.analyze_content.analysis }}

outputs:
  report:
    source: "{{ outputs.generate_report.rendered }}"
    format: "markdown"
    file: "analysis_report.md"
```

### Example 3: Batch Image Processing
```yaml
# batch-image-analysis.yml
name: "Batch Image Analysis"
description: "Analyze multiple images and generate descriptions"

inputs:
  image_directory:
    type: "string"
    required: true
    default: "images/"

workflow:
  type: "sequential"
  nodes:
    - name: "find_images"
      type: "file"
      config:
        operation: "glob"
        pattern: "{{ inputs.image_directory }}/*.{jpg,jpeg,png}"
      outputs:
        image_files: "$.files"
    
    - name: "analyze_images"
      type: "batch_llm"
      config:
        model: "step-1o-turbo-vision"
        batch_input: "{{ outputs.find_images.image_files }}"
        batch_size: 5
        prompt: |
          Analyze this image and provide:
          1. A detailed description
          2. Objects and people identified
          3. Setting/environment
          4. Mood or atmosphere
          
          Image: {{ item }}
      outputs:
        analyses: "$.batch_results"
    
    - name: "create_gallery"
      type: "template"
      config:
        template: |
          # Image Gallery Analysis
          
          Generated on: {{ now() }}
          
          {% for analysis in outputs.analyze_images.analyses %}
          ## {{ analysis.filename }}
          
          {{ analysis.description }}
          
          ---
          {% endfor %}

outputs:
  gallery:
    source: "{{ outputs.create_gallery.rendered }}"
    format: "markdown"
    file: "image_gallery.md"
```

### Example 4: Research and Writing Pipeline
```bash
# Usage
agentflow run research-pipeline.yml \
  --input topic="Sustainable Energy Technologies" \
  --input style="academic" \
  --input max_sections=6 \
  --output research_report.md
```

## Future Extensions

### 1. Plugin System
```yaml
# Enable custom node types
- name: "custom_analysis"
  type: "plugin:data-science/advanced-stats"
  config:
    algorithm: "lstm"
    parameters:
      window_size: 30
      prediction_horizon: 7
```

### 2. Cloud Integration
```yaml
# Cloud execution
workflow:
  execution:
    provider: "aws"
    instance_type: "c5.xlarge"
    timeout: "1h"
    auto_scale: true
```

### 3. Real-time Workflows
```yaml
# Event-driven workflows
triggers:
  - type: "file_watch"
    path: "input/*.txt"
    workflow: "process-document.yml"
  - type: "http_webhook"
    endpoint: "/api/process"
    workflow: "api-handler.yml"
```

### 4. Workflow Debugging
```bash
# Debug workflow execution
agentflow run workflow.yml --debug --step-through
agentflow run workflow.yml --profile --output-trace trace.json
```

This comprehensive design provides a solid foundation for the AgentFlow CLI implementation, ensuring it integrates seamlessly with the existing AgentFlow ecosystem while providing powerful new capabilities for workflow orchestration and automation.