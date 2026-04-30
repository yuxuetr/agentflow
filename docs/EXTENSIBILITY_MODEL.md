# Extensibility Model

AgentFlow currently has a layered extension model built around Rust nodes,
runtime tools, MCP servers, Skills, and local Skill catalogs. It does not yet
ship a general-purpose plugin runtime.

## Decision Guide

| Goal | Use | Why |
| --- | --- | --- |
| Add deterministic workflow behavior inside a DAG | Rust node | Nodes participate in `Flow` dependency ordering, checkpointing, and workflow state. |
| Let an agent call a local function or wrapper | Tool | Tools are runtime-callable functions registered in `ToolRegistry`. |
| Expose tools implemented outside AgentFlow | MCP server | MCP provides an external protocol/transport boundary; discovered tools are adapted into `ToolRegistry`. |
| Package an agent capability for reuse | Skill | Skills combine persona, model defaults, tools, MCP servers, knowledge, memory, and security. |
| Share Skills inside a repo or organization | Skill registry / marketplace catalog | Index and marketplace files are catalogs that resolve Skills; they are not plugin runtimes. |
| Load arbitrary extensions dynamically | Future plugin system | Not implemented yet; future work needs lifecycle, permissions, versioning, signatures, and loading strategy. |

## Concepts

### Rust Node

A Rust node implements workflow behavior for deterministic DAG execution. Use a
node when the work belongs inside a workflow graph and should be orchestrated by
`agentflow-core::Flow`.

Current surfaces:

- `AsyncNode`
- `GraphNode`
- CLI config-first node factory
- workflow schema validation

### Tool

A Tool is the runtime-callable function abstraction used by agents. Built-in
tools, script tools, MCP-adapted tools, and workflow tools all share the same
`Tool` trait and are registered in `ToolRegistry`.

Tool metadata includes:

- source: builtin, script, MCP, or workflow
- permissions: filesystem, process, network, MCP, workflow
- optional MCP server/tool origin

Tool execution passes through policy decision and audit recording before the
tool implementation runs.

### MCP

MCP is the external tool transport/protocol boundary. An MCP server owns its
implementation and schema; AgentFlow connects to it, lists tools, validates
tool arguments against `inputSchema`, and adapts each remote tool into the local
`ToolRegistry`.

Use MCP when:

- the tool implementation already exists outside AgentFlow;
- a separate process/runtime is desirable;
- the boundary should be protocol-based rather than linked into Rust.

### Skill

A Skill is an agent capability package. It is the recommended user-facing unit
for reusable agent behavior.

A Skill can declare:

- persona and model defaults
- built-in or script tools
- MCP servers
- knowledge/references
- memory backend
- security and tool permission policy

Skills are not plugins. They configure and assemble existing runtime
components; they do not provide a dynamic binary loading ABI.

### Skill Registry And Marketplace Catalog

`skills.index.toml` is a local index of Skill directories. `marketplace.toml`
groups one or more indexes into a browsable catalog.

Current catalog behavior:

- local-first and no-network by default;
- resolves a Skill directory and manifest;
- can pin a manifest checksum;
- prints or runs install flows that copy local Skill directories.

This is not a general plugin marketplace. It is a Skill catalog layer that can
later grow remote indexes, cache, bundle checksums, and trust policy without
changing the local model.

### Plugin

Plugin is reserved for a future extension boundary. AgentFlow should not claim
plugin runtime support until it has explicit answers for:

- lifecycle hooks
- permission model and policy enforcement
- version compatibility
- distribution and signatures
- dynamic loading strategy, such as process, WASM, or native ABI
- observability and audit records

## Composition

The intended dependency direction is:

```text
Workflow DAG -> AgentNode -> Agent Runtime -> ToolRegistry -> Tool / MCP / WorkflowTool
Skill -> Agent Runtime + ToolRegistry + Memory + Security
Skill Registry / Marketplace Catalog -> Skill directories
```

This keeps deterministic workflows, agent loops, tool calls, external tool
protocols, and reusable capability packages separate while still allowing them
to compose.

## Current Non-Goals

- No dynamic native plugin loading.
- No WASM plugin runtime.
- No remote plugin marketplace.
- No background Skill updates.
- No automatic network fetch during local registry validation.

Future work may add these capabilities, but current documentation and CLI
output should describe the implemented system as Skills, Tools, MCP, and Skill
catalogs rather than a complete plugin system.
