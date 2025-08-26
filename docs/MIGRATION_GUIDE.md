# AgentFlow Migration Guide

**Version**: 2.0  
**Last Updated**: 2025-08-26  
**Status**: Implementation Guide

## Overview

This guide provides detailed instructions for migrating to AgentFlow's new modular architecture, which separates code-first and configuration-first approaches into distinct crates.

## Architecture Changes

### Before (v1.x): Monolithic Structure
```
agentflow/
├── agentflow-core/
│   ├── src/workflow_runner.rs    # Mixed concerns
│   ├── src/config.rs             # Configuration logic
│   └── src/nodes/                # All node types
├── agentflow-llm/                # LLM integration
└── agentflow-cli/                # CLI interface
```

### After (v2.0): Modular Architecture  
```
agentflow/
├── agentflow-core/               # Pure code-first foundation
├── agentflow-config/            # Configuration-first support
├── agentflow-llm/               # Unified LLM provider abstraction
├── agentflow-mcp/               # Model Context Protocol support
├── agentflow-agents/            # Reusable agent applications
└── agentflow-cli/               # Unified command-line interface
```

## Migration Steps

### Phase 1: Code Migration

#### 1.1 Update Dependencies

**Before (v1.x)**:
```toml
[dependencies]
agentflow = "0.1.0"                    # Monolithic dependency
```

**After (v2.0)** - Choose what you need:
```toml
# For code-first development
[dependencies]
agentflow-core = "0.2.0"
agentflow-llm = "0.2.0"

# OR for configuration-first usage
[dependencies]
agentflow-config = "0.2.0"            # Includes agentflow-core

# OR for complete functionality
[dependencies]
agentflow-core = "0.2.0"
agentflow-config = "0.2.0" 
agentflow-llm = "0.2.0"
agentflow-agents = "0.2.0"
```

#### 1.2 Update Import Statements

**Before (v1.x)**:
```rust
use agentflow::{
    AsyncFlow, AsyncNode, SharedState,
    ConfigWorkflowRunner, WorkflowConfig,
    AgentFlow, LLMProvider
};
```

**After (v2.0)**:
```rust
// Code-first imports
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, AgentFlowError};

// Configuration-first imports  
use agentflow_config::{ConfigWorkflowRunner, WorkflowConfig, ConfigCompiler};

// LLM integration imports
use agentflow_llm::{AgentFlow, LLMProvider, ModelRegistry};

// Agent utilities
use agentflow_agents::{AgentApplication, FileAgent, BatchProcessor};
```

#### 1.3 Update Configuration Workflow Usage

**Before (v1.x)**:
```rust
use agentflow::{ConfigWorkflowRunner};

let runner = ConfigWorkflowRunner::from_file("workflow.yml").await?;
let result = runner.run(inputs).await?;
```

**After (v2.0)**:
```rust
use agentflow_config::{ConfigWorkflowRunner};

let runner = ConfigWorkflowRunner::from_file("workflow.yml").await?;
let result = runner.run(inputs).await?;
```

### Phase 2: Configuration Migration

#### 2.1 Configuration Format Updates

**Before (v1.x) - Basic YAML**:
```yaml
name: "Simple Workflow"
workflow:
  - name: "llm_node"
    type: "llm"
    model: "gpt-4"
    prompt: "Analyze: {{input}}"
    
inputs:
  input:
    type: "string"
    required: true
    
outputs:
  result:
    from: "llm_node.response"
```

**After (v2.0) - Enhanced Configuration**:
```yaml
version: "2.0"                     # Required version field
metadata:
  name: "Enhanced Analysis Workflow"
  description: "Advanced analysis with templates"
  author: "AgentFlow User"

shared:                            # Enhanced variable system
  user_input:
    type: string
    description: "Input text to analyze"
    required: true
  analysis_result:
    type: object
    description: "Structured analysis output"

templates:                         # New template system
  prompts:
    analysis_prompt: |
      Analyze the following text and provide insights:
      
      {{shared.user_input}}
      
      Return analysis in JSON format with:
      - sentiment: score from -1 to 1
      - topics: array of key topics
      - complexity: reading level 1-10

parameters:                        # Organized parameters
  analyzer:
    model: "gpt-4o"
    temperature: 0.7
    max_tokens: 1500

nodes:                            # Enhanced node definition
  - name: text_analyzer
    type: llm
    model: "{{parameters.analyzer.model}}"
    prompt: "{{templates.prompts.analysis_prompt}}"
    parameters: "{{parameters.analyzer}}"
    outputs:
      - target: shared.analysis_result
        format: json
      - target: final_output        # Workflow output
```

#### 2.2 Automated Configuration Migration

Use the migration tool to convert v1.x configurations:

```bash
# Install CLI with migration support
cargo install agentflow-cli

# Migrate single configuration file
agentflow config migrate old_workflow.yml --output new_workflow.yml --target-version 2.0

# Batch migrate directory
agentflow config migrate-dir ./workflows/ --output ./workflows_v2/ --target-version 2.0

# Interactive migration with guidance
agentflow config migrate old_workflow.yml --interactive

# Validate migrated configuration
agentflow config validate new_workflow.yml
```

#### 2.3 Manual Migration Checklist

- [ ] Add `version: "2.0"` to configuration header
- [ ] Wrap workflow metadata in `metadata` section
- [ ] Convert `inputs`/`outputs` to `shared` variables with types
- [ ] Move complex prompts to `templates.prompts` section
- [ ] Organize node parameters in `parameters` section
- [ ] Update node definitions with enhanced syntax
- [ ] Add dependency relationships with `depends_on`
- [ ] Validate configuration with `agentflow config validate`

### Phase 3: Advanced Features Migration

#### 3.1 Node Type Updates

**LLM Node Migration**:

**Before (v1.x)**:
```yaml
- name: "analyzer"
  type: "llm"
  model: "gpt-4"
  prompt: "Analyze: {{input}}"
  temperature: 0.7
  max_tokens: 1000
```

**After (v2.0)**:
```yaml
- name: analyzer
  type: llm
  model: "{{parameters.analyzer.model}}"
  prompt: "{{templates.prompts.analysis}}"
  system: "{{templates.prompts.system_context}}"  # New: system prompts
  parameters:
    temperature: 0.7
    max_tokens: 1000
    response_format: json                          # New: format control
  outputs:
    - target: shared.analysis_result
      format: json
      validate_schema: true                        # New: validation
```

**HTTP Node Migration**:

**Before (v1.x)**:
```yaml
- name: "api_call"
  type: "http"
  url: "https://api.example.com/analyze"
  method: "POST"
  body: "{{input}}"
```

**After (v2.0)**:
```yaml
- name: api_call
  type: http
  method: POST
  url: "https://api.example.com/analyze"
  headers:                                        # New: header support
    Authorization: "Bearer {{shared.api_token}}"
    Content-Type: "application/json"
  body: "{{shared.request_payload}}"
  timeout: 30000                                  # New: timeout control
  retry_count: 3                                  # New: retry logic
  outputs:
    - target: shared.api_response
```

#### 3.2 New Node Types

Take advantage of new node types in v2.0:

```yaml
nodes:
  # Conditional execution
  - name: quality_gate
    type: conditional
    condition: "{{shared.confidence_score > 0.8}}"
    on_true: "advanced_processing"
    on_false: "basic_processing"
    
  # Batch processing
  - name: batch_processor
    type: batch
    items: "{{shared.document_list}}"
    batch_size: 5
    max_concurrent: 3
    node_template:
      type: llm
      model: "gpt-4o"
      prompt: "Process: {{item.content}}"
      
  # Template rendering
  - name: report_generator
    type: template
    template: "{{templates.outputs.report}}"
    output_format: markdown
    
  # Parallel execution
  - name: parallel_analysis
    type: parallel
    nodes:
      - name: sentiment
        type: llm
        prompt: "Analyze sentiment: {{input}}"
      - name: topics
        type: llm  
        prompt: "Extract topics: {{input}}"
```

### Phase 4: Agent Application Migration

#### 4.1 Extracting Agent Applications

If you have complex workflows that should become reusable agents:

**Before (v1.x) - Embedded Logic**:
```rust
// Embedded in main application
async fn analyze_document(path: &Path) -> Result<AnalysisResult> {
    let content = tokio::fs::read_to_string(path).await?;
    
    let flow = AsyncFlow::new(/* ... */);
    let result = flow.run_async(&shared_state).await?;
    
    Ok(result)
}
```

**After (v2.0) - Agent Application**:
```rust
// agentflow-agents/src/document_analyzer.rs
use agentflow_agents::{AgentApplication, FileAgent, AgentResult};

pub struct DocumentAnalyzer {
    config: AnalyzerConfig,
}

#[async_trait]
impl AgentApplication for DocumentAnalyzer {
    type Config = AnalyzerConfig;
    type Result = AnalysisResult;
    
    async fn initialize(config: Self::Config) -> AgentResult<Self> {
        // Initialize agent with configuration
    }
    
    async fn execute(&self, input: &str) -> AgentResult<Self::Result> {
        // Core execution logic using agentflow-core
    }
}

#[async_trait] 
impl FileAgent for DocumentAnalyzer {
    async fn process_file<P: AsRef<Path>>(&self, file_path: P) -> AgentResult<Self::Result> {
        // File-specific processing logic
    }
    
    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["txt", "md", "pdf"]
    }
}
```

#### 4.2 Using Agent Applications

```rust
// Use the agent application in your code
use agentflow_agents::document_analyzer::DocumentAnalyzer;

let analyzer = DocumentAnalyzer::initialize(config).await?;
let result = analyzer.process_file("document.pdf").await?;
```

### Phase 5: CLI Integration

#### 5.1 New CLI Commands

**Configuration Workflows**:
```bash
# Execute configuration-based workflows
agentflow config run workflow.yml --input key=value

# Validate configuration files
agentflow config validate workflow.yml

# Interactive configuration builder
agentflow config create --interactive

# Convert v1 to v2 configuration
agentflow config migrate old.yml --output new.yml
```

**Direct LLM Commands**:
```bash
# Direct LLM interaction
agentflow llm prompt "Analyze this text" --model gpt-4o

# Interactive chat mode
agentflow llm chat --model claude-3-sonnet

# List available models
agentflow llm models

# Batch processing
agentflow llm batch --input-file prompts.txt --model gpt-4o
```

#### 5.2 Workflow Commands

```bash
# Code-first workflow execution (if using programmatic flows)
agentflow workflow run --class MyWorkflow --input data.json

# Agent application execution
agentflow agent run document_analyzer --input document.pdf

# Batch agent processing
agentflow agent batch document_analyzer --input-dir documents/
```

## Common Migration Issues

### Issue 1: Import Path Changes

**Problem**: `use agentflow::ConfigWorkflowRunner` not found

**Solution**: Update imports to use specific crates
```rust
// Change from:
use agentflow::ConfigWorkflowRunner;

// To:
use agentflow_config::ConfigWorkflowRunner;
```

### Issue 2: Configuration Format Validation

**Problem**: v1.x configuration fails validation

**Solution**: Use migration tool or manually update format
```bash
# Automatic migration
agentflow config migrate old_config.yml

# Manual validation with detailed errors
agentflow config validate old_config.yml --verbose
```

### Issue 3: Missing Features

**Problem**: Some v1.x features not available in v2.0

**Solution**: Check feature compatibility and alternatives
```bash
# Check what features are available in v2.0
agentflow --help

# Check specific node types
agentflow config node-types

# Report missing features
agentflow config validate --report-unsupported
```

### Issue 4: Performance Differences

**Problem**: Workflows run slower/faster after migration

**Solution**: Review and optimize configuration
```bash
# Profile workflow execution
agentflow config run workflow.yml --profile

# Enable detailed timing
agentflow config run workflow.yml --timing

# Optimize batch settings
agentflow config optimize workflow.yml
```

## Migration Timeline

### Immediate (Phase 1): Basic Migration
- [ ] Update dependencies in `Cargo.toml`
- [ ] Fix import statements
- [ ] Test basic functionality
- [ ] Update CI/CD scripts

### Short-term (Phase 2): Configuration Enhancement  
- [ ] Migrate configuration files to v2.0 format
- [ ] Add enhanced templates and parameters
- [ ] Test configuration workflows
- [ ] Update documentation

### Medium-term (Phase 3): Feature Adoption
- [ ] Adopt new node types (conditional, batch, parallel)
- [ ] Implement advanced templates
- [ ] Add comprehensive error handling
- [ ] Optimize performance

### Long-term (Phase 4): Architecture Benefits
- [ ] Extract reusable agent applications
- [ ] Implement advanced MCP integration
- [ ] Add custom provider support
- [ ] Build workflow libraries

## Rollback Strategy

If migration issues arise, you can rollback:

### Code Rollback
```toml
# Temporarily revert to v1.x
[dependencies]
agentflow = "0.1.9"  # Last v1.x version
```

### Configuration Rollback
```bash
# Keep v1.x configurations alongside v2.0
cp workflow.yml workflow_v1_backup.yml
agentflow-v1 run workflow_v1_backup.yml
```

### Gradual Migration
```rust
// Use both versions during transition
#[cfg(feature = "v1-compat")]
use agentflow_v1 as agentflow_legacy;

#[cfg(not(feature = "v1-compat"))]
use agentflow_config as agentflow_new;
```

## Support and Resources

### Migration Tools
- **CLI Migration**: `agentflow config migrate`  
- **Validation**: `agentflow config validate`
- **Interactive Setup**: `agentflow config create --interactive`

### Documentation
- **[Architecture Guide](ARCHITECTURE.md)**: Complete technical architecture
- **[Configuration Reference](CONFIGURATION.md)**: Full YAML configuration documentation
- **[Examples Directory](../examples/)**: Working examples for all features

### Community Support
- **GitHub Issues**: Report migration problems
- **Discussions**: Ask migration questions
- **Examples**: Community-contributed migration examples

---

This migration guide provides comprehensive instructions for upgrading to AgentFlow v2.0. For additional help, consult the [Architecture Documentation](ARCHITECTURE.md) or reach out via GitHub Issues.