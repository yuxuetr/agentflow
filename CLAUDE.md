# AgentFlow Project - Claude Code Configuration

## Project Overview

AgentFlow is a Rust-based workflow orchestration platform with comprehensive LLM integration and Model Context Protocol (MCP) support. The project consists of three main crates with clear separation of concerns:

- **agentflow-core**: Workflow execution engine and core abstractions
- **agentflow-llm**: Unified LLM provider interface and multimodal capabilities  
- **agentflow-cli**: Command-line interface for workflow and LLM operations

## Architecture Principles

### High Cohesion, Low Coupling
- Each crate has clearly defined responsibilities
- Minimal cross-crate dependencies
- Well-defined public APIs between components
- MCP integration as separate concern (future `agentflow-mcp` crate)

### Crate Responsibilities

#### agentflow-core
**Primary Focus**: Workflow execution engine and orchestration
- Core workflow execution engine (`workflow_runner.rs`)
- Async node execution framework with concurrency control
- Configuration-first workflow management (`config.rs`)
- Robustness features: circuit breakers, rate limiting, timeout management
- Observability infrastructure: metrics collection, event tracking
- Shared state management across workflow execution
- Base abstractions for nodes and flows (`node.rs`, `async_node.rs`, `flow.rs`)

#### agentflow-llm  
**Primary Focus**: LLM provider abstraction and multimodal capabilities
- Unified LLM provider interface (OpenAI, Anthropic, Google, Moonshot, StepFun)
- Model registry and configuration management (`registry/`, `config/`)
- Multimodal capabilities (text, image, audio processing) (`multimodal.rs`)
- Streaming response handling (`client/streaming.rs`)
- API key management and authentication
- Model discovery and validation (`discovery/`)
- Provider-specific implementations (`providers/`)

#### agentflow-cli
**Primary Focus**: User interface and command orchestration
- Command-line interface and user interaction (`main.rs`)
- Workflow orchestration commands (run, validate, list)
- Direct LLM interaction commands (prompt, chat, models)
- Image generation and analysis commands (`commands/image/`)
- Audio processing commands (TTS, ASR, voice cloning) (`commands/audio/`)
- Configuration management (`commands/config/`)
- Output formatting and progress reporting (`utils/output.rs`)

## Development Guidelines

### Code Style and Standards
- **Indentation**: 2 spaces (NO TABS) - overrides Rust default
- Follow Rust idioms: ownership, borrowing, error handling with `Result<T, E>`
- Use snake_case for functions/variables, PascalCase for types
- Explicit error handling with custom error types (`error.rs` in each crate)
- Comprehensive documentation with `///` comments
- Use `async/await` with Tokio runtime

### Testing Strategy
- Unit tests in each module (`#[cfg(test)]`)
- Integration tests in `tests/` directories
- Example-driven development with `examples/` directories
- CLI tests with `assert_cmd` crate in `agentflow-cli`

### Error Handling Patterns
```rust
// Each crate defines its own error types
pub enum AgentFlowError {
    ConfigurationError { message: String },
    ExecutionError { message: String },
    ValidationError(String),
    // ...
}

// Use thiserror for implementation
#[derive(thiserror::Error, Debug)]
pub enum LLMError {
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
    // ...
}
```

### Configuration Management
- YAML-based configuration for workflows and models
- Environment variable support with `.env` files
- Hierarchical config: project → user → built-in defaults
- Runtime configuration validation

## Current Implementation Status

### ✅ Completed & Production-Ready Features
- **Core workflow execution engine** - Async/await with Tokio, DAG execution, state management
- **Control flow nodes** - Map (sequential/parallel), While loops, Conditional execution
- **LLM provider integration** - 5 providers (OpenAI, Anthropic, Google, Moonshot, StepFun)
- **Multimodal support** - Text, image (generation/understanding), audio (TTS/ASR)
- **Built-in nodes** - 16+ node types including HTTP, File, Batch, Template (Tera), MarkMap, Arxiv
- **CLI interface** - Workflow execution, LLM chat, image generation/understanding, audio processing
- **Configuration management** - YAML workflows, model registry, API key management
- **Testing** - Integration tests for core workflow features

### ✅ MCP Client Implementation (Production-Ready)
- **MCP client** - Full-featured MCP client with comprehensive testing ✅
  - Status: `agentflow-mcp` crate complete and production-ready
  - Features: JSON-RPC 2.0, retry mechanism, timeout handling, stdio transport
  - Testing: 162 tests, 100% pass rate (includes property-based testing)
  - Documentation: Complete (TESTING.md, integration reports)
  - **Ready for integration**: Can be integrated into agentflow-nodes immediately
  - Next step: Create MCPNode and CLI integration
- **Voice cloning** - CLI command hidden, requires file upload API

### 📋 Planned Features (Not Started)
- **RAG system** - No implementation yet
  - Requires: New `agentflow-rag` crate
  - Vector store abstractions (Pinecone, Weaviate, Qdrant, Chroma)
  - Embedding generation and document retrieval
  - RAGNode for workflow integration
- **Hybrid context strategies** - Depends on MCP + RAG completion
  - RAGFirst, MCPFirst, Smart routing strategies
  - Auto-discovery of tools and knowledge sources
- **Distributed execution** - Multi-node workflow execution
- **WebAssembly (WASM) nodes** - Sandboxed node execution
- **Enhanced robustness** - Circuit breakers in production use, advanced retry logic

## Implementation Roadmap

### Phase 1: Stabilization & Refinement ✅ COMPLETE (v0.2.0)
**Priority**: Fix existing issues, improve reliability, enhance developer experience

- ✅ **Fix compilation errors** - MCPToolNode V2 API migration
- ✅ **Voice cloning handling** - Hide unimplemented features
- ✅ **Documentation accuracy** - Update CLAUDE.md to reflect reality
- ✅ **Testing improvements** - Added comprehensive test suite
- ✅ **Error handling** - Retry mechanisms, better error context
- ✅ **Workflow debugging** - Debug command, DAG visualization
- ✅ **Resource management** - Memory limits, state cleanup

**Deliverable**: ✅ Stable v0.2.0 released with improved reliability and developer experience

### Phase 2: RAG System Implementation (3-6 months)
**Priority**: Enable knowledge-augmented workflows

- **agentflow-rag crate** - Core RAG infrastructure
  - Vector store trait and abstractions
  - Initial integration: Qdrant or Chroma (local-first)
  - Document chunking and embedding generation
  - Semantic search and retrieval
- **RAGNode** - Workflow integration
  - Configuration-first RAG queries
  - Similarity search with filtering
  - Result reranking
- **LLM-RAG integration** - Context injection patterns
- **CLI commands** - RAG index management, search testing
- **Examples** - Documentation assistant, knowledge Q&A

**Deliverable**: v0.3.0 with production-ready RAG capabilities

### Phase 3: MCP Integration ✅ CLIENT COMPLETE (Ahead of Schedule!)
**Priority**: Enable dynamic tool execution

**Status**: ✅ MCP Client complete, ready for AgentFlow integration (1-2 weeks)

- ✅ **agentflow-mcp client** - Production-ready MCP client
  - Full stdio transport layer ✅
  - JSON-RPC 2.0 protocol ✅
  - Retry + timeout mechanisms ✅
  - 162 tests, 100% pass rate ✅
  - Property-based testing ✅
- 🔄 **MCPNode integration** - Workflow integration (1-2 weeks)
  - Create MCPNode in agentflow-nodes
  - Add to node factory
  - Write workflow examples
- 🔄 **CLI integration** - MCP commands (1 week)
  - `agentflow mcp list-tools`
  - `agentflow mcp call-tool`
  - `agentflow mcp list-resources`
- 📋 **Future enhancements**
  - HTTP/SSE transport support
  - MCP server (expose workflows as tools)
  - Server-initiated notifications

**Deliverable**: v0.3.0 with MCP client + workflow integration

### Phase 4: Advanced Features (9-12 months)
**Priority**: Enterprise capabilities and scaling

- **Hybrid context strategies** - RAG + MCP smart routing
- **Distributed execution** - Multi-node workflow orchestration
- **WASM node support** - Sandboxed custom logic
- **Enhanced observability** - Metrics, tracing, profiling
- **Web UI** - Visual workflow editor and monitoring
- **Enterprise features** - RBAC, audit logs, workflow versioning

**Deliverable**: v1.0.0 - Production-ready enterprise platform

### Decision Gates
- **Before Phase 2**: Validate Phase 1 stability with community feedback
- **Before Phase 3**: Confirm official Rust MCP SDK availability
- **Before Phase 4**: Assess user demand for enterprise features

## Future Architecture Vision

### Context Integration Architecture (Planned)

> **Note**: This section describes planned architecture for future releases.
> Current implementation focuses on core workflow orchestration and LLM integration.

#### MCP vs RAG: Complementary Context Sources

**MCP (Model Context Protocol)**: Real-time, dynamic context (Planned)
- Live API calls and tool executions
- Real-time data retrieval
- Interactive system operations
- Dynamic tool discovery

**RAG (Retrieval-Augmented Generation)**: Static knowledge context (Planned)
- Pre-indexed knowledge bases
- Historical data and learned patterns
- Domain-specific documentation
- Semantic similarity-based retrieval

### MCP Integration Architecture (Planned)

> **Current Status**: Basic framework exists in `agentflow-mcp` crate.
> Full implementation pending official Rust MCP SDK release.

#### Proposed agentflow-mcp Crate Structure
```rust
// agentflow-mcp/src/lib.rs
pub mod client;     // MCP client implementation  
pub mod server;     // MCP server for exposing AgentFlow
pub mod transport;  // JSON-RPC transport (stdio, http)
pub mod schema;     // MCP protocol types and validation
pub mod tools;      // Tool calling integration
pub mod resources;  // Resource access patterns
pub mod prompts;    // Prompt template management
```

### MCPNode Integration
```rust
// agentflow-core/src/nodes/mcp.rs
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MCPNode {
    pub name: String,
    pub server_uri: String,
    pub tool_name: String,
    pub parameters: Option<serde_json::Value>,
}

#[async_trait::async_trait]
impl AsyncNode for MCPNode {
    async fn execute(&mut self, context: &mut ExecutionContext) -> Result<serde_json::Value>;
}
```

### LLM Node Context Discovery
```rust
// Enhanced LLM Node with auto-discovery of both MCP and RAG
pub struct LLMNode {
    // ... existing fields
    pub discover_mcp_tools: Option<bool>,
    pub mcp_tool_filter: Option<Vec<String>>,
    pub enable_rag: Option<bool>,
    pub rag_sources: Option<Vec<String>>,
    pub context_strategy: Option<ContextStrategy>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub enum ContextStrategy {
    RAGOnly,           // Use only static knowledge
    MCPOnly,           // Use only real-time tools
    RAGFirst,          // Try RAG first, then MCP if needed
    MCPFirst,          // Try MCP first, then RAG if needed  
    Hybrid,            // Use both simultaneously
    Smart,             // LLM decides which to use based on query
}
```

### RAG Integration Architecture (Planned)

> **Current Status**: Not started. Planned for future release.

#### Proposed agentflow-rag Crate Structure
```rust
// agentflow-rag/src/lib.rs
pub mod embeddings;    // Text embedding generation
pub mod vectorstore;   // Vector database abstractions
pub mod retrieval;     // Document retrieval strategies
pub mod indexing;      // Document processing and indexing
pub mod chunking;      // Text chunking strategies
pub mod reranking;     // Result reranking and filtering
pub mod sources;       // Data source connectors
```

### RAGNode Implementation
```rust
// agentflow-core/src/nodes/rag.rs
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RAGNode {
    pub name: String,
    pub vectorstore_uri: String,
    pub collection: String,
    pub query: String,
    pub top_k: Option<usize>,
    pub similarity_threshold: Option<f32>,
    pub rerank: Option<bool>,
}

#[async_trait::async_trait]
impl AsyncNode for RAGNode {
    async fn execute(&mut self, context: &mut ExecutionContext) -> Result<serde_json::Value> {
        let client = VectorStoreClient::connect(&self.vectorstore_uri).await?;
        let query = self.resolve_query(context)?;
        
        let results = client
            .similarity_search(&self.collection, &query, self.top_k.unwrap_or(5))
            .await?;
            
        let filtered_results = if let Some(threshold) = self.similarity_threshold {
            results.into_iter()
                .filter(|r| r.score >= threshold)
                .collect()
        } else {
            results
        };
        
        context.set_result(&self.name, &filtered_results)?;
        Ok(serde_json::to_value(filtered_results)?)
    }
}
```

### Vector Store Abstractions
```rust
// agentflow-rag/src/vectorstore.rs
#[async_trait::async_trait]
pub trait VectorStore {
    async fn similarity_search(
        &self, 
        collection: &str, 
        query: &str, 
        top_k: usize
    ) -> Result<Vec<SearchResult>>;
    
    async fn add_documents(&self, collection: &str, docs: Vec<Document>) -> Result<()>;
    async fn delete_collection(&self, collection: &str) -> Result<()>;
}

// Support multiple vector databases
pub struct PineconeStore { /* ... */ }
pub struct WeaviateStore { /* ... */ }
pub struct QdrantStore { /* ... */ }
pub struct ChromaStore { /* ... */ }
```

## File Organization

### Key Configuration Files
- `Cargo.toml` - Workspace configuration
- `agentflow-cli/examples/workflows/` - Example workflow definitions
- `agentflow-llm/config/models/` - LLM provider configurations
- `agentflow-llm/templates/` - Default configuration templates

### Important Source Files
- `agentflow-core/src/lib.rs` - Core exports and module organization
- `agentflow-llm/src/lib.rs` - LLM API entry point and fluent interface
- `agentflow-cli/src/main.rs` - CLI command structure and routing

### Example and Template Directories
- `agentflow-cli/examples/` - Comprehensive CLI usage examples
- `agentflow-llm/examples/` - LLM integration examples
- `agentflow-core/examples/` - Core workflow examples

## Working Workflow Examples

See `agentflow-cli/examples/` and `agentflow-cli/templates/` for production-ready workflow examples:

- **ai_research_assistant.yml** - Complex multi-step research workflow with:
  - Arxiv paper search with while loops
  - LLM summarization and translation
  - MarkMap visualization
  - Conditional language processing

- **Template workflows** - 11+ examples demonstrating:
  - Simple single-node and multi-step LLM chains
  - Map nodes (sequential and parallel)
  - While loops with state management
  - Tera template features (loops, conditionals, filters)
  - Complex report generation

> **Note**: Examples for MCP/RAG hybrid workflows will be added once these features are implemented.

## Common Development Tasks

### Adding New LLM Provider (Production-Ready)
1. Create provider module in `agentflow-llm/src/providers/`
2. Implement provider trait with authentication and API calls
3. Add configuration in `agentflow-llm/config/models/`
4. Update model registry in `agentflow-llm/src/registry/`
5. Add examples and tests

### Adding New Node Type (Production-Ready)
1. Create node module in `agentflow-nodes/src/nodes/`
2. Implement `AsyncNode` trait from `agentflow-core`
3. Add to factory in `agentflow-nodes/src/factories/`
4. Add configuration parsing and validation
5. Create examples and tests
6. Update documentation

### Adding New CLI Command (Production-Ready)
1. Define command structure in `agentflow-cli/src/main.rs`
2. Implement command handler in appropriate `commands/` module
3. Add output formatting and error handling
4. Create examples and documentation

## Quality Standards

### Code Quality Checklist
- [ ] All public APIs documented with `///` comments
- [ ] Error handling with appropriate error types
- [ ] Unit tests for core functionality
- [ ] Integration tests for CLI commands
- [ ] Examples demonstrating usage
- [ ] Configuration validation
- [ ] Logging and observability support

### Pre-Commit Requirements
- [ ] `cargo fmt` - Code formatting
- [ ] `cargo clippy` - Lint checks
- [ ] `cargo test` - All tests passing
- [ ] `cargo doc` - Documentation builds
- [ ] Example workflows validate successfully

## Security Considerations

### API Key Management
- Never commit API keys to repository
- Use environment variables or secure config files
- Support multiple configuration precedence levels
- Mask sensitive data in logs and error messages

### Input Validation
- Validate all user inputs (prompts, file paths, URLs)
- Sanitize template inputs to prevent injection
- Validate workflow configurations before execution
- Secure MCP transport connections

## Performance Guidelines

### Async Best Practices
- Use `tokio::spawn` for concurrent operations
- Implement proper backpressure handling
- Use streaming for large responses
- Connection pooling for HTTP clients

### Memory Management
- Avoid holding large responses in memory
- Use streaming parsers for large files
- Implement proper cleanup in Drop traits
- Monitor memory usage in long-running workflows

## Deployment and Distribution

### Binary Distribution
- Cross-platform compilation targets
- Optimized release builds with LTO
- Static linking where possible
- Containerized deployment options

### Library Distribution
- Semantic versioning for all crates
- Comprehensive changelogs
- Backward compatibility guarantees
- Clear migration guides for breaking changes

---

## Recent Updates

### October 28, 2025 - MCP Client Completion
- ✅ **MCP client implementation complete** (agentflow-mcp v0.1.0-alpha)
- 162 tests, 100% pass rate
- Property-based testing with proptest
- Comprehensive documentation
- Ready for AgentFlow workflow integration
- **Impact**: Phase 3 MCP client work completed ahead of schedule (originally 6-9 months out)

### October 26, 2025 - Phase 1 Completion
- ✅ **v0.2.0 released**
- Retry mechanism and error context
- Workflow debugging tools
- Resource management system
- 74 tests, all passing
- 3,600+ lines of documentation

---

**Last Updated**: 2025-10-28
**AgentFlow Version**: 0.2.0
**agentflow-mcp Version**: 0.1.0-alpha
**Rust Edition**: 2021
