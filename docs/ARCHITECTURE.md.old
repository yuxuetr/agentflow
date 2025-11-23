# AgentFlow Technical Architecture

**Version**: 2.0  
**Last Updated**: 2025-08-26  
**Status**: Active Development

## Executive Summary

AgentFlow is a modular, Rust-based workflow orchestration platform designed for AI agent applications. The architecture follows a layered approach with clear separation between code-first and configuration-first paradigms, enabling both technical and non-technical users to build sophisticated AI workflows.

## Core Design Principles

### 1. **Separation of Concerns**
- **Code-First**: Programmatic workflow construction for maximum flexibility
- **Configuration-First**: Declarative YAML/JSON workflows for ease of use
- **Clear Boundaries**: Each approach remains independent and first-class

### 2. **Modular Architecture**
- **Foundation Layer**: Core execution engine and shared utilities
- **Orchestration Layer**: Configuration parsing and compilation
- **Application Layer**: CLI, agents, and specialized integrations

### 3. **Unified LLM Integration**
- **agentflow-llm**: Centralized provider abstraction
- **Multi-Provider Support**: OpenAI, Anthropic, Google, StepFun, Moonshot
- **Future-Ready**: Designed for Ollama, vLLM, SGLang integration

## Crate Organization

```
agentflow/
├── agentflow-core/           # Foundation: Code-first workflow engine
├── agentflow-config/         # Orchestration: Configuration-first workflows  
├── agentflow-llm/           # Foundation: Unified LLM provider integration
├── agentflow-mcp/           # Application: Model Context Protocol support
├── agentflow-agents/        # Application: Reusable AI agent applications
├── agentflow-cli/           # Application: Command-line interface
└── docs/                    # Documentation and guides
```

## Dependency Architecture

### Layer 1: Foundation
**agentflow-core** - Pure code-first workflow execution engine
- `AsyncNode` trait: Core abstraction for workflow components
- `AsyncFlow`: Execution orchestrator with concurrency support  
- `SharedState`: Thread-safe state management across workflow
- Built-in node implementations (LLM, HTTP, File)
- Observability and robustness features

**agentflow-llm** - Unified LLM provider abstraction  
- Multi-provider support with consistent interface
- Model registry and discovery
- Streaming response handling
- Multimodal capabilities (text, image, audio)
- Configuration management and validation

### Layer 2: Orchestration
**agentflow-config** - Configuration-first workflow system
- YAML/JSON workflow parsing and validation
- Template engine with Handlebars syntax
- Node registry for built-in and custom types
- Configuration compiler (Config → Code compilation)
- Runtime execution environment

### Layer 3: Application  
**agentflow-cli** - Unified command-line interface
- Workflow commands: `run`, `validate`, `list`
- LLM commands: `prompt`, `chat`, `models`
- Configuration commands: `config run`, `config validate`
- Image/audio processing utilities

**agentflow-agents** - Reusable AI agent applications
- Complete agent implementations (e.g., PDF research analyzer)
- Shared utilities and common patterns
- Agent trait abstractions
- Batch processing capabilities

**agentflow-mcp** - Model Context Protocol integration
- MCP client implementation
- Tool calling abstractions  
- Transport layer support (stdio, HTTP)
- Future workflow integration

## Core Abstractions

### AsyncNode Trait
```rust
#[async_trait]
pub trait AsyncNode: Send + Sync {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value>;
    async fn exec_async(&self, prep_result: Value) -> Result<Value>;  
    async fn post_async(
        &self, 
        shared: &SharedState, 
        prep_result: Value, 
        exec_result: Value
    ) -> Result<Option<String>>;
}
```

**Three-Phase Execution**:
1. **Preparation**: Data collection and validation from shared state
2. **Execution**: Core processing logic (LLM calls, HTTP requests, etc.)
3. **Post-processing**: Result storage and next node determination

### AsyncFlow Orchestrator
```rust
pub struct AsyncFlow {
    pub id: Uuid,
    start_node: Option<Box<dyn AsyncNode>>,
    nodes: HashMap<String, Box<dyn AsyncNode>>,
    // Advanced execution options
    parallel_nodes: Vec<Box<dyn AsyncNode>>,
    timeout: Option<Duration>,
    metrics_collector: Option<Arc<MetricsCollector>>,
}
```

**Execution Modes**:
- **Sequential**: Traditional workflow execution
- **Parallel**: Concurrent node execution  
- **Batch**: Resource-aware batch processing
- **Nested**: Hierarchical workflow composition

### SharedState Management
```rust
pub struct SharedState {
    data: Arc<RwLock<HashMap<String, Value>>>,
}
```

**Features**:
- Thread-safe concurrent access
- JSON value storage with type conversion
- Workflow-scoped state isolation
- Template variable resolution

## Configuration-First Architecture

### Enhanced Configuration Format
```yaml
version: "2.0"
metadata:
  name: "Workflow Name"
  description: "Workflow description"

shared:
  variable_name:
    type: string
    description: "Variable description"  
    default: "default_value"

templates:
  prompts:
    template_name: "Template content with {{shared.variable_name}}"
  outputs:
    output_template: "Result: {{shared.result}}"

parameters:
  node_name:
    temperature: 0.8
    max_tokens: 1000

nodes:
  - name: node_name
    type: llm
    model: gpt-4o
    depends_on: [previous_node]
    condition: "{{shared.enable_feature}}"
    prompt: "{{templates.prompts.template_name}}"
    parameters: "{{parameters.node_name}}"
    outputs:
      - target: shared.result
        format: json
```

### Configuration Compilation Process
1. **Parsing**: YAML/JSON → Configuration objects
2. **Validation**: Schema validation and dependency checking
3. **Template Resolution**: Handlebars template compilation
4. **Node Creation**: Factory pattern instantiation
5. **Flow Assembly**: Dependency graph → AsyncFlow

## LLM Integration Architecture

### Provider Abstraction
```rust
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    async fn stream_complete(&self, request: CompletionRequest) -> Result<CompletionStream>;
    fn supports_multimodal(&self) -> bool;
    fn get_model_info(&self, model: &str) -> Option<ModelInfo>;
}
```

### Current Provider Support
- **OpenAI**: GPT models, DALL-E, Whisper
- **Anthropic**: Claude models  
- **Google**: Gemini models
- **StepFun**: Specialized Chinese models
- **Moonshot**: Kimi models

### Planned Provider Support  
- **Ollama**: Local model deployment
- **vLLM**: High-performance inference server
- **SGLang**: Structured generation language
- **Custom**: User-defined provider implementations

### Model Registry
```rust
pub struct ModelRegistry {
    providers: HashMap<String, Box<dyn LLMProvider>>,
    models: HashMap<String, ModelInfo>,
}

pub struct ModelInfo {
    pub provider: String,
    pub model_id: String,
    pub context_length: usize,
    pub supports_streaming: bool,
    pub supports_multimodal: bool,
    pub cost_per_token: Option<f64>,
}
```

## Concurrency and Performance

### Execution Patterns
1. **Sequential Execution**: Traditional workflow chains
2. **Parallel Execution**: Independent node concurrent processing
3. **Batch Processing**: Resource-aware batch operations with backpressure
4. **Pipeline Processing**: Streaming data through workflow stages

### Resource Management
- **Connection Pooling**: Shared HTTP clients across nodes
- **Rate Limiting**: Provider-specific request throttling  
- **Circuit Breakers**: Automatic failure recovery
- **Timeout Management**: Configurable execution timeouts

### Observability
```rust
pub struct MetricsCollector {
    events: Vec<ExecutionEvent>,
    counters: HashMap<String, f64>,
    histograms: HashMap<String, Vec<f64>>,
}

pub struct ExecutionEvent {
    pub node_id: String,
    pub event_type: String,
    pub timestamp: Instant,
    pub duration_ms: Option<u64>,
    pub metadata: HashMap<String, String>,
}
```

## Error Handling Strategy

### Error Type Hierarchy
```rust
#[derive(thiserror::Error, Debug)]
pub enum AgentFlowError {
    #[error("Async execution failed: {message}")]
    AsyncExecutionError { message: String },
    
    #[error("Flow execution failed: {message}")]
    FlowExecutionFailed { message: String },
    
    #[error("Timeout exceeded: {duration_ms}ms")]
    TimeoutExceeded { duration_ms: u64 },
    
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
    
    #[error("LLM provider error: {0}")]
    LLMError(#[from] agentflow_llm::LLMError),
}
```

### Recovery Patterns
- **Retry Logic**: Exponential backoff with jitter
- **Graceful Degradation**: Fallback to alternative providers
- **Partial Success**: Continue workflow with partial results
- **Error Propagation**: Structured error context preservation

## Security Considerations

### API Key Management
- Environment variable configuration
- Secure configuration file support  
- Runtime key rotation capability
- Provider-specific authentication patterns

### Input Validation
- Template injection prevention
- Input sanitization at workflow boundaries
- Schema validation for all configurations
- Safe execution sandboxing

### Data Privacy
- Configurable data retention policies
- Secure state management
- Provider data handling compliance
- User data anonymization options

## Extension Points

### Custom Node Development
```rust
pub struct CustomNode {
    // Custom node implementation
}

#[async_trait]
impl AsyncNode for CustomNode {
    // Implementation required
}

// Registration
let mut registry = NodeRegistry::new();
registry.register("custom_type", Box::new(CustomNodeFactory));
```

### Custom Provider Integration
```rust
pub struct CustomProvider {
    // Provider-specific implementation  
}

#[async_trait] 
impl LLMProvider for CustomProvider {
    // Provider interface implementation
}

// Registration  
AgentFlow::register_provider("custom", Box::new(CustomProvider::new()))?;
```

## Migration and Compatibility

### Version Compatibility
- **Semantic Versioning**: Major.Minor.Patch versioning scheme
- **API Stability**: Core traits maintain backward compatibility
- **Migration Tools**: Automated configuration migration utilities
- **Deprecation Policy**: 2-version deprecation cycle

### Configuration Migration
```bash
# Migrate v1.x configurations to v2.0
agentflow migrate --from config_v1.yml --to config_v2.yml

# Validate configuration compatibility
agentflow validate --config config.yml --target-version 2.0
```

## Performance Characteristics

### Benchmarks (Preliminary)
- **Cold Start**: < 100ms for simple workflows
- **Node Execution**: < 10ms overhead per node
- **Concurrent Nodes**: Linear scaling up to system limits
- **Memory Usage**: < 50MB base footprint

### Scalability Targets
- **Workflow Size**: 1000+ nodes per workflow
- **Concurrent Workflows**: 100+ simultaneous executions
- **Batch Processing**: 10,000+ items with backpressure control
- **Provider Requests**: Rate-limited per provider specifications

## Development Roadmap

### Phase 1: Architecture Separation (Q4 2024)
- [x] Extract agentflow-config from agentflow-core
- [x] Implement enhanced configuration format
- [x] Create configuration compiler
- [ ] Complete migration tooling

### Phase 2: LLM Expansion (Q1 2025)
- [ ] Ollama integration
- [ ] vLLM provider support
- [ ] SGLang structured generation
- [ ] Custom provider SDK

### Phase 3: Advanced Features (Q2 2025)
- [ ] Visual workflow builder
- [ ] A/B testing framework  
- [ ] Distributed execution
- [ ] WebAssembly node support

### Phase 4: Enterprise Features (Q3 2025)
- [ ] Role-based access control
- [ ] Audit logging
- [ ] Multi-tenant support
- [ ] Enterprise provider integrations

## Contributing Guidelines

### Code Organization
- **One Concern Per Crate**: Clear separation of responsibilities
- **Interface-First Design**: Define traits before implementations
- **Test-Driven Development**: Comprehensive test coverage required
- **Documentation**: All public APIs must be documented

### Quality Standards
- **Code Coverage**: > 80% for all crates
- **Performance Benchmarks**: No regression without justification
- **API Stability**: Breaking changes require RFC process
- **Security Review**: All provider integrations security-reviewed

---

**Maintainers**: AgentFlow Core Team  
**License**: MIT  
**Repository**: https://github.com/agentflow/agentflow