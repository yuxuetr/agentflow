# Hybrid Workflow

AgentFlow supports two complementary execution modes:

- Deterministic DAG workflows through `agentflow_core::Flow`.
- Agent-native loops through `agentflow_agents::AgentRuntime` and `ReActAgent`.

Hybrid workflow support connects those modes in both directions:

- `AgentNode` embeds an agent inside a DAG.
- `WorkflowTool` exposes a DAG workflow as an agent-callable tool.

Use this pattern when most of a process should remain deterministic, recoverable, and inspectable, but one step needs agent reasoning or tool selection.

## Architecture

```text
Parent Flow
  -> deterministic nodes
  -> AgentNode
       -> ReActAgent / AgentRuntime
          -> ToolRegistry
             -> built-in tools
             -> MCP tools
             -> WorkflowTool
                  -> child Flow
  -> downstream deterministic nodes
```

The DAG owns ordering, dependencies, retries, checkpoints, and workflow state. The agent owns planning, tool calls, reflection, memory, and stop reasons for the autonomous step.

## When To Use It

Use `AgentNode` when a DAG needs a non-deterministic decision point:

- Route a case based on free-form user input.
- Gather information with tools before returning a structured answer.
- Delegate one node to a Skill-backed agent.
- Preserve the rest of the workflow as a deterministic pipeline.

Use `WorkflowTool` when an agent should call stable automation:

- Format, validate, transform, or enrich data through an existing DAG.
- Keep deterministic business logic outside the prompt loop.
- Let an agent decide when to invoke a workflow, while the workflow remains testable on its own.

Avoid hybrid execution when every step can be modeled as a normal DAG node. Plain workflows are easier to replay and reason about.

## AgentNode

`AgentNode` wraps a `ReActAgent` as an `AsyncNode`.

Input:

| Key | Type | Required |
| --- | --- | --- |
| `message` | `FlowValue::Json(String)` | yes |

Outputs:

| Key | Type | Meaning |
| --- | --- | --- |
| `response` | `FlowValue::Json(String)` | final answer text |
| `session_id` | `FlowValue::Json(String)` | agent session id |
| `stop_reason` | `FlowValue::Json(Object)` | serialized stop reason |
| `agent_result` | `FlowValue::Json(Object)` | full serialized `AgentRunResult` |

`AgentNode` treats non-success stop reasons as node execution failures. That lets the parent DAG retry or fail using the same control flow as any other node.

Minimal shape:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use agentflow_agents::nodes::AgentNode;
use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_core::flow::{Flow, GraphNode, NodeType};
use agentflow_core::FlowValue;
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;
use serde_json::json;

let agent = ReActAgent::new(
  ReActConfig::new("gpt-4o").with_persona("Answer the workflow request."),
  Box::new(SessionMemory::default_window()),
  Arc::new(ToolRegistry::new()),
);

let agent_node = AgentNode::from_agent("assistant", agent);
let flow = Flow::new(vec![GraphNode {
  id: "agent".to_string(),
  node_type: NodeType::Standard(Arc::new(agent_node)),
  dependencies: Vec::new(),
  input_mapping: None,
  run_if: None,
  initial_inputs: HashMap::from([(
    "message".to_string(),
    FlowValue::Json(json!("Summarize this workflow state.")),
  )]),
}]);
```

## WorkflowTool

`WorkflowTool` wraps a `Flow` as a normal agent tool.

Default parameter schema:

```json
{
  "type": "object",
  "description": "Initial workflow inputs keyed by input name.",
  "additionalProperties": true
}
```

Each JSON property is converted into a `FlowValue`. Values that deserialize as explicit `FlowValue` are preserved; other values become `FlowValue::Json`.

Minimal shape:

```rust
use std::sync::Arc;
use std::time::Duration;

use agentflow_agents::tools::WorkflowTool;
use agentflow_tools::ToolRegistry;

let workflow_tool = WorkflowTool::new(
  "format_summary_workflow",
  "Run a deterministic child workflow that formats a summary.",
  child_flow,
)
.with_timeout(Duration::from_secs(10));

let mut registry = ToolRegistry::new();
registry.register(Arc::new(workflow_tool));
```

The tool output is a JSON serialization of the child workflow results. It is returned both as compatible string content and as a structured `ToolOutputPart::Resource` with a `workflow://<tool_name>` URI.

If any child workflow node returns an error, `WorkflowTool` returns an error `ToolOutput`. If the workflow exceeds its timeout, the tool call fails with a tool execution error.

## Runnable Example

Run the self-contained mock example:

```bash
cargo run -p agentflow-agents --example hybrid_workflow_agent
```

The example:

1. Builds a child DAG with one deterministic formatting node.
2. Wraps the child DAG as `format_summary_workflow`.
3. Registers that workflow tool in an agent `ToolRegistry`.
4. Embeds the agent in a parent DAG through `AgentNode`.
5. Uses a mock model response to call the workflow tool.
6. Prints the agent response and structured runtime result.

Expected high-level result:

```text
Agent response:
"Hybrid answer: workflow summary for hybrid DAG + agent runtime"
```

## Checkpoint And Resume

Hybrid workflows use the normal `Flow` checkpoint system.

When an `AgentNode` completes, its output is saved in workflow state. That includes the full `agent_result`, so checkpoint state preserves the agent step history:

- observe step.
- plan or tool call steps.
- tool result steps.
- reflection steps when enabled.
- final answer step.

On resume, `Flow` skips nodes already completed in the checkpoint. If a downstream node failed after an `AgentNode`, resume continues after the agent node and does not repeat completed agent tool calls.

This behavior is covered by checkpoint recovery tests that assert:

- checkpoint state preserves `agent_result.steps`.
- resume does not re-execute an already completed agent-like node.
- downstream nodes can retry after restoring the agent output from checkpoint state.

## Tracing

Workflow tracing can attach a listener to `Flow` and capture nested agent details from `agent_result`.

The trace path is:

```text
workflow trace
  -> node trace
     -> agent_result
        -> AgentTrace
           -> tool calls
```

Tool names beginning with `mcp_` are marked as MCP calls by the tracing layer, allowing one trace to connect:

```text
workflow -> agent node -> agent step -> tool call -> MCP tool
```

## Failure Modes

Common failure points:

- Missing `message`: `AgentNode` returns a node input error.
- Non-string `message`: `AgentNode` returns a node input error.
- Agent stop reason is not success: `AgentNode` returns a node execution error.
- Child workflow node fails: `WorkflowTool` returns an error tool output.
- Child workflow timeout: `WorkflowTool` returns a tool execution error.
- Agent repeats or chooses the wrong tool: tune the agent persona, tool descriptions, and schemas.

## Design Guidance

Keep deterministic work in DAG nodes. Give the agent a narrow decision boundary and expose stable work as tools.

Good hybrid boundary:

```text
parse input -> validate account -> AgentNode decides next action -> deterministic fulfillment workflow
```

Good workflow-tool boundary:

```text
agent investigates issue -> calls calculate_refund_workflow -> explains result
```

Risky boundary:

```text
agent owns every workflow step, retry decision, and data transformation
```

That shape loses most of the benefits of deterministic workflow orchestration.

## Current Boundaries

Current support includes:

- `AgentNode` wrapping `ReActAgent`.
- `WorkflowTool` wrapping `Flow`.
- Workflow tool timeout support.
- Agent result serialization into DAG output state.
- Checkpoint resume that skips completed agent outputs.
- Trace extraction from nested `agent_result`.

Known follow-up work:

- More declarative configuration for `AgentNode` and `WorkflowTool`.
- A fixed DAG workflow example independent of agents.
- More examples showing Skill-backed agents inside DAGs.
- Richer tool metadata source classification across built-in, script, MCP, and workflow tools.
