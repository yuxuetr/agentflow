# AgentFlow

AgentFlow is a Rust workspace for building agent-native workflows.
It supports both deterministic DAGs and autonomous loops, with
first-class LLM, tool, MCP, RAG, and tracing integration.

## When to use which tier

- **L1 fixed pipeline** → use `agentflow-core::Flow` directly.
- **L3 agent decisions** → write a SKILL.md and run via
  `agentflow skill run`.
- **DAG with agent embedded** → use `AgentNode` inside a workflow.

## Quick start

```rust
use agentflow_core::Flow;

let flow = Flow::new(nodes);
let result = flow.execute().await?;
```

See the `examples/` directory for more.
