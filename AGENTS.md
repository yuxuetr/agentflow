# AgentFlow Project - Codex Configuration

## Project Overview

AgentFlow is a Rust workspace that supports both deterministic DAG workflows and agent-native autonomous loops, with full LLM, MCP, RAG, Skill, and tracing support. The workspace is organized as a layered framework with 14 active crates plus 2 scaffold crates (`agentflow-server`, `agentflow-db`).

Recommended four-layer mental model:

- **L1 Execution Core**: `agentflow-core` (DAG engine, `AsyncNode`, `FlowValue`, scheduler, retry, timeout, checkpoint, resource manager, health, events)
- **L2 Capability Adapters**: `agentflow-nodes`, `agentflow-llm`, `agentflow-tools`, `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`
- **L3 Agent / Orchestration**: `agentflow-agents`, `agentflow-skills`, `agentflow-cli`
- **L4 Operations / Productization**: `agentflow-tracing`, `agentflow-viz`, `agentflow-server`, `agentflow-db`

The framework supports two complementary execution styles:

- **DAG workflows** via `agentflow-core::Flow` (sequential or `FlowExecutionMode::Concurrent` dependency-ready scheduling) with explicit I/O, checkpoints, retry, timeout, and conditional execution.
- **Agent-native loops** via `agentflow-agents::AgentRuntime` (ReAct, Plan-Execute, Reflection, Supervisor) with structured `AgentStep` / `AgentEvent` / `AgentStopReason`, tool calling, memory, and cancellation.

The two compose via `AgentNode` (agent embedded in DAG) and `WorkflowTool` (DAG exposed as agent tool). Config-first YAML supports `agent` / `skill_agent` node types so non-Rust users can build hybrid agents.

## Architecture Principles

### High Cohesion, Low Coupling
- Each crate has clearly defined responsibilities
- Minimal cross-crate dependencies
- Well-defined public APIs between components
- MCP integration as separate concern (future `agentflow-mcp` crate)

### Crate Responsibilities

#### L1 — agentflow-core
**Primary Focus**: DAG execution engine and core abstractions
- `Flow` orchestrator with topological sort and `FlowExecutionMode::{Serial, Concurrent}` (dependency-ready dispatch via `FuturesUnordered` + `max_concurrency`)
- `AsyncNode` trait + `GraphNode` (dependencies, `input_mapping`, `run_if`, `initial_inputs`)
- `NodeType::{Standard, Map, While}` with parallel/sequential map and conditional loops
- `FlowValue::{Json, File, Url}` for explicit, namespaced state pool
- Production primitives: retry/retry_executor, timeout, checkpoint, resource_manager, resource_limits, health, state_monitor, events

#### L2 — agentflow-nodes
**Primary Focus**: Built-in `AsyncNode` library
- 16+ node types: `llm`, `template`, `http`, `file`, `arxiv`, `markmap`, `batch`, `conditional`, `while`, `mcp`, `rag`, `asr`, `tts`, `text_to_image`, `image_to_image`, `image_edit`, `image_understand`
- Feature-gated (mcp, rag, etc.) so optional capabilities don't pull dependencies
- Factory pattern in `factory_traits.rs` for dynamic node instantiation

#### L2 — agentflow-llm
**Primary Focus**: LLM provider abstraction
- Unified fluent API: `AgentFlow::model(...).prompt(...).execute()`
- 6 providers: OpenAI, Anthropic, Google, StepFun, Moonshot, Mock
- Multimodal (text + image url/base64), streaming, model registry/discovery
- Pending: native `tool_calls` / `tool_choice` first-class support (currently routed via prompt protocol)

#### L2 — agentflow-tools
**Primary Focus**: Unified tool abstraction
- `Tool` trait + `ToolRegistry` + `SandboxPolicy` + `ToolPolicy`
- `ToolMetadata` with `source: ToolSource::{Builtin, Script, Mcp, Workflow}`, permissions, original MCP server/tool names
- Built-in `FileTool` / `HttpTool` / `ShellTool` (shell defaults to disabled)
- `ToolOutputPart::{Text, Image, Resource}` for typed multimodal output

#### L2 — agentflow-mcp
**Primary Focus**: Model Context Protocol integration
- Client + server + transport (stdio first), JSON-RPC 2.0
- Retry, timeout, reconnect; latency benchmarks
- Adapter into `agentflow-tools::ToolRegistry`

#### L2 — agentflow-rag
**Primary Focus**: Retrieval-Augmented Generation
- Document chunking, embeddings (OpenAI / StepFun API or local ONNX), Qdrant vectorstore, retrieval, reranking
- Document sources: PDF, HTML, CSV, text
- Pending: end-to-end recall/precision evaluation harness

#### L2 — agentflow-memory
**Primary Focus**: Agent conversation memory
- `MemoryStore` trait with `SessionMemory` (token-windowed in-memory) and `SqliteMemory` (persistent)
- `SemanticMemory` for similarity search (interlocks with `agentflow-rag`)

#### L3 — agentflow-agents
**Primary Focus**: Agent-native runtime and patterns
- `AgentRuntime` trait with `AgentContext`, `RuntimeLimits` (max_steps, max_tool_calls, timeout_ms, token_budget), `AgentCancellationToken`
- `ReActAgent` (observe/plan/tool/result/reflect/final answer with memory summary)
- `PlanExecuteAgent` (structured plan JSON + sequential execution)
- `ReflectionStrategy` trait (`FailureReflection` / `FinalReflection` / `NoOpReflection`)
- `MemorySummaryBackend` trait (`RecentOnlyMemorySummary` / `CompactMemorySummary`)
- `AgentNode` (agent in DAG) + `WorkflowTool` (DAG as agent tool) + `AgentNodeResumeContract` (partial resume)
- `Supervisor` multi-agent scaffold

#### L3 — agentflow-skills
**Primary Focus**: Declarative agent capability packages
- `SKILL.md` (recommended) + `skill.toml` (compatibility) parsing
- `SkillBuilder` wires persona / model / tools / knowledge / memory / mcp_servers / security into a runnable agent
- Local registry (`skills.index.toml`) + marketplace catalog
- CLI: `init`, `install`, `list`, `inspect`, `list-tools`, `run`, `chat`, `test`, `validate`, `index`, `marketplace`

#### L3 — agentflow-cli
**Primary Focus**: Unified user interface
- `workflow run|validate|debug` (with `--input`, `--dry-run`, `--output`, `--timeout`, `--max-retries`, `--model`, `--run-dir`, `--max-concurrency`)
- `config init|show|validate`
- `llm models` (model discovery only; interactive chat is via Skills/agents)
- `skill *`, `mcp list-tools|call-tool|list-resources`, `trace replay|tui`
- `audio asr|tts`, `image generate|understand`
- `rag search|index|collections` (feature-gated)

#### L4 — agentflow-tracing
**Primary Focus**: Observability
- Event collection via `EventListener` (non-invasive)
- Persistence: JSONL (default) or SQLite/Postgres (feature-gated)
- `agentflow trace replay` + TUI timeline
- OpenTelemetry exporter (OTLP)
- Redaction for API keys, env secrets, sensitive tool params
- `AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` for explicit storage roots

#### L4 — agentflow-viz
**Primary Focus**: DAG visualization
- YAML → VisualGraph → Mermaid / DOT / JSON
- Static visualization; not yet wired to live trace state

#### L4 — agentflow-server (scaffold)
**Primary Focus**: Axum gateway for platform mode
- Currently health/live/ready only (130 LOC, 0 tests)
- Pending: `/v1/runs`, `/v1/skills`, `/v1/runs/{id}/events` (SSE), AuthN/Z, tenant isolation

#### L4 — agentflow-db (scaffold)
**Primary Focus**: PostgreSQL persistence for the gateway
- Currently 48 LOC: pool initialization only
- Pending: run/step/event/artifact/skill_install/mcp_session schema + migration

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

#### Core Workflow Engine
- **Core workflow execution engine** - Async/await with Tokio, DAG execution, state management
- **Control flow nodes** - Map (sequential/parallel), While loops, Conditional execution
- **Built-in nodes** - 16+ node types including HTTP, File, Batch, Template (Tera), MarkMap, Arxiv

#### LLM & Multimodal
- **LLM provider integration** - 5 providers (OpenAI, Anthropic, Google, Moonshot, StepFun)
- **Multimodal support** - Text, image (generation/understanding), audio (TTS/ASR)

#### Observability & Reliability (Phase 1.5) ⭐ NEW!
- **Timeout Control** - Comprehensive timeout management with environment presets
  - Performance: ~244ns overhead (413x faster than target)
  - Environment presets (production/development/default)
  - Complete documentation with examples
- **Health Checks** - Kubernetes-compatible liveness/readiness probes
  - Performance: <4μs for 11 checks (2494x faster than target)
  - Built-in memory and metrics checks
  - Custom health check support
- **Checkpoint Recovery** - Workflow state persistence and fault tolerance
  - Performance: ~5.5ms save, ~97μs load
  - Automatic incremental checkpointing
  - Resume from failure capability
  - Configurable retention policies
- **Retry Mechanism** - Automatic retry with exponential backoff
- **Resource Management** - Memory limits, automatic cleanup, monitoring
- **Workflow Debugging** - Debug command, DAG visualization, validation tools
- **Structured Logging** - JSON/Pretty formats, trace/span support
- **Prometheus Metrics** - Comprehensive metrics collection system

#### Interfaces & Integration
- **CLI interface** - Workflow execution, LLM chat, image generation/understanding, audio processing
- **Configuration management** - YAML workflows, model registry, API key management
- **Testing** (2025-11-17 verified) - **479 tests** (381 unit + 98 integration) - **100% passing**
  - agentflow-core: 155 tests (107 unit + 48 integration)
  - agentflow-mcp: 162 tests (117 unit + 45 integration)
  - agentflow-llm: 49 unit tests (2 ignored)
  - agentflow-rag: 83 unit tests (4 ignored, 6 integration ignored)
  - agentflow-nodes: 25 unit tests (4 ignored)
  - agentflow-cli: 5 integration tests

### ✅ MCP Integration (Production-Ready) - ⭐ NEW!
- **MCP client** - Full-featured MCP client with comprehensive testing ✅
  - Status: `agentflow-mcp` crate complete and production-ready
  - Features: JSON-RPC 2.0, retry mechanism, timeout handling, stdio transport
  - Testing: 162 tests, 100% pass rate (includes property-based testing)
  - Documentation: Complete (TESTING.md, integration reports)
- **MCPNode** - Workflow integration complete ✅
  - Fully integrated into agentflow-nodes with AsyncNode trait
  - Configurable timeout and retry mechanisms
  - Dynamic parameter resolution from workflow context
  - Unit tests and integration tests complete
- **MCP CLI Commands** - Complete ✅
  - `agentflow mcp list-tools` - Discover available tools
  - `agentflow mcp call-tool` - Execute tools directly
  - `agentflow mcp list-resources` - List server resources
  - Colored output and JSON result export
- **Workflow Examples** - Production-ready examples ✅
  - Simple MCP integration example
  - Filesystem operations workflow
  - Advanced code analyzer (MCP + LLM + Templates)
  - Comprehensive documentation (MCP_EXAMPLES.md)
- **Voice cloning** - CLI command hidden, requires file upload API

### ✅ Agent-native Runtime (Production-Ready)
- **AgentRuntime trait** - `AgentContext`, `RuntimeLimits` (max_steps / max_tool_calls / timeout_ms / token_budget), `AgentCancellationToken`
- **ReAct loop** - structured Observe / Plan / ToolCall / ToolResult / Reflect / FinalAnswer steps with 8 typed `AgentStopReason` variants
- **Plan-Execute** - structured plan JSON + sequential execution
- **Reflection strategies** - pluggable `FailureReflection` / `FinalReflection` / `NoOpReflection`
- **Memory summary backends** - pluggable `RecentOnlyMemorySummary` / `CompactMemorySummary` for context window management
- **Memory hooks** - non-failing observers for memory read/search/write
- **Hybrid composition** - `AgentNode` (agent in DAG), `WorkflowTool` (DAG as agent tool), `AgentNodeResumeContract` (partial resume)
- **YAML config-first** - `agent` / `skill_agent` node types validated by schema and wired through factory

### ✅ RAG System (alpha → 0.3.0)
- **agentflow-rag** - Document chunking, embeddings (OpenAI/StepFun API or local ONNX), Qdrant vectorstore, retrieval, reranking
- **Sources** - PDF, HTML, CSV, text
- **CLI integration** - `agentflow rag search|index|collections` (feature-gated)
- **Pending**: end-to-end recall/precision evaluation harness

### 📋 Planned Features (Roadmap N8 / N9 / N10)

**N8 — Platform skeleton + native tool calling (v0.3.0 candidate):**
- Server gateway: `/v1/runs`, `/v1/skills`, SSE event streams, tenant-aware routing
- DB schema + migration: run / step / event / artifact / skill_install / mcp_session
- LLM `tool_calls` / `tool_choice` first-class in `agentflow-llm` (provider-native via OpenAI tools / Anthropic tool_use / Google function declarations, prompt fallback)
- `Tool` idempotency metadata for partial-resume auto-replay
- `FlowValue::File` / `Url` checkpoint round-trip type fidelity
- Expression engine upgrade for `run_if` / `while.condition` ✅
- Workspace edition unified on Rust 2024

**N9 — Multi-agent collaboration + ecosystem (v0.4.0 candidate):**
- Handoff / blackboard / debate collaboration patterns in `agentflow-agents/supervisor`
- Process-level sandbox (macOS sandbox-exec / Linux seccomp + chroot subset) for `ShellTool` / `ScriptTool`
- `Tool::requires_capabilities()` (`fs.read` / `fs.write` / `net` / `exec`) + effective capabilities after three-way merge (Skill / ToolPolicy / CLI flag)
- OpenTelemetry context propagation through LLM HTTP calls (`traceparent` headers)
- `agentflow-rag/eval/` harness with Recall@K / MRR / nDCG; `agentflow rag eval <dataset>`
- `docs/SKILL_PERMISSIONS.md` formalizes Skill/Tool/CLI permission merge algorithm

**N10 — Plugin / distributed / Web UI (v1.0.0-rc candidate):**
- Plugin / Custom Node system (dlopen+abi_stable or WASM via wasmtime/wasmer)
- Distributed scheduling: worker abstraction over gRPC / NATS / Redis Streams
- Web UI: React/Svelte SPA + `agentflow-server` SSE for live DAG / Agent / Tool state
- `docs/AGENT_SDK.md` five-minute extension tutorial

See `RoadMap.md` for the full plan; `PROJECT_EVALUATION_2026-05-01.md` for the 2026-05-01 evaluation that drove the prioritization.

## Implementation Roadmap

### Phase 1: Stabilization & Refinement ✅ COMPLETE (v0.2.0)
**Priority**: Fix existing issues, improve reliability, enhance developer experience

- ✅ **Fix compilation errors** - MCPToolNode V2 API migration
- ✅ **Voice cloning handling** - Hide unimplemented features
- ✅ **Documentation accuracy** - Update AGENTS.md to reflect reality
- ✅ **Testing improvements** - Added comprehensive test suite
- ✅ **Error handling** - Retry mechanisms, better error context
- ✅ **Workflow debugging** - Debug command, DAG visualization
- ✅ **Resource management** - Memory limits, state cleanup

**Deliverable**: ✅ Stable v0.2.0 released with improved reliability and developer experience

### Phase 1.5: Observability & Fault Tolerance ✅ COMPLETE (2025-11-16)
**Priority**: Production-ready observability and fault tolerance features

- ✅ **Timeout Control** - Comprehensive timeout management
  - Environment presets (production/development/default)
  - Configurable timeouts for different operation types
  - Minimal overhead (~244ns per operation)
  - Complete user documentation (TIMEOUT_CONTROL.md)

- ✅ **Health Checks** - Kubernetes-compatible monitoring
  - Liveness and readiness probe support
  - Built-in memory and metrics checks
  - Custom health check interface
  - Performance: <4μs for multiple checks
  - Complete user documentation (HEALTH_CHECKS.md)

- ✅ **Checkpoint Recovery** - Workflow state persistence
  - Automatic incremental checkpointing
  - Resume from failure capability
  - Configurable retention policies
  - Atomic file operations for safety
  - Performance: ~5.5ms save, ~97μs load
  - Complete user documentation (CHECKPOINT_RECOVERY.md)

- ✅ **Performance Benchmarks** - Comprehensive benchmark suite
  - 12 benchmark tests covering all features
  - All performance targets met or exceeded
  - Documented in PERFORMANCE.md

- ✅ **Documentation** - 10,000+ lines of comprehensive guides
  - API references with examples
  - Best practices and patterns
  - Kubernetes integration guides
  - Troubleshooting guides

**Deliverable**: ✅ Phase 1.5 complete - Production-ready observability and fault tolerance
**Test Coverage**: 87 tests (54 unit + 17 integration + 12 benchmarks + 4 doc) - 100% passing
**Documentation**: 3 major guides + updated README and PERFORMANCE.md

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

### Phase 3: MCP Integration ✅ COMPLETE! (2025-01-04)
**Priority**: Enable dynamic tool execution

**Status**: ✅ COMPLETE - Fully integrated and production-ready

- ✅ **agentflow-mcp client** - Production-ready MCP client
  - Full stdio transport layer ✅
  - JSON-RPC 2.0 protocol ✅
  - Retry + timeout mechanisms ✅
  - 162 tests, 100% pass rate ✅
  - Property-based testing ✅
- ✅ **MCPNode integration** - Workflow integration COMPLETE
  - MCPNode implemented in agentflow-nodes ✅
  - Added to node factory with feature flag ✅
  - Workflow examples created ✅
  - Comprehensive documentation ✅
- ✅ **CLI integration** - MCP commands COMPLETE
  - `agentflow mcp list-tools` ✅
  - `agentflow mcp call-tool` ✅
  - `agentflow mcp list-resources` ✅
- 📋 **Future enhancements**
  - HTTP/SSE transport support
  - MCP server (expose workflows as tools)
  - Server-initiated notifications

**Deliverable**: ✅ MCP fully integrated into AgentFlow workflows and CLI

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

- **Skill-agent hybrid** (`examples/workflows/skill_agent_hybrid.yml`) - DAG with `skill_agent` nodes, supports dry-run
- **RAG + Skill assistant** (`examples/workflows/rag_skill_assistant.yml`) - RAG search → template → skill_agent
- **Fixed DAG basic** (`examples/workflows/fixed_dag_basic.yml`) - DAG with no external API dependency
- **Agent-native ReAct** + **Plan-Execute** examples - in `agentflow-agents/examples/`

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

### May 3, 2026 - Multi-Agent Collaboration Patterns (P1 #7 closed) ✅
- ✅ **`AgentStepKind` + `AgentEvent` extended** — added `Handoff`,
  `BlackboardOp`, `DebateProposal`, `DebateVerdict` step kinds and matching
  `HandoffOccurred`, `BlackboardWritten`, `DebateRoundStarted`,
  `DebateVerdictRendered` events. `BlackboardOpKind::{Read, Write}` enum,
  10 serde round-trip tests.
- ✅ **`HandoffSupervisor`** (`agentflow-agents/src/supervisor/handoff.rs`)
  — agents transfer control via a shared `HandoffTool` + `HandoffSignal`;
  `HandoffSupervisorBuilder` with `add_agent(name, desc, factory)` /
  `initial_agent` / `max_handoffs(n)` / `use_signal(sig)`. 14 tests
  including 5 mock-LLM end-to-end (single handoff, no handoff, max cap,
  invalid target, cancellation). Example: `multi_agent_handoff.rs`.
- ✅ **`BlackboardSupervisor`** (`blackboard.rs`) — `Blackboard` shared
  `Arc<RwLock<HashMap>>` with versioned entries + per-agent `bb_read` /
  `bb_write` tools that replay through supervisor-level `BlackboardOp`
  steps + `BlackboardWritten` events. Schedules:
  `Sequential([..])` / `Parallel([..])`. Stops: `AllAgentsCompleted` /
  `KeySet(key)`. Optional `answer_from(key)`. 12 tests. Example:
  `multi_agent_blackboard.rs`.
- ✅ **`DebateSupervisor`** (`debate.rs`) — N participants run concurrently
  per round, optional revision rounds (each participant sees prior round's
  proposals), then a judge synthesises the final answer. Failed
  participants are recorded as empty proposals, judge still runs. 8 tests.
  Example: `multi_agent_debate.rs`.
- ✅ **`SkillBuilder::build_with_extra_tools(...)`** — new entry point that
  injects supplemental tools (handoff/blackboard) into a skill's tool
  registry before constructing the `ReActAgent`. Original `build()` is now
  a thin wrapper.
- ✅ **`multi_agent` YAML node** — `agentflow-cli/src/executor/multi_agent.rs`
  parses `mode: handoff|blackboard|debate` plus mode-specific YAML
  (`agents` / `participants` / `judge` / `schedule` / `stop_when` /
  `answer_from` / `rounds` / `judge_prompt`). `schema.rs` accepts the new
  node, `workflow run --model` propagates through. 5 config-parsing unit
  tests + 1 end-to-end CLI smoke (`cli_workflow_run_supports_multi_agent_handoff_node`).
- ✅ **`docs/MULTI_AGENT.md`** — user-facing reference: pattern decision
  table, Rust + YAML examples, trace shape, cancellation semantics,
  legacy-`Supervisor` compatibility notes.
- 📊 **Test counts**: agentflow-agents 119 (+34), agentflow-cli +1 CLI
  smoke + 5 multi-agent unit tests. Workspace clippy `-D warnings` clean.

### May 3, 2026 - Platform Skeleton (P0 #1 closed) ✅
- ✅ **`agentflow-db` schema + migrations** — `migrations/0001_initial_schema.sql`
  ships the 6-table v0.3.0 N8 control-plane schema (runs / steps / events /
  artifacts / skill_installs / mcp_sessions) with appropriate indexes.
  `Database::connect_and_migrate(...)` applies migrations idempotently via
  `sqlx::migrate!()`.
- ✅ **`agentflow-db` Repository layer** — `RunRepo` / `StepRepo` / `EventRepo` /
  `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo` traits + `Pg*Repo`
  Postgres impls; `Repositories::from_pool` bundles all six. Models
  (`Run` / `Step` / `Event` / `Artifact` / ...) derive `sqlx::FromRow` +
  serde for direct HTTP serialisation.
- ✅ **`agentflow-server` AuthN + unified error envelope** — `auth::require_bearer_token`
  middleware (constant-time compare, `AGENTFLOW_API_TOKEN` env);
  `ApiError` rewritten with stable `code` per variant
  (`{ "error": { "code", "message", "details" } }`).
- ✅ **`agentflow-server` v1 routes**:
  - `POST /v1/runs` accepts `{workflow, workflow_id?, tenant_id?}`,
    persists queued row, dispatches via `RunExecutor` trait.
  - `GET /v1/runs/{id}` reads runs table.
  - `GET /v1/runs/{id}/events` Server-Sent Events: per-run
    `tokio::sync::broadcast` + DB backfill with `?after_seq=` resume.
  - `GET /v1/skills` lists installed skills (via `AGENTFLOW_SKILLS_INDEX`
    pointing at `skills.index.toml`).
  - `POST /v1/skills/{name}:run` resolves skill, persists run with
    `workflow = "@skill:<name>"`, dispatches via the same `RunExecutor`.
- ✅ **EventListener bridge** — `WorkflowEventListener` adapts
  `agentflow_core::events::WorkflowEvent` (sync) to async DB+broker via
  unbounded mpsc + drain task. All 14 `WorkflowEvent` variants map to
  stable JSON payloads. The real Flow runner that replaces `StubExecutor`
  lands in v0.4.0; the bridge is ready.
- ✅ **`docker-compose.yml` + `docs/DEPLOYMENT.md`** — local Postgres + server,
  curl examples for submit / status / SSE / skills, unified error envelope.
  `AGENTFLOW_DATABASE_TEST_URL` documented for running DB-gated integration
  tests against the compose Postgres.
- ✅ **+14 server tests** (5 auth/error + 4 run routes + 1 SSE + 2 skill +
  2 listener bridge) — 7 are DB-gated and skip when env unset.

### May 3, 2026 - LLM Native Tool Calling (P0 #2 closed) ✅
- ✅ **`agentflow-llm/src/tool_calling.rs`** — typed `ToolSpec` / `ToolChoice` /
  `ToolCallRequest` / `StopReason` / `LLMResponse`, with provider-specific
  `StopReason` mapping helpers.
- ✅ **`ProviderRequest::tools` + `tool_choice`**, **`ProviderResponse::tool_calls` +
  `stop_reason`**, **`LLMClient::execute_full() -> LLMResponse`**, builder gains
  `.tool_choice(..)` / `.tools_from_openai_json(..)`.
- ✅ **`ModelCapabilities::native_tool_calling`** + `ModelConfig::native_tool_calling`,
  auto-derived by `ConfigUpdater` for OpenAI / Anthropic / Google / Moonshot /
  StepFun / DashScope.
- ✅ **All 6 providers** wired to native tool calling:
  - OpenAI: `tools` / `tool_choice` array, `tool_calls` parsing with JSON-arg decoding.
  - Anthropic: `tools` block with `input_schema`, `tool_use` content blocks → typed calls; `Required` ↔ `any` mapping.
  - Google: `tools[0].functionDeclarations` + `toolConfig.functionCallingConfig`; `functionCall` parts with synthesised `call_<idx>` ids; `STOP` rewritten to `ToolCalls` when calls present.
  - Moonshot / StepFun: OpenAI-compatible passthrough via shared helpers.
  - Mock: `with_tool_calls(..)` + `AGENTFLOW_MOCK_TOOL_CALLS` env-var queue.
- ✅ **`ReActAgent` + `PlanExecuteAgent`** prefer native `tool_calls`; both forward
  registered tools to the LLM via `collect_tool_specs()`. Empty `tool_calls` falls
  back to the existing JSON prompt parser unchanged.
- ✅ **+21 tests** (5 OpenAI, 4 Anthropic, 4 Google, 4 Moonshot/StepFun/Mock,
  2 mapping helpers, 2 ReAct/Plan-Execute end-to-end golden traces). Workspace
  test count: ≈500 (TTS env-dependent test still pending API key, unaffected).

### May 1, 2026 - Project Evaluation 2026-05-01 ✅
- ✅ **`PROJECT_EVALUATION_2026-05-01.md`** completed against HEAD `41ed3f8`
- ✅ **Composite rating: B+** — 架构 A-, DAG 内核 A-, agent-native B+, CLI/config-first B, 可观测性 B+, 平台化 C-
- ✅ **N1–N7 fully closed** — agent runtime productionization, observability/replay, security/tool governance, Skill CLI, CI quality gates, CLI productionization, unified trace/recovery
- ✅ **Confirmed dual-paradigm support** — DAG with concurrent dependency-ready scheduler + agent-native ReAct/Plan-Execute/Reflection + hybrid via `AgentNode`/`WorkflowTool`
- ✅ **YAML config-first agent now first-class** — `agent` / `skill_agent` node types in CLI factory + schema validation
- 📋 **Next phases (RoadMap N8/N9/N10)** — platform skeleton (server/db), LLM native tool calling, FlowValue checkpoint type fidelity, expression engine upgrade, multi-agent collaboration, plugin system

### November 17, 2025 - Compilation Fixes & Test Verification ✅
- ✅ **Fixed CLI compilation errors** - RAG command feature gate issues resolved
  - Added `#[cfg(feature = "rag")]` to Commands::Rag enum variant
  - Conditionally imported rag module based on feature
  - Fixed RagArgs struct feature gating
- ✅ **Fixed agentflow-nodes type errors** - FlowValue API corrections
  - Removed non-existent FlowValue::String usage
  - Updated to use FlowValue::Json(Value::String(...))
- ✅ **Fixed factory_traits mutability** - Conditional compilation improvements
- ✅ **Verified complete test suite** - **479 tests, 100% passing**
  - 381 unit tests across all crates
  - 98 integration tests
  - 16 ignored (require API keys/external services)
- **Impact**: Project now compiles cleanly and all tests verified passing!

### November 16, 2025 - Phase 1.5 Observability & Fault Tolerance COMPLETE! 🎉
- ✅ **Timeout Control System** - Production-ready timeout management
  - Environment presets (production/development/default)
  - ~244ns overhead (413x faster than target)
  - Complete documentation (TIMEOUT_CONTROL.md - 536 lines)

- ✅ **Health Check System** - Kubernetes-compatible monitoring
  - Liveness/readiness probe support
  - <4μs for 11 checks (2494x faster than target)
  - Complete documentation (HEALTH_CHECKS.md - 753 lines)

- ✅ **Checkpoint Recovery System** - Workflow state persistence
  - Automatic incremental checkpointing
  - ~5.5ms save, ~97μs load
  - Complete documentation (CHECKPOINT_RECOVERY.md - 721 lines)

- ✅ **Enhanced Performance Benchmarks**
  - 12 total benchmarks (was 9)
  - All performance targets met or exceeded
  - Updated PERFORMANCE.md with comprehensive results

- ✅ **Comprehensive Documentation**
  - 10,000+ lines total
  - API references, best practices, troubleshooting
  - Kubernetes integration examples

- **Test Coverage** (2025-11-17 verified): **479 tests** (381 unit + 98 integration) - **100% passing**
- **Impact**: Production-ready observability and fault tolerance complete!

### January 4, 2025 - MCP Integration COMPLETE! 🎉
- ✅ **Phase 3 MCP Integration fully complete**
- ✅ **MCPNode** implemented and integrated into agentflow-nodes
- ✅ **MCP CLI commands** complete (list-tools, call-tool, list-resources)
- ✅ **Workflow examples** created with comprehensive documentation
- **Impact**: MCP now fully integrated into AgentFlow workflows

### October 26, 2025 - Phase 1 Completion
- ✅ **v0.2.0 released**
- Retry mechanism and error context
- Workflow debugging tools
- Resource management system
- 74 tests, all passing

---

**Last Updated**: 2026-05-03
**AgentFlow Version**: 0.2.0+ (Phase 1.5 + N1–N7 complete; targeting v0.3.0 candidate)
**agentflow-mcp Version**: 0.1.0-alpha (Fully Integrated)
**agentflow-rag Version**: 0.3.0-alpha
**Rust Edition**: 2024 (all workspace members)
**Test Status**: ✅ 479/479 passing (100%, verified 2025-11-17)
**Composite Maturity Rating**: B+ (per `PROJECT_EVALUATION_2026-05-01.md`)
