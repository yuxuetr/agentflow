# AgentFlow Configuration Reference

**Version**: 2.0  
**Last Updated**: 2025-08-26  
**Status**: Design Document

## Overview

This document provides a comprehensive reference for AgentFlow's enhanced YAML configuration format. The configuration-first approach allows users to build sophisticated AI workflows without writing code.

## Configuration File Structure

### Complete Configuration Template

```yaml
version: "2.0"
metadata:
  name: "Workflow Name"
  description: "Workflow description"
  author: "Author Name"
  version: "1.0.0"
  tags: ["ai", "analysis", "automation"]

shared:
  variable_name:
    type: string|number|boolean|object|array
    description: "Variable description"
    default: default_value
    required: true|false

templates:
  prompts:
    template_name: |
      Multi-line template content
      with {{shared.variable_name}} interpolation
  outputs:
    output_template: |
      Result template with {{shared.result}}

parameters:
  node_name:
    parameter_key: parameter_value
    temperature: 0.8
    max_tokens: 1000

nodes:
  - name: node_name
    type: node_type
    model: model_name
    depends_on: [previous_node1, previous_node2]
    condition: "{{shared.enable_feature}}"
    prompt: "{{templates.prompts.template_name}}"
    parameters: "{{parameters.node_name}}"
    inputs:
      - source_reference
    outputs:
      - target: shared.variable_name
        format: json|text|markdown
        transform: optional_transform_function
```

## Configuration Sections

### 1. Version and Metadata

```yaml
version: "2.0"  # Configuration format version
metadata:
  name: "Document Analysis Pipeline"
  description: "Analyze and summarize documents using AI"
  author: "AgentFlow Team"
  version: "1.2.0"
  created: "2025-01-15"
  updated: "2025-01-20"
  tags: ["document", "analysis", "summarization"]
  license: "MIT"
```

**Fields:**
- `version`: Configuration format version (required)
- `name`: Human-readable workflow name
- `description`: Detailed workflow description  
- `author`: Workflow creator
- `version`: Workflow version (semantic versioning)
- `tags`: Searchable workflow tags
- `license`: Usage license

### 2. Shared Variables

Define workflow-scoped variables with type safety and validation.

```yaml
shared:
  # String variable with default
  document_content:
    type: string
    description: "Raw document text to analyze"
    default: ""
    required: true
    
  # Numeric variable with constraints
  confidence_threshold:
    type: number
    description: "Minimum confidence score (0.0-1.0)"
    default: 0.8
    minimum: 0.0
    maximum: 1.0
    
  # Object variable for structured data
  analysis_result:
    type: object
    description: "Structured analysis output"
    schema:
      type: object
      properties:
        topics: 
          type: array
          items: 
            type: string
        sentiment:
          type: number
          minimum: -1
          maximum: 1
          
  # Array variable
  processing_stages:
    type: array
    description: "List of processing stages to execute"
    default: ["parse", "analyze", "summarize"]
    items:
      type: string
      enum: ["parse", "analyze", "summarize", "translate"]
      
  # Boolean flag
  enable_translation:
    type: boolean
    description: "Whether to include translation step"
    default: false
```

**Variable Types:**
- `string`: Text data with optional length constraints
- `number`: Numeric values with min/max constraints  
- `boolean`: True/false values
- `object`: Structured data with JSON schema validation
- `array`: Lists with type constraints on items

**Constraints:**
- `required`: Whether variable must be provided
- `default`: Default value if not provided
- `minimum`/`maximum`: Numeric range constraints
- `minLength`/`maxLength`: String length constraints
- `enum`: Allowed values list
- `schema`: JSON schema for complex validation

### 3. Templates

Reusable template definitions with Handlebars syntax.

```yaml
templates:
  prompts:
    system_analyzer: |
      You are an expert document analyzer. Your task is to:
      1. Extract key topics and themes
      2. Determine overall sentiment (-1 to 1 scale)  
      3. Assess reading complexity (1-10 scale)
      4. Identify important entities
      
      Respond in valid JSON format only.
      
    document_analysis: |
      Analyze the following document:
      
      === DOCUMENT START ===
      {{shared.document_content}}
      === DOCUMENT END ===
      
      Requirements:
      - Minimum confidence: {{shared.confidence_threshold}}
      - Output format: JSON
      - Include: topics, sentiment, complexity, entities
      
    summarization: |
      Based on this analysis:
      {{shared.analysis_result | json}}
      
      Create a {{parameters.summary_length}} summary that covers:
      - Main themes and topics
      - Key findings and insights  
      - Important entities and relationships
      - Overall conclusions
      
  outputs:
    analysis_report: |
      # Document Analysis Report
      
      ## Summary
      {{shared.final_summary}}
      
      ## Key Topics
      {{#each shared.analysis_result.topics}}
      - {{this}}
      {{/each}}
      
      ## Sentiment Analysis
      Overall sentiment: {{shared.analysis_result.sentiment}}
      
      ## Generated
      *Report generated on {{timestamp}} using AgentFlow*
      
    json_output: |
      {
        "summary": "{{shared.final_summary}}",
        "analysis": {{shared.analysis_result | json}},
        "metadata": {
          "generated_at": "{{timestamp}}",
          "workflow": "{{metadata.name}}",
          "version": "{{metadata.version}}"
        }
      }
```

**Template Features:**
- **Handlebars Syntax**: `{{variable}}` interpolation
- **Helpers**: `{{json}}`, `{{timestamp}}`, etc.
- **Conditionals**: `{{#if condition}}...{{/if}}`
- **Loops**: `{{#each array}}...{{/each}}`
- **Multi-line Support**: Using YAML `|` syntax
- **Nested References**: `{{shared.analysis_result.sentiment}}`

**Built-in Helpers:**
- `{{json variable}}`: Format as JSON
- `{{timestamp}}`: Current ISO timestamp
- `{{uuid}}`: Generate UUID
- `{{length array}}`: Array/string length
- `{{uppercase text}}`: Convert to uppercase
- `{{lowercase text}}`: Convert to lowercase

### 4. Parameters

Node-specific configuration parameters.

```yaml
parameters:
  analyzer:
    model: "gpt-4o"
    temperature: 0.3
    max_tokens: 2000
    response_format: "json"
    timeout_seconds: 30
    
  summarizer:
    model: "claude-3-sonnet"
    temperature: 0.7
    max_tokens: 800
    summary_length: "concise"  # concise, detailed, comprehensive
    
  translator:
    model: "gpt-4o"
    temperature: 0.1
    target_languages: ["es", "fr", "de"]
    preserve_formatting: true
    
  image_generator:
    model: "dall-e-3"
    size: "1024x1024"
    quality: "hd"
    style: "natural"
    
  global:
    retry_count: 3
    retry_delay_ms: 1000
    circuit_breaker_threshold: 5
    rate_limit_requests_per_minute: 60
```

**Parameter Categories:**
- **Model Settings**: `model`, `temperature`, `max_tokens`
- **Request Configuration**: `timeout_seconds`, `retry_count`
- **Format Options**: `response_format`, `output_format`
- **Provider-Specific**: `style`, `quality`, `size` (for image models)
- **Workflow Control**: `rate_limit`, `circuit_breaker_threshold`

### 5. Nodes

Individual processing units that make up the workflow.

```yaml
nodes:
  # LLM Analysis Node
  - name: document_analyzer
    type: llm
    model: "{{parameters.analyzer.model}}"
    prompt: "{{templates.prompts.document_analysis}}"
    system: "{{templates.prompts.system_analyzer}}"
    parameters:
      temperature: "{{parameters.analyzer.temperature}}"
      max_tokens: "{{parameters.analyzer.max_tokens}}"
      response_format: "{{parameters.analyzer.response_format}}"
    outputs:
      - target: shared.analysis_result
        format: json
        validate_schema: true
        
  # Conditional Processing Node  
  - name: quality_check
    type: conditional
    depends_on: [document_analyzer]
    condition: "{{shared.analysis_result.confidence >= shared.confidence_threshold}}"
    on_true: "summarizer"
    on_false: "error_handler"
    
  # Summarization Node
  - name: summarizer
    type: llm
    model: "{{parameters.summarizer.model}}"
    depends_on: [quality_check]
    prompt: "{{templates.prompts.summarization}}"
    parameters: "{{parameters.summarizer}}"
    outputs:
      - target: shared.final_summary
        
  # Translation Node (Conditional)
  - name: translator
    type: llm
    model: "{{parameters.translator.model}}"
    depends_on: [summarizer]
    condition: "{{shared.enable_translation}}"
    prompt: |
      Translate the following summary to {{parameters.translator.target_languages | join ', '}}:
      
      {{shared.final_summary}}
    parameters: "{{parameters.translator}}"
    outputs:
      - target: shared.translated_summaries
        format: object
        
  # File Output Node
  - name: report_generator
    type: file
    depends_on: [summarizer, translator]
    template: "{{templates.outputs.analysis_report}}"
    outputs:
      - target: "analysis_report.md"
        format: markdown
      - target: "analysis_data.json" 
        template: "{{templates.outputs.json_output}}"
        format: json
        
  # Parallel Processing Node
  - name: batch_processor
    type: batch
    depends_on: [document_analyzer]
    batch_size: 5
    max_concurrent: 3
    items: "{{shared.document_list}}"
    node_template:
      type: llm
      model: "gpt-4o"
      prompt: "Process: {{item.content}}"
    outputs:
      - target: shared.batch_results
        format: array
```

**Node Types:**

#### LLM Node
```yaml
- name: llm_node
  type: llm
  model: model_name
  prompt: "Prompt text with {{variables}}"
  system: "System prompt (optional)"
  parameters:
    temperature: 0.7
    max_tokens: 1000
    top_p: 0.9
    frequency_penalty: 0.0
```

#### HTTP Node  
```yaml
- name: api_call
  type: http
  method: GET|POST|PUT|DELETE
  url: "https://api.example.com/{{endpoint}}"
  headers:
    Authorization: "Bearer {{api_token}}"
    Content-Type: "application/json"
  body: "{{request_payload}}"
  timeout: 30000
```

#### File Node
```yaml
- name: file_processor
  type: file
  operation: read|write|append
  path: "/path/to/{{filename}}"
  encoding: utf-8
  template: "{{templates.outputs.file_content}}"  # For write operations
```

#### Template Node
```yaml
- name: formatter
  type: template
  template: "{{templates.outputs.report}}"
  output_format: markdown|json|yaml|text
```

#### Conditional Node
```yaml
- name: decision_point
  type: conditional
  condition: "{{shared.score > 0.8}}"
  on_true: "success_handler"
  on_false: "retry_handler"
```

#### Batch Processing Node
```yaml
- name: batch_operation
  type: batch
  items: "{{shared.item_list}}"
  batch_size: 10
  max_concurrent: 3
  node_template:
    type: llm
    prompt: "Process: {{item}}"
```

#### Loop Node
```yaml
- name: iterative_processor
  type: loop
  items: "{{shared.data_array}}"
  max_iterations: 100
  break_condition: "{{item.processed == true}}"
  node_template:
    type: llm
    prompt: "Process item: {{item}}"
```

#### Code Node
```yaml
- name: custom_processor
  type: code
  language: javascript|python|lua
  code: |
    // Custom processing logic
    function process(input) {
      return {
        processed: true,
        result: input.data.toUpperCase()
      };
    }
    return process(input);
```

## Advanced Features

### 1. Template Expressions

Complex template expressions with conditionals and loops:

```yaml
templates:
  prompts:
    conditional_prompt: |
      {{#if shared.include_context}}
      Context: {{shared.context_data}}
      {{/if}}
      
      {{#each shared.requirements}}
      Requirement {{@index}}: {{this}}
      {{/each}}
      
      {{#unless shared.skip_examples}}
      Examples:
      {{#each shared.examples}}
      - {{this.title}}: {{this.description}}
      {{/each}}
      {{/unless}}
```

### 2. Dynamic Node Creation

Create nodes dynamically based on runtime conditions:

```yaml
nodes:
  - name: dynamic_processor
    type: dynamic
    condition: "{{shared.processing_modes}}"
    node_templates:
      simple:
        type: llm
        model: "gpt-3.5-turbo"
        prompt: "Simple processing: {{input}}"
      advanced:
        type: llm  
        model: "gpt-4o"
        prompt: "Advanced analysis: {{input}}"
      batch:
        type: batch
        batch_size: 5
        node_template:
          type: llm
          prompt: "Batch process: {{item}}"
```

### 3. Error Handling

Comprehensive error handling and recovery:

```yaml
nodes:
  - name: robust_processor
    type: llm
    model: "gpt-4o"
    prompt: "{{templates.prompts.main}}"
    error_handling:
      retry_count: 3
      retry_delay_ms: 1000
      retry_backoff: exponential
      fallback_node: "backup_processor"
      on_error: "error_logger"
      continue_on_error: false
      
  - name: backup_processor
    type: llm
    model: "gpt-3.5-turbo" 
    prompt: "{{templates.prompts.simplified}}"
    
  - name: error_logger
    type: file
    operation: append
    path: "error.log"
    template: |
      {{timestamp}}: Error in {{node.name}}
      Error: {{error.message}}
      Input: {{error.input}}
```

### 4. Parallel Execution

Execute multiple nodes in parallel:

```yaml
nodes:
  - name: parallel_analysis
    type: parallel
    nodes:
      - name: sentiment_analysis
        type: llm
        model: "gpt-4o"
        prompt: "Analyze sentiment: {{input}}"
        
      - name: topic_extraction
        type: llm
        model: "claude-3-sonnet"
        prompt: "Extract topics: {{input}}"
        
      - name: entity_recognition
        type: llm
        model: "gpt-4o"
        prompt: "Identify entities: {{input}}"
        
    outputs:
      - target: shared.parallel_results
        format: object
        combine_strategy: merge  # merge, array, first_success
```

### 5. Workflow Composition

Include and compose workflows:

```yaml
# main_workflow.yml
version: "2.0"
metadata:
  name: "Main Analysis Pipeline"

includes:
  - preprocessing.yml
  - analysis.yml
  - postprocessing.yml

nodes:
  - name: preprocessor
    type: workflow
    workflow: preprocessing
    inputs:
      raw_data: "{{input.data}}"
      
  - name: analyzer  
    type: workflow
    workflow: analysis
    depends_on: [preprocessor]
    inputs:
      clean_data: "{{shared.preprocessed_data}}"
      
  - name: postprocessor
    type: workflow
    workflow: postprocessing
    depends_on: [analyzer]
    inputs:
      results: "{{shared.analysis_results}}"
```

## Configuration Validation

### Schema Validation

AgentFlow validates configurations against JSON Schema:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["version", "metadata", "nodes"],
  "properties": {
    "version": {
      "type": "string",
      "enum": ["2.0"]
    },
    "metadata": {
      "type": "object",
      "required": ["name"],
      "properties": {
        "name": {"type": "string"},
        "description": {"type": "string"}
      }
    },
    "nodes": {
      "type": "array",
      "minItems": 1,
      "items": {"$ref": "#/definitions/node"}
    }
  }
}
```

### Validation Commands

```bash
# Validate configuration file
agentflow config validate workflow.yml

# Validate with specific schema version
agentflow config validate workflow.yml --schema-version 2.0

# Validate and show detailed errors
agentflow config validate workflow.yml --verbose

# Dry run validation (parse and validate without execution)
agentflow config validate workflow.yml --dry-run
```

## Migration Guide

### From Version 1.x to 2.0

Key changes in version 2.0:

1. **Enhanced shared variables** with type constraints
2. **Template system** with Handlebars syntax
3. **Improved node types** with more configuration options  
4. **Advanced execution patterns** (parallel, batch, conditional)

Migration steps:

```bash
# Automatic migration
agentflow config migrate workflow_v1.yml --target-version 2.0

# Manual migration with guidance
agentflow config migrate workflow_v1.yml --interactive

# Validation after migration
agentflow config validate workflow_v2.yml
```

## Best Practices

### 1. Configuration Organization

```yaml
# Use clear, descriptive names
shared:
  user_input_text:  # Not: input1
    type: string
    description: "User-provided text for analysis"
    
# Group related parameters
parameters:
  analysis_models:
    primary_model: "gpt-4o"
    fallback_model: "gpt-3.5-turbo"
    temperature: 0.7
    
# Use meaningful node names that describe their purpose
nodes:
  - name: content_parser      # Not: node1
  - name: sentiment_analyzer  # Not: llm_node
  - name: report_generator    # Not: output
```

### 2. Template Management

```yaml
# Keep templates focused and reusable
templates:
  prompts:
    # Specific, single-purpose templates
    extract_entities: |
      Extract named entities from: {{input}}
      
    analyze_sentiment: |
      Analyze sentiment of: {{input}}
      Return score from -1 to 1.
      
    # Avoid overly complex templates
    # Instead of one massive template, use multiple focused ones
```

### 3. Error Handling Strategy

```yaml
# Always include error handling for external dependencies
nodes:
  - name: api_dependent_node
    type: http
    url: "https://external-api.com/process"
    error_handling:
      retry_count: 3
      fallback_node: "offline_processor"
      
# Use validation for critical data
  - name: data_validator
    type: conditional
    condition: "{{shared.data.length > 0}}"
    on_false: "error_handler"
```

### 4. Performance Optimization

```yaml
# Use parallel processing where possible
nodes:
  - name: parallel_analysis
    type: parallel
    nodes: [sentiment_node, entity_node, topic_node]
    
# Implement batch processing for large datasets
  - name: document_processor
    type: batch
    items: "{{shared.documents}}"
    batch_size: 10
    max_concurrent: 3
    
# Set appropriate timeouts
parameters:
  global:
    timeout_seconds: 30
    retry_count: 2
```

---

This configuration reference provides the complete specification for AgentFlow's YAML-based workflow definition system. For implementation examples, see the [examples directory](../examples/) and [Architecture Documentation](ARCHITECTURE.md).