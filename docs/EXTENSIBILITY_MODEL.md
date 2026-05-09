# Extensibility Model

AgentFlow currently has a layered extension model built around Rust nodes,
runtime tools, MCP servers, Skills, local and remote marketplace catalogs, and
subprocess plugins.

## Decision Guide

| Goal | Use | Why |
| --- | --- | --- |
| Add deterministic workflow behavior inside a DAG | Rust node | Nodes participate in `Flow` dependency ordering, checkpointing, and workflow state. |
| Let an agent call a local function or wrapper | Tool | Tools are runtime-callable functions registered in `ToolRegistry`. |
| Expose tools implemented outside AgentFlow | MCP server | MCP provides an external protocol/transport boundary; discovered tools are adapted into `ToolRegistry`. |
| Package an agent capability for reuse | Skill | Skills combine persona, model defaults, tools, MCP servers, knowledge, memory, and security. |
| Share Skills or Plugins inside a repo or organization | Skill registry / marketplace catalog | Index and marketplace files resolve packages; remote marketplace entries are verified before being cached locally. |
| Load arbitrary workflow extensions dynamically | Plugin | Subprocess JSON-RPC plugins provide process isolation, a manifest, lifecycle handshake, workflow node execution, and sandbox handoff. |

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
groups one or more indexes into a browsable local catalog. Remote marketplace
manifests provide a unified package index for Skills and Plugins.

Current catalog behavior:

- local-first and no-network by default;
- resolves a Skill directory and manifest;
- can pin a manifest checksum;
- prints or runs install flows that copy local Skill directories;
- can fetch a remote marketplace TOML over HTTP(S);
- verifies downloaded artifacts by SHA-256 and pluggable signature policy;
- stores verified artifacts under the local marketplace cache for offline
  verification.

Remote `install` currently stops at the verified artifact cache. Package-specific
unpack into `~/.agentflow/skills` or `~/.agentflow/plugins` is the remaining
handoff step.

### Plugin

Plugin is the dynamic workflow extension boundary. The implemented runtime uses
a subprocess child that speaks newline-delimited JSON-RPC 2.0 over stdio.

Current plugin behavior:

- `plugin.toml` manifest with name, version, runtime, entrypoint, protocol,
  node declarations, capabilities, and optional signature metadata;
- lifecycle: load, spawn child, handshake, execute node calls, shutdown, and
  drop-time cleanup;
- workflow YAML node type `plugin` routed through the CLI executor when built
  with the `plugin` feature;
- `agentflow plugin install|list|inspect|uninstall` for local plugin
  management;
- sandbox bridge through `AGENTFLOW_PLUGIN_SANDBOX=1`, translating plugin
  manifest capabilities into the same OS sandbox backend used by process tools;
- reference `agentflow-echo-plugin` binary and host demo under
  `agentflow-core`.

The first runtime is deliberately subprocess-based. WASM remains a possible
future runtime tier; native `dlopen` is not part of the supported extension
model.

## Composition

The intended dependency direction is:

```text
Workflow DAG -> AgentNode -> Agent Runtime -> ToolRegistry -> Tool / MCP / WorkflowTool
Skill -> Agent Runtime + ToolRegistry + Memory + Security
Skill Registry / Marketplace Catalog -> Skill directories / verified artifacts
Plugin Workflow Node -> PluginHost -> subprocess plugin
```

This keeps deterministic workflows, agent loops, tool calls, external tool
protocols, and reusable capability packages separate while still allowing them
to compose.

## Current Non-Goals

- No dynamic native plugin loading.
- No WASM plugin runtime.
- No automatic unpack from remote marketplace cache into runtime install
  directories.
- No background Skill updates.
- No automatic network fetch during local registry validation.

Future work may add these capabilities, but current documentation and CLI output
should distinguish the implemented subprocess plugin runtime and verified remote
artifact cache from still-pending package unpack and background update flows.
