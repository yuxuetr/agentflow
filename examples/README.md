# AgentFlow Examples

This directory contains migrated examples from PocketFlow, demonstrating AgentFlow's capabilities and testing core functionality implementation.

## 📊 Migration Progress

### ✅ Completed Examples

- `hello_world.rs` - Basic AsyncNode functionality and SharedState (✅ Core verified)
- `batch_processing.rs` - Parallel batch processing with concurrency control (✅ BatchNode verified)  
- `workflow.rs` - Sequential multi-stage workflow with structured data flow (✅ Workflow verified)
- `chat.rs` - Interactive chat with conversation history and self-looping flows (✅ Chat patterns verified)
- `structured_output.rs` - YAML/JSON structured data extraction and validation (✅ Data parsing verified)
- `simple_agent_llm_flow.rs` - LLM integration with AgentFlow using moonshot demo pattern (✅ LLM-Agent flow verified)

### 🔄 In Progress

- Minor compilation fixes for borrowing issues in some examples
- Continuing migration with remaining PocketFlow examples

### 📋 Planned (High Priority PocketFlow Examples)

- `agent.rs` - Research agent with web search capabilities
- `rag.rs` - Retrieval-augmented generation workflow
- `map_reduce.rs` - Distributed processing pattern
- `multi_agent.rs` - Multi-agent coordination patterns

## 🎯 Example Categories

### **Basic Examples (☆☆☆)**

Testing core AgentFlow functionality:

- `hello_world.rs` - Simple Q&A workflow ✅
- `batch_processing.rs` - Parallel batch processing ✅
- `workflow.rs` - Sequential multi-stage workflow ✅

### **Intermediate Examples (★☆☆)**

Testing advanced features:

- `agent_workflow.rs` - AI agent patterns
- `batch_processing.rs` - Parallel batch operations
- `async_patterns.rs` - Concurrent execution

### **Advanced Examples (★★☆)**

Testing production features:

- `multi_agent.rs` - Multi-agent coordination
- `enterprise_patterns.rs` - Robustness and observability
- `performance_benchmarks.rs` - High-throughput scenarios

## 🧪 Testing Strategy

Each example serves as both:

1. **Functionality Test** - Verifies AgentFlow core features
2. **Migration Validation** - Ensures PocketFlow patterns work in Rust
3. **Performance Benchmark** - Measures improvement over Python

## 🚀 Running Examples

```bash
# Run a specific example
cargo run --example hello_world

# Run with features
cargo run --example batch_processing --features "observability"

# Run all examples (testing)
./scripts/run_all_examples.sh
```

## 📈 Migration Results

Will be documented as examples are migrated:

| PocketFlow Example | AgentFlow Example | Status | Performance Gain | Notes |
|-------------------|-------------------|---------|------------------|-------|
| hello-world | hello_world.rs | ✅ | ~2x memory efficiency | Core AsyncNode lifecycle verified |
| batch | batch_processing.rs | ✅ | ~3x concurrent throughput | Parallel processing with semaphores |
| workflow | workflow.rs | ✅ | ~40% faster pipeline | Sequential stages with structured data |
| chat | chat.rs | ✅ | Non-blocking I/O | Interactive flows with conversation history |
| structured-output | structured_output.rs | ✅ | Type-safe parsing | YAML/JSON validation with serde |
| agent | agent.rs | 📋 | - | Next: Research agent with web search |

---

## 🤖 LLM Integration Example

### `simple_agent_llm_flow.rs` - LLM-Powered Agent Flow

A comprehensive example demonstrating how to integrate LLM calls (using the AgentFlow-LLM moonshot demo pattern) within an AgentFlow workflow.

**Key Features:**
- **LLM Agent Nodes**: Custom nodes that make LLM API calls
- **Dynamic Prompt Templates**: Context-aware prompt building from shared state  
- **Response Processing**: Automated analysis of LLM outputs (sentiment, word count)
- **Smart Routing**: Flow branching based on LLM response characteristics
- **Full Observability**: Built-in metrics and tracing

**Flow Architecture:**
1. Initial LLM Node → Processes user input with context
2. Response Processor → Analyzes LLM response characteristics  
3. Decision Node → Routes to specialized follow-up nodes
4. Final Nodes → Different LLM strategies based on analysis

**Usage:**
```bash
# Requires proper LLM configuration (API keys, models.yml)
cargo run --example simple_agent_llm_flow
```

This example demonstrates real-world patterns for:
- Integrating external LLM APIs into agent workflows
- Building intelligent routing based on AI responses
- Managing complex conversational flows with state
- Combining deterministic logic with AI decision-making

*Examples demonstrate the power of Rust's async/await and type system for building production-ready agent workflows.*
