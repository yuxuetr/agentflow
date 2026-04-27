# AgentFlow V2

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Documentation](https://img.shields.io/badge/docs-available-green.svg)](docs/)

> **A modular Rust agent framework for deterministic DAG workflows, agent-native execution loops, Skills, MCP tools, memory, tracing, and checkpointed recovery.**

AgentFlow V2 is evolving from a workflow orchestration engine into an agent framework with a shared runtime foundation. It supports deterministic DAG workflows for production automation and agent-native loops for planning, tool use, reflection, memory, and multi-step decision making.

## 🏗️ Architecture: Shared Runtime For Workflows And Agents

AgentFlow is built on five core principles:

1. **Directed Acyclic Graph (DAG)**: Workflows are defined as graphs where nodes have explicit dependencies.
2. **Agent-native runtime**: Agents record observe, plan, tool call, tool result, reflection, and final answer steps.
3. **Explicit I/O**: Nodes and tools receive typed inputs and produce typed outputs.
4. **Shared tools and skills**: Built-in tools, script tools, MCP tools, and workflow tools all adapt into one `ToolRegistry`.
5. **Recoverable execution**: Workflow checkpoints preserve node outputs and agent step history so interrupted runs can resume.

## 🤖 Agent Framework Positioning

AgentFlow now treats workflows and agents as complementary execution strategies:

- Use `agentflow-core::Flow` for deterministic production pipelines, batch jobs, RAG flows, and business processes.
- Use `agentflow-agents::AgentRuntime` / `ReActAgent` for autonomous loops with planning, tool use, reflection, memory, and runtime guards.
- Use `AgentNode` when a DAG needs an agent for a non-deterministic step.
- Use `WorkflowTool` when an agent should call a stable DAG workflow as a normal tool.
- Use Skills to package instructions, manifests, script tools, MCP server declarations, and runtime constraints.
- Use MCP integration to expose external tool servers through the same tool interface as local tools.

The intended dependency direction is:

```text
Flow -> AgentNode -> AgentRuntime -> ToolRegistry -> Tool / MCP / WorkflowTool
```

## 🎯 Two Ways to Build

### 💻 **Code-First Approach (Core SDK)**
For developers who need maximum power and type-safety.

```rust
use agentflow_core::{
    flow::{Flow, GraphNode, NodeType},
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    value::FlowValue,
};
use agentflow_nodes::nodes::llm::LlmNode;
use agentflow_nodes::nodes::template::TemplateNode;
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;

#[tokio::main]
async fn main() {
    // 1. Define the nodes in the workflow
    let template_node = GraphNode {
        id: "get_topic".to_string(),
        node_type: NodeType::Standard(Arc::new(TemplateNode::new("get_topic", "A short poem about {{topic}}."))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: {
            let mut map = HashMap::new();
            map.insert("topic".to_string(), FlowValue::Json(json!("the moon")));
            map
        },
    };

    let llm_node = GraphNode {
        id: "generate_poem".to_string(),
        node_type: NodeType::Standard(Arc::new(LlmNode::default())),
        dependencies: vec!["get_topic".to_string()],
        input_mapping: Some({
            let mut map = HashMap::new();
            map.insert("prompt".to_string(), ("get_topic".to_string(), "output".to_string()));
            map
        }),
        run_if: None,
        initial_inputs: HashMap::new(),
    };

    // 2. Create and run the flow
    let flow = Flow::new(vec![template_node, llm_node]);
    let final_state = flow.run().await.expect("Flow execution failed");

    // 3. Safely access the result
    if let Some(Ok(llm_outputs)) = final_state.get("generate_poem") {
        if let Some(FlowValue::Json(serde_json::Value::String(poem))) = llm_outputs.get("output") {
            println!("Generated poem: {}", poem);
        }
    }
}
```

### 📋 **Configuration-First Approach (CLI)**
For users who prefer declarative, dynamic workflows.

```yaml
# workflow_v2.yml
name: "Calculation Pipeline"
nodes:
  - id: initial_values
    type: template
    parameters:
      template: "{{a}} + {{b}}"
      a: 10
      b: 5

  - id: step_1
    type: llm
    dependencies: ["initial_values"]
    input_mapping:
      prompt: "Calculate {{ nodes.initial_values.outputs.output }}"

  - id: step_2
    type: llm
    dependencies: ["step_1"]
    run_if: "{{ nodes.step_1.outputs.is_valid }}" # Conditional execution
    input_mapping:
      prompt: "Summarize the result: {{ nodes.step_1.outputs.result }}"
```

```bash
# Execute via CLI
agentflow workflow run workflow_v2.yml
```

## 🚀 Key V2 Features

- **`FlowValue` Enum**: A unified, type-safe wrapper for multi-modal data (JSON, files, URLs).
- **Stateless Nodes**: Nodes are self-contained and receive all data via an `inputs` map.
- **DAG Execution Engine**: Workflows are defined with explicit `dependencies` for clear, traceable execution.
- **Explicit Input Mapping**: The `input_mapping` field provides full control over data flow between nodes.
- **Powerful Control Flow**: Native support for conditional execution (`run_if`), `while` loops, and `map` iteration (with parallel execution support).
- **File-based Persistence**: Each workflow run is saved to a unique directory for debugging and auditing.
- **Agent Runtime**: ReAct-compatible runtime with structured steps/events, stop reasons, reflection hooks, runtime guards, and golden test coverage.
- **Hybrid DAG + Agent Execution**: `AgentNode` embeds agents in DAGs; `WorkflowTool` lets agents call DAG workflows.
- **Skills + MCP Tools**: Skills can declare MCP servers, discover tools, expose schemas, and call them through the unified tool registry.
- **Tracing Across Boundaries**: Workflow, agent, tool, and MCP calls can be linked through structured trace events.
- **Checkpointed Recovery**: Workflow resume skips completed nodes and preserves serialized agent step history.

## ✨ New in v0.2.0: Production-Ready Stability

AgentFlow v0.2.0 introduces comprehensive stability and observability improvements for production workflows:

### 🔄 Retry Mechanism
Automatic retry with configurable strategies for handling transient failures.

```rust
use agentflow_core::{RetryPolicy, RetryStrategy, execute_with_retry};

let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::ExponentialBackoff {
        initial_delay_ms: 100,
        max_delay_ms: 5000,
        multiplier: 2.0,
        jitter: true,
    })
    .build();

let result = execute_with_retry(&policy, "api_call", || async {
    api_client.fetch_data().await
}).await?;
```

### 📝 Error Context Enhancement
Detailed error tracking with full execution context and history.

```rust
use agentflow_core::{execute_with_retry_and_context, ErrorContext};

let result = execute_with_retry_and_context(
    &policy, "run_123", "process_node", Some("processor"),
    || async { workflow.execute().await }
).await;

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err((error, context)) => {
        eprintln!("{}", context.detailed_report());
    }
}
```

### 🔍 Workflow Debugging Tools
Interactive workflow debugging and inspection via CLI.

```bash
# Validate workflow configuration
agentflow workflow debug workflow.yml --validate

# Visualize DAG structure
agentflow workflow debug workflow.yml --visualize

# Analyze complexity and bottlenecks
agentflow workflow debug workflow.yml --analyze

# Dry-run without execution
agentflow workflow debug workflow.yml --dry-run --verbose
```

### 💾 Resource Management
Configurable memory limits with automatic cleanup and monitoring.

```rust
use agentflow_core::{ResourceLimits, StateMonitor};

let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)  // 100 MB
    .max_value_size(10 * 1024 * 1024)   // 10 MB
    .cleanup_threshold(0.8)              // Clean at 80%
    .auto_cleanup(true)
    .build();

let monitor = StateMonitor::new(limits);

// Track allocations
monitor.record_allocation("data", data.len());

// Automatic cleanup when needed
if monitor.should_cleanup() {
    monitor.cleanup(0.5)?;  // Clean to 50%
}
```

### ⏱️ Timeout Control
Comprehensive timeout management for async operations.

```rust
use agentflow_core::timeout::{with_timeout, TimeoutConfig};

let config = TimeoutConfig::production();

let result = with_timeout(
    long_running_operation(),
    config.workflow_execution_timeout
).await?;
```

### 🏥 Health Checks
Kubernetes-compatible health and readiness monitoring.

```rust
use agentflow_core::health::{HealthChecker, HealthStatus};

let checker = HealthChecker::new();

checker.add_check("database", || {
    Box::pin(async {
        match db.ping().await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) => Err(format!("DB error: {}", e)),
        }
    })
}).await;

let report = checker.check_health().await;
```

### 💾 Checkpoint Recovery
Persistent workflow state for fault tolerance and resumability.

```rust
use agentflow_core::checkpoint::{CheckpointManager, CheckpointConfig};

let config = CheckpointConfig::default();
let manager = CheckpointManager::new(config)?;

// Save checkpoint after node execution
manager.save_checkpoint("workflow_123", "node1", &state).await?;

// Resume from checkpoint
if let Some(checkpoint) = manager.load_latest_checkpoint("workflow_123").await? {
    println!("Resuming from: {}", checkpoint.last_completed_node);
}
```

### 📊 Performance Guarantees

All features meet strict performance targets:
- Retry overhead: **< 5ms** per retry ✅
- Resource limit enforcement: **< 100μs** per operation ✅
- Error context creation: **< 1ms** ✅
- State monitor operations: **< 10μs** ✅
- Combined overhead: **< 1ms** ✅
- Timeout overhead: **< 100μs** ✅
- Health checks: **< 1ms** single, **< 10ms** multiple ✅
- Checkpoint save: **< 10ms** small, **< 50ms** large ✅
- Checkpoint load: **< 10ms** ✅

### 🎯 Production-Ready Metrics

- **87 tests** (54 unit + 17 integration + 12 benchmarks + 4 doc) - **100% passing**
- **Zero breaking changes** - Fully backward compatible
- **Zero compilation warnings**
- **10,000+ lines** of comprehensive documentation
- **5,200+ lines** of production-ready code

## 📚 Documentation

### Core Documentation
- **[RoadMap](RoadMap.md)**: Current direction for evolving AgentFlow into a DAG + agent framework.
- **[Agent Runtime](docs/AGENT_RUNTIME.md)**: Runtime boundary, core types, ReAct trace contract, and DAG interop.
- **[V2 Architecture](docs/ARCHITECTURE.md)**: Technical design for the DAG workflow architecture.
- **[Skills](docs/SKILLS.md)**: User-facing guide for packaging agent instructions, tools, MCP servers, knowledge, and memory.
- **[Skill Format](docs/SKILL_FORMAT.md)**: `SKILL.md` and `skill.toml` behavior for reusable capabilities.
- **[MCP Skills](docs/MCP_SKILLS.md)**: Operational guide for exposing MCP server tools through Skills.
- **[MCP Skills Integration](docs/MCP_SKILLS_INTEGRATION.md)**: Skills, MCP server configuration, and tool discovery.
- **[Hybrid Workflow](docs/HYBRID_WORKFLOW.md)**: Guide for embedding agents in DAGs and exposing workflows as tools.
- **[Release Checklist](docs/RELEASE_CHECKLIST.md)**: Manual quality gate before tagging or publishing.

### v0.2.0 Feature Guides
- **[Retry Mechanism](docs/RETRY_MECHANISM.md)**: Comprehensive guide to retry configuration and strategies
- **[Workflow Debugging](docs/WORKFLOW_DEBUGGING.md)**: CLI debugging tools and workflow visualization
- **[Resource Management](docs/RESOURCE_MANAGEMENT.md)**: Memory limits, monitoring, and automatic cleanup
- **[Timeout Control](docs/TIMEOUT_CONTROL.md)**: Operation timeout management and configuration
- **[Health Checks](docs/HEALTH_CHECKS.md)**: Kubernetes-compatible health and readiness monitoring
- **[Checkpoint Recovery](docs/CHECKPOINT_RECOVERY.md)**: Workflow state persistence and fault tolerance
- **[Migration Guide v0.2.0](docs/MIGRATION_GUIDE_v0.2.0.md)**: Upgrade guide from v0.1.0 to v0.2.0
- **[Release Notes v0.2.0](docs/RELEASE_NOTES_v0.2.0.md)**: Complete changelog and improvements

### Examples
- `agentflow-core/examples/retry_example.rs`: Retry mechanism demonstrations
- `agentflow-core/examples/fixed_dag_workflow.rs`: Deterministic fixed DAG workflow example
- `agentflow-core/examples/resource_management_example.rs`: Resource monitoring examples
- `agentflow-agents/examples/agent_native_react.rs`: Self-contained ReAct agent-native runtime example
- `agentflow-agents/examples/react_agent.rs`: ReAct agent runtime example
- `agentflow-agents/examples/hybrid_workflow_agent.rs`: DAG + Agent hybrid example
- `agentflow-skills/examples/skill_calls_mcp_tool.rs`: Skill-to-MCP tool call example
- `examples/skills/mcp-basic`: Minimal Skill with MCP server configuration
- `agentflow-cli/examples/workflows/`: Complete workflow examples including AI research assistant

## 📦 Installation

### For Developers (Core SDK)
```toml
[dependencies]
agentflow-core = { path = "agentflow-core" }
agentflow-nodes = { path = "agentflow-nodes" }
```

### For CLI Usage
```bash
cargo install --path agentflow-cli
agentflow --help
```

## 🛣️ Development Plan

See the tracked short-term plan in [TODO.md](TODO.md) and the longer-term framework direction in [RoadMap.md](RoadMap.md). A more granular local task list exists in `TODOs.md`, but that file is intentionally ignored.
