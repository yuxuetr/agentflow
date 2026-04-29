# AgentFlow Examples Documentation

This directory contains comprehensive documentation for AgentFlow examples, demonstrating various patterns and use cases for building agent workflows.

## Available Examples

### 📘 [Runnable Tutorials (中文)](./runnable_tutorials_zh.md)

Copy-and-run tutorials for the current AgentFlow runtime paths:

- Fixed DAG workflow
- Agent-native ReAct runtime
- DAG + Agent hybrid workflow
- Skill + MCP tool discovery and call
- `WorkflowTool` wrapping a child DAG

### 🧰 [CLI Config-First Tutorial](./cli_config_first_tutorial.md)

Copy-and-run CLI tutorial for the current V2 path:

- mock model configuration
- fixed DAG dry-run and execution
- Skill inspect/list-tools/test
- Skill run with `--model`, `--memory`, and `--trace`
- `skill_agent` workflow execution
- RAG + Skill workflow dry-run with `agentflow-cli --features rag`
- marketplace install flow

### 🤖 [Simple Agent LLM Flow](./simple_agent_llm_flow.md)

**File**: `examples/simple_agent_llm_flow.rs`

A comprehensive demonstration of integrating LLM API calls within AgentFlow's async workflow system. This example showcases:

- **LLM Integration**: Using the moonshot demo pattern within AgentFlow nodes
- **Response Processing**: Automated analysis of AI responses (sentiment, complexity)
- **Intelligent Routing**: Dynamic flow branching based on response characteristics
- **State Management**: Proper shared state usage across nodes
- **Observability**: Built-in metrics and tracing throughout the flow

**Key Features**:
- Dynamic prompt templates with context substitution
- Multi-path routing based on AI response analysis
- Error handling and recovery patterns
- Real-time flow execution monitoring

**Usage**:
```bash
cargo run --example simple_agent_llm_flow
```

**Architecture**: 5-node workflow with intelligent branching based on LLM response analysis.

## Workflow Diagrams

### Visual Flow Representation

The examples include comprehensive workflow diagrams:

- **[Flow Structure Diagram](./simple_agent_llm_flow_diagram.mermaid)**: High-level flow architecture
- **[Execution Sequence Diagram](./execution_flow_diagram.mermaid)**: Detailed step-by-step execution

### Viewing Diagrams

#### Option 1: GitHub/GitLab (Automatic Rendering)
View `.mermaid` files directly in your git repository interface.

#### Option 2: Mermaid Live Editor
1. Copy diagram content from `.mermaid` files
2. Paste into [Mermaid Live Editor](https://mermaid.live/)
3. View rendered diagram

#### Option 3: VS Code Extension
Install the "Mermaid Preview" extension and open `.mermaid` files.

#### Option 4: Command Line Rendering
```bash
# Install mermaid-cli
npm install -g @mermaid-js/mermaid-cli

# Render diagram to PNG/SVG
mmdc -i simple_agent_llm_flow_diagram.mermaid -o flow_diagram.png
```

## Example Categories

### 🌟 **AI Integration Examples**
- `simple_agent_llm_flow.rs` - LLM-powered workflows with intelligent routing
- `agentflow-core/examples/fixed_dag_workflow.rs` - deterministic fixed DAG workflow without an LLM
- `agentflow-agents/examples/agent_native_react.rs` - mock agent-native ReAct loop with a local echo tool
- `agentflow-skills/examples/skill_calls_mcp_tool.rs` - Skill loads a local MCP server and calls a discovered tool
- `agentflow-agents/examples/hybrid_workflow_agent.rs` - DAG + Agent hybrid flow where an `AgentNode` calls a child workflow through `WorkflowTool`
- `agentflow-cli/examples/workflows/rag_skill_assistant.yml` - config-first RAG search followed by a Skill-backed agent node; dry-run is CI-friendly, full execution requires Qdrant and embedding credentials

### 🔧 **Core Functionality Examples** (from main examples/)
- `hello_world.rs` - Basic AsyncNode functionality
- `batch_processing.rs` - Parallel processing patterns
- `workflow.rs` - Sequential multi-stage workflows
- `chat.rs` - Interactive conversation flows
- `structured_output.rs` - Data extraction and validation

### 📊 **Advanced Pattern Examples** (planned)
- Multi-agent coordination workflows
- RAG (Retrieval-Augmented Generation) patterns
- Map-reduce distributed processing
- Enterprise robustness patterns

## Documentation Standards

Each example includes:

### 📋 **Complete Documentation**
- Overview and architecture description
- Detailed component explanations
- Configuration and setup instructions
- Usage examples and expected output
- Customization and extension guides

### 🎨 **Visual Diagrams**
- Flow structure diagrams (Mermaid)
- Sequence diagrams for execution flow
- Architecture overview illustrations

### 🔧 **Practical Guidance**
- Prerequisites and setup instructions
- Troubleshooting common issues
- Performance considerations
- Security best practices

### 🚀 **Real-World Patterns**
- Production-ready error handling
- Observability and monitoring
- Scalability considerations
- Integration with external systems

## Getting Started

1. **Choose an Example**: Start with `simple_agent_llm_flow` for LLM integration
2. **Read Documentation**: Review the complete `.md` file for the example
3. **View Diagrams**: Understand the flow structure using provided diagrams
4. **Run the Code**: Execute the example with `cargo run --example <name>`
5. **Experiment**: Modify and extend the example for your use case

## Prerequisites

### General Requirements
- Rust 1.70+ with async/await support
- Tokio runtime for async execution
- Access to external APIs (for LLM examples)

### LLM Examples Specific
- Valid API keys for LLM providers
- Properly configured `models.yml` file
- Network connectivity to API endpoints

## Configuration

### Environment Setup
```bash
# Create .env file with API keys
echo "MOONSHOT_API_KEY=your-key-here" > .env
echo "OPENAI_API_KEY=your-key-here" >> .env
```

### Models Configuration
```yaml
# models.yml example
models:
  moonshot-v1-8k:
    vendor: "moonshot"
    model_id: "moonshot-v1-8k"
    temperature: 0.7
    max_tokens: 1000
```

## Contributing

When adding new examples:

1. **Create the Example**: Add to `examples/` directory
2. **Write Documentation**: Create detailed `.md` file in `docs/examples/`
3. **Add Diagrams**: Include workflow diagrams in Mermaid format
4. **Update Index**: Add entry to this README
5. **Test Thoroughly**: Ensure example runs and documentation is accurate

### Documentation Template

```markdown
# Example Name

## Overview
Brief description of what the example demonstrates.

## Architecture
Detailed explanation of components and flow.

## Workflow Diagram
[Include Mermaid diagram]

## Running the Example
Step-by-step instructions.

## Customization
How to modify and extend the example.

## Troubleshooting
Common issues and solutions.
```

## Support

For questions or issues with examples:

1. **Check Documentation**: Review the complete example documentation
2. **Examine Code**: Study the implementation in `examples/` directory
3. **View Diagrams**: Use workflow diagrams to understand flow structure
4. **Test Locally**: Run examples in your environment
5. **Create Issues**: Report problems or suggest improvements

---

*These examples demonstrate production-ready patterns for building sophisticated agent workflows with AgentFlow's async execution framework and LLM integration capabilities.*
