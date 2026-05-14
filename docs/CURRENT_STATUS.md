# Current Project Status

Last updated: 2026-05-10

This is the current authoritative status entrypoint for AgentFlow. Historical
evaluations, roadmap notes, and TODO queues may explain how the project arrived
here, but this document is the shortest maintained summary of what exists now
and what remains active.

## Summary

AgentFlow is a Rust workspace for deterministic DAG workflows, agent-native
runtime loops, Skills, MCP tools, RAG, memory, tracing, plugins, distributed
worker foundations, and a Web UI run console.

The current architecture is organized into four layers:

- L1 execution core: `agentflow-core`.
- L2 capability adapters: `agentflow-nodes`, `agentflow-llm`,
  `agentflow-tools`, `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`.
- L3 agent and orchestration: `agentflow-agents`, `agentflow-skills`,
  `agentflow-cli`.
- L4 operations and productization: `agentflow-tracing`, `agentflow-viz`,
  `agentflow-server`, `agentflow-db`, `agentflow-worker`, `agentflow-ui`.

## Implemented Surfaces

- DAG workflow execution through `agentflow-core::Flow`.
- Config-first workflow validation and execution through `agentflow-cli`.
- Agent-native runtimes through `AgentRuntime`, ReAct, Plan-Execute,
  reflection, memory, and supervisor patterns.
- Skills through `SKILL.md` and `skill.toml`.
- Tool abstraction through `Tool`, `ToolRegistry`, policy, permissions, and
  typed output parts.
- MCP client, server scaffolding, workflow nodes, CLI calls, and Skill tool
  integration.
- RAG search/index/eval foundations behind the `rag` feature.
- Trace persistence, replay, TUI, redaction, and OpenTelemetry mapping.
- Server run APIs, event history, SSE streaming, cancellation, and embedded Web
  UI run console.
- Subprocess plugin runtime, `plugin.toml`, workflow plugin nodes, plugin CLI,
  and marketplace schema support.
- Distributed scheduler foundation, gRPC worker protocol, worker runtime, and
  stitched worker trace events.
- Official offline-first ecosystem samples under `examples/ecosystem/`.

## LLM providers

The full per-provider capability matrix, `ProviderRequest` contract,
`ToolChoice` modes, `ModelCapabilities` flags, model families /
context windows, and rate-limit handling all live in
[`LLM_PROVIDERS_MATRIX.md`](LLM_PROVIDERS_MATRIX.md). That document is
the single source of truth for what each provider supports; entries
are verified by `agentflow-llm/tests/provider_consistency.rs` (offline)
and `provider_consistency_live.rs` (opt-in live).

## Stability

The v1 stability inventory lives in:

- [STABILITY.md](STABILITY.md)
- [API_COMPATIBILITY.md](API_COMPATIBILITY.md)

These documents define stable, beta, experimental, and internal surfaces for
Rust traits, manifests, trace schemas, server envelopes, and plugin/marketplace
contracts.

## Active Work

The short-term execution queue remains in [`TODOs.md`](../TODOs.md). As of this
update, the completed P0-P4 work has moved the project from platform skeletons
toward a documented v1 boundary and offline ecosystem samples.

The next active cleanup is documentation convergence:

- keep this file as the current status entrypoint;
- keep `RoadMap.md` focused on future direction;
- keep `TODOs.md` focused on short-term execution;
- leave historical evaluations marked as historical references.

## Historical References

- [`PROJECT_EVALUATION_2026-05-01.md`](../PROJECT_EVALUATION_2026-05-01.md):
  historical module-by-module evaluation that informed the P0-P4 task queue.
- [`RoadMap.md`](../RoadMap.md): roadmap and future direction.
- [`TODOs.md`](../TODOs.md): active execution queue and task completion record.
