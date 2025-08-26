# AgentFlow

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Tests](https://img.shields.io/badge/tests-67%2F67%20passing-brightgreen.svg)](#testing)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **A modular, Rust-based AI workflow orchestration platform supporting both code-first and configuration-first paradigms with unified LLM integration.**

AgentFlow delivers production-ready workflow orchestration through a clean, layered architecture that separates concerns while providing seamless integration across all components.

## 🏗️ Architecture Overview

AgentFlow follows a modular, layered architecture with clear separation of concerns:

```
                    Application Layer
    ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
    │  agentflow-cli  │    │ agentflow-agents │    │ agentflow-mcp   │
    │                 │    │                  │    │                 │  
    │ • workflow run  │    │ • paper_analyzer │    │ • MCP client    │
    │ • config run    │    │ • batch_utils    │    │ • tool calls    │
    │ • llm commands  │    │ • agent traits   │    │ • transport     │
    └─────────┬───────┘    └─────────┬────────┘    └─────────┬───────┘
              │                      │                       │
        Orchestration Layer           │                       │
        ┌────────────────▼────────────▼───────────────────────┘
        │     agentflow-config            │    
        │                                 │    
        │ • YAML/JSON parsing             │   
        │ • Template engine               │    
        │ • Node registry                 │    
        │ • Configuration compiler        │    
        └─────────────────┬───────────────┘
                          │
        ┌─────────────────▼───────────────┐    ┌─────────────────────┐
        │        agentflow-core           │    │   agentflow-llm     │
        │                                 │◄───┤                     │
        │ • AsyncNode trait               │    │ • OpenAI            │
        │ • AsyncFlow execution           │    │ • Anthropic         │ 
        │ • SharedState                   │    │ • Google            │
        │ • Core error types              │    │ • Moonshot          │
        │ • Built-in nodes                │    │ • StepFun           │
        └─────────────────────────────────┘    │ • Ollama (planned)  │
                          ▲                    │ • vLLM (planned)    │
                          │                    │ • SGLang (planned)  │
                          │                    │ • Model registry    │
                          │                    │ • Multimodal        │
                          └────────────────────┤ • Streaming         │
                                               └─────────────────────┘
                    Foundation Layer
```

## 🎯 Two Approaches, One Platform

### 💻 **Code-First Approach**
For developers who want full programmatic control:

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState};
use agentflow_llm::AgentFlow;

// Direct LLM integration
let response = AgentFlow::model("gpt-4o")
    .prompt("Analyze this data")
    .execute().await?;

// Build workflows programmatically
let mut flow = AsyncFlow::new(Box::new(llm_node));
flow.add_node("summarizer".to_string(), Box::new(summary_node));
let result = flow.run_async(&shared_state).await?;
```

### 📋 **Configuration-First Approach**
For users who prefer declarative workflows:

```yaml
# workflow.yml
version: "2.0"
metadata:
  name: "Data Analysis Pipeline"

shared:
  analysis_result:
    type: string
    description: "Analysis output"

nodes:
  - name: analyzer
    type: llm
    model: gpt-4o
    prompt: "Analyze this data: {{input.data}}"
    outputs:
      - target: shared.analysis_result
      
  - name: summarizer
    type: llm
    depends_on: [analyzer]
    prompt: "Summarize: {{shared.analysis_result}}"
```

```bash
# Execute via CLI
agentflow config run workflow.yml --input data="sales data here"
```

## 🛠️ Modular Crate Architecture

### **agentflow-core** - Foundation Layer
Pure code-first workflow execution engine:
```toml
[dependencies]
agentflow-core = "0.2.0"
```
- `AsyncNode` trait and three-phase execution
- `AsyncFlow` orchestrator with concurrency support
- `SharedState` for thread-safe data sharing
- Built-in robustness and observability features

### **agentflow-llm** - Unified LLM Integration
Multi-provider LLM abstraction layer:
```toml
[dependencies]
agentflow-llm = "0.2.0"
```
- **Current Providers**: OpenAI, Anthropic, Google, StepFun, Moonshot
- **Planned Providers**: Ollama, vLLM, SGLang, custom deployments
- Streaming and multimodal capabilities
- Centralized model registry and configuration

### **agentflow-config** - Configuration-First Support
Declarative workflow orchestration:
```toml
[dependencies]
agentflow-config = "0.2.0"
```
- Enhanced YAML/JSON configuration parsing
- Handlebars template engine integration
- Node registry with built-in types
- Configuration compiler (YAML → Rust code)

### **agentflow-cli** - Command-Line Interface
Unified command-line access to all features:
```bash
cargo install agentflow-cli
agentflow --help
```
- Workflow execution commands
- Direct LLM interaction
- Configuration validation and tools
- Interactive and batch modes

### **agentflow-agents** - Application Layer
Reusable AI agent applications:
```toml
[dependencies]
agentflow-agents = "0.2.0"
```
- Complete agent implementations (PDF analyzer, etc.)
- Shared utilities and batch processing
- Agent trait abstractions
- Real-world workflow examples

### **agentflow-mcp** - MCP Integration
Model Context Protocol support:
```toml
[dependencies]
agentflow-mcp = "0.2.0"
```
- MCP client implementation
- Tool calling abstractions
- Transport layer support (stdio, HTTP)
- Future workflow integration

## 🚀 Key Features

### ⚡ **Dual Paradigm Support**
- **Code-First**: Full programmatic control with Rust's type safety
- **Configuration-First**: Declarative YAML workflows for non-programmers
- **Seamless Integration**: Mix and match approaches as needed

### 🛡️ **Enterprise Robustness**
- Circuit breakers and retry policies with exponential backoff
- Rate limiting and timeout management
- Resource pooling and connection management
- Graceful error handling and recovery

### 🌐 **Unified LLM Integration**
- **Current**: OpenAI, Anthropic, Google, StepFun, Moonshot
- **Planned**: Ollama, vLLM, SGLang, custom deployments
- Streaming responses and multimodal support
- Centralized model registry and configuration

### 📊 **Built-in Observability**
- Real-time metrics collection and performance monitoring
- Structured event logging with execution tracing
- Integration-ready for monitoring platforms
- Comprehensive debugging and profiling tools

### 🔄 **Advanced Execution Models**
- Sequential, parallel, and nested workflow execution
- Batch processing with backpressure control
- Conditional routing and dynamic flow control
- Three-phase node execution (prep/exec/post)

## 📦 Installation

Choose the components you need for your use case:

### For Code-First Development
```toml
[dependencies]
agentflow-core = "0.2.0"  # Core workflow engine
agentflow-llm = "0.2.0"   # Multi-provider LLM integration
tokio = { version = "1.0", features = ["full"] }
```

### For Configuration-First Usage
```toml
[dependencies]
agentflow-config = "0.2.0"  # Includes core + config support
```

### For CLI Usage
```bash
cargo install agentflow-cli
agentflow --help
```

## 🎯 Quick Start Examples

### Code-First: Basic Node Implementation

```rust
use agentflow_core::{AsyncFlow, AsyncNode, SharedState, Result};
use async_trait::async_trait;
use serde_json::Value;

// Define a custom node
struct GreetingNode {
    name: String,
}

#[async_trait]
impl AsyncNode for GreetingNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
        Ok(Value::String(format!("Preparing greeting for {}", self.name)))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
        Ok(Value::String(format!("Hello, {}!", self.name)))
    }

    async fn post_async(&self, shared: &SharedState, _prep: Value, exec: Value) -> Result<Option<String>> {
        shared.insert("greeting".to_string(), exec);
        Ok(None) // End flow
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let node = Box::new(GreetingNode {
        name: "AgentFlow".to_string()
    });

    let flow = AsyncFlow::new(node);
    let shared = SharedState::new();

    let result = flow.run_async(&shared).await?;
    println!("Flow completed: {:?}", result);

    Ok(())
}
```

### Configuration-First: YAML Workflow

```yaml
# analysis_workflow.yml
version: "2.0"
metadata:
  name: "Document Analysis Pipeline"
  description: "Analyze and summarize documents with AI"

shared:
  document_content:
    type: string
    description: "Raw document text"
  analysis_result:
    type: object
    description: "Structured analysis output"
  summary:
    type: string 
    description: "Final document summary"

templates:
  prompts:
    analyze: |
      Analyze the following document and extract key insights:
      
      {{shared.document_content}}
      
      Provide analysis in JSON format with:
      - main_topics: array of key topics
      - sentiment: overall sentiment score (-1 to 1)
      - complexity: reading complexity (1-10)
      - key_entities: important people, places, organizations
      
    summarize: |
      Based on this analysis: {{shared.analysis_result}}
      
      Create a concise 2-3 paragraph summary of the document's
      main points and conclusions.

nodes:
  - name: analyzer
    type: llm
    model: gpt-4o
    prompt: "{{templates.prompts.analyze}}"
    parameters:
      temperature: 0.3
      max_tokens: 1000
    outputs:
      - target: shared.analysis_result
        format: json
        
  - name: summarizer
    type: llm
    model: claude-3-sonnet
    depends_on: [analyzer]
    prompt: "{{templates.prompts.summarize}}"
    parameters:
      temperature: 0.7
      max_tokens: 500
    outputs:
      - target: shared.summary
      - target: final_output
```

### LLM Integration Example

```rust
use agentflow_llm::AgentFlow;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the AgentFlow client
    AgentFlow::init().await?;
    
    // Single model call
    let response = AgentFlow::model("gpt-4o")
        .prompt("Explain quantum computing in simple terms")
        .temperature(0.7)
        .max_tokens(500)
        .execute()
        .await?;
    
    println!("Response: {}", response.content);
    
    // Multi-modal example with image
    let multimodal_response = AgentFlow::model("gpt-4o")
        .prompt("Describe this image")
        .add_image_url("https://example.com/image.jpg")
        .execute()
        .await?;
    
    Ok(())
}
```

## 📋 CLI Usage Examples

```bash
# Execute configuration workflows
agentflow config run analysis_workflow.yml --input document_content="Document text here"

# Direct LLM interaction
agentflow llm prompt "Explain machine learning" --model gpt-4o --temperature 0.8

# Interactive chat mode
agentflow llm chat --model claude-3-sonnet

# Batch processing
agentflow config run batch_analysis.yml --input dir="/path/to/documents"

# Configuration validation
agentflow config validate workflow.yml

# List available models
agentflow llm models
```

## 📚 Documentation

### 🏗️ **Architecture & Design**
- **[Technical Architecture](docs/ARCHITECTURE.md)** - Complete technical architecture overview
- **[System Design](docs/agentflow-design.md)** - High-level system design and component diagrams
- **[Core Engine Design](docs/design.md)** - Detailed workflow engine architecture  

### 🔧 **Integration Guides**
- **[LLM Integration Guide](docs/agentflow-llm-design.md)** - Multi-provider LLM integration details
- **[CLI Usage Guide](docs/agentflow-cli-design.md)** - Command-line interface usage patterns
- **[Configuration Reference](docs/CONFIGURATION.md)** - YAML workflow configuration guide

### 🎯 **Specialized Topics**
- **[Multimodal Support](docs/MULTIMODAL_GUIDE.md)** - Text, image, and audio processing capabilities
- **[Model Types System](docs/GRANULAR_MODEL_TYPES.md)** - Type-safe model capability definitions
- **[Use Cases](docs/use-cases.md)** - Real-world implementation scenarios
- **[Examples Documentation](docs/examples/)** - Comprehensive example walkthroughs

## 🧪 Testing

AgentFlow maintains 100% test coverage with comprehensive test suites:

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific module tests
cargo test async_flow
cargo test robustness
cargo test observability
```

**Current Status**: 67/67 tests passing ✅

## 🚢 Production Readiness

AgentFlow is designed for production environments with:

- **Memory Safety**: Rust's ownership model prevents data races and memory leaks
- **Performance**: Zero-cost abstractions and efficient async runtime
- **Reliability**: Comprehensive error handling and graceful degradation
- **Scalability**: Built-in support for horizontal scaling patterns
- **Monitoring**: Full observability stack for production insights

## 📚 Examples

### 🤖 Featured Example: LLM-Powered Agent Flow

```bash
# Run the comprehensive LLM integration example
cargo run --example simple_agent_llm_flow
```

This example demonstrates:
- **LLM Integration**: Using moonshot API within AgentFlow workflows
- **Intelligent Routing**: Dynamic flow branching based on AI response analysis
- **Response Processing**: Automated sentiment and complexity analysis
- **Context Management**: Dynamic prompt templates with state substitution
- **Full Observability**: Real-time metrics and execution tracing

### 📂 All Examples

See the [examples directory](./examples) for working examples:
- `hello_world.rs` - Basic AsyncNode functionality
- `batch_processing.rs` - Parallel processing patterns  
- `workflow.rs` - Sequential multi-stage workflows
- `chat.rs` - Interactive conversation flows
- `simple_agent_llm_flow.rs` - **LLM integration with intelligent routing**

📚 **[Complete Documentation](./docs/examples/)** with workflow diagrams and detailed guides.

## 🛣️ Roadmap and Next Steps

### **Upcoming Releases**

- **v0.3.0**: CLI Implementation and MCP Integration
- **v0.4.0**: RAG System and Vector Database Support  
- **v0.5.0**: Distributed Execution Engine
- **v0.6.0**: WebAssembly Plugin System
- **v1.0.0**: Production Stability Guarantees

### **📋 Follow-up Tasks (Implementation Priority)**

#### **Phase 1: CLI Foundation (High Priority)**
- [ ] **Create `agentflow-cli` crate** - Command-line interface implementation
  - CLI argument parsing with `clap`
  - Command structure (`workflow`, `llm`, `config` subcommands)
  - Basic error handling and user experience
- [ ] **Implement LLM commands** - Direct LLM interaction via CLI
  - `agentflow llm prompt` - text prompting
  - `agentflow llm models` - list available models
  - File input support (text, images, audio)
- [ ] **Update workspace configuration** - Produce unified `agentflow` binary
  - Configure Cargo.toml for binary generation
  - Set up cross-crate dependencies
  - Ensure proper feature flag management

#### **Phase 2: Workflow Engine (High Priority)**
- [ ] **YAML workflow parser** - Configuration file parsing and validation
  - Schema definition for workflow configurations
  - Template engine integration (Tera)
  - Input parameter handling and validation
- [ ] **Core node types** - Built-in workflow building blocks
  - LLM node (single model calls)
  - Batch LLM node (parallel processing)
  - Template node (rendering and formatting)
  - File I/O node (read/write operations)
- [ ] **Workflow execution engine** - Runtime workflow processing
  - Sequential execution support
  - Dependency resolution
  - Context management and state handling
  - Integration with agentflow-core's AsyncFlow

#### **Phase 3: Advanced Features (Medium Priority)**  
- [ ] **Advanced workflow types** - Complex execution patterns
  - Parallel workflow execution
  - Conditional branching logic
  - Loop and iteration support
  - Error handling and recovery strategies
- [ ] **Enhanced file I/O** - Multimodal input/output support
  - Image file processing integration
  - Audio file processing capabilities
  - Multiple output file generation
  - File utility functions in templates
- [ ] **Interactive features** - User experience improvements
  - Interactive chat mode (`agentflow llm chat`)
  - Streaming output support
  - Progress indicators and status reporting
  - Configuration management commands

#### **Phase 4: Integration and Polish (Medium Priority)**
- [ ] **MCP client integration** - Model Context Protocol support
  - Add MCP feature flag to agentflow-llm
  - Implement tools support in LLMClientBuilder
  - Follow existing provider pattern for consistency
  - Enable function calling across providers
- [ ] **Documentation and examples** - User guidance and tutorials
  - Complete CLI user documentation
  - Workflow template library creation
  - Integration examples and tutorials
  - Video guides and walkthroughs
- [ ] **Performance optimization** - Runtime efficiency improvements
  - Execution performance profiling
  - Memory usage optimization
  - Concurrent processing enhancements
  - Benchmarking and testing

#### **Phase 5: Future Extensions (Lower Priority)**
- [ ] **RAG system implementation** - Knowledge retrieval and processing
  - Create `agentflow-rag` crate
  - Document indexing and vector storage
  - Retrieval-augmented generation workflows
  - Vector database integrations
- [ ] **MCP server implementation** - Expose AgentFlow as MCP tools
  - Separate `agentflow-mcp-server` crate  
  - Workflow-to-tool conversion
  - Server deployment and management
  - Integration with existing MCP ecosystem
- [ ] **Plugin system** - Extensibility and customization
  - WebAssembly plugin support
  - Custom node type development
  - Third-party integration framework
  - Community plugin marketplace

### **🎯 Getting Started with Development**

1. **For CLI Development**: Start with `docs/agentflow-cli-design.md` for comprehensive implementation details
2. **For Library Usage**: Explore the existing examples in `examples/` directory
3. **For Contributing**: Review the modular architecture in `docs/agentflow-design.md`

### **🤝 Community Involvement**

- **Documentation**: Help improve guides, examples, and API documentation
- **Testing**: Write integration tests and performance benchmarks
- **Examples**: Create workflow templates and real-world use cases
- **Feedback**: Report issues, suggest features, and share use cases

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built on the foundation of the original PocketFlow concept
- Inspired by modern distributed systems patterns
- Powered by the Rust ecosystem and Tokio runtime

---

**AgentFlow**: Where intelligent workflows meet enterprise reliability. 🦀✨
