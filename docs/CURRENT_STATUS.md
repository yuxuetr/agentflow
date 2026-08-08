# Current Project Status

Last updated: 2026-08-08 (V4.1: refreshed the Active Work section, which
had gone stale since the 2026-07-28 update — H / P-A / S / L / R / T / U
have all since closed and archived, and the active segment is now **V**,
the 2026-08-05 production-readiness remediation — see `TODOs.md`)

This is the current authoritative status entrypoint for AgentFlow. Historical
evaluations, roadmap notes, and TODO queues may explain how the project arrived
here, but this document is the shortest maintained summary of what exists now
and what remains active.

## Summary

AgentFlow is a Rust workspace for deterministic DAG workflows, agent-native
runtime loops, Skills, MCP tools, RAG, memory, tracing, plugins, distributed
worker foundations, and a Web UI run console.

The current architecture is organized into five layers, with a narrow-waist
**contract kernel** (L0) extracted by the P-A track so the runtimes depend only on
shared contracts (enforced by `cargo xtask check-arch`; see
`docs/RFC_CRATE_ARCHITECTURE.md` and `docs/ARCHITECTURE.md`):

- L0 contract kernel: `agentflow-value` (`FlowValue`), `agentflow-graph`
  (the `Flow` IR), `agentflow-store-spi`, `agentflow-agent-spi`,
  `agentflow-async-util` (+ `agentflow-tools` as the `Tool` contract).
- L1 execution core (the executor): `agentflow-core` runs the L0 `Flow` IR via
  the `FlowExt` trait (`flow.run()`).
- L2 capability adapters: `agentflow-nodes` (tool-tier), `agentflow-nodes-ai`
  (capability-backed node adapters, split out of `agentflow-nodes` by the
  P-A4.0 nodes decomposition), `agentflow-llm`, `agentflow-tools`,
  `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`.
- L3 agent and orchestration: `agentflow-agents`, `agentflow-skills`,
  `agentflow-harness`, `agentflow-config` (shared config-first workflow assembly
  + diagnostics, consumed by both the CLI and server), `agentflow-cli`.
- L4 operations and productization: `agentflow-tracing`,
  `agentflow-server`, `agentflow-db`, `agentflow-worker`, `agentflow-ui`.

## Implemented Surfaces

- DAG workflow execution through `agentflow-core::Flow` (run via the `FlowExt` trait).
- Config-first workflow validation and execution through `agentflow-cli`.
- Agent-native runtimes through `AgentRuntime`, ReAct, Plan-Execute,
  reflection, memory, and supervisor patterns.
- Dynamic workflow: `agentflow_agents::dynamic::compile_plan_to_flow` compiles a
  declarative `WorkflowPlan` into a parallel `Flow` of tool calls, and
  `DynamicWorkflowAgent` makes the LLM planning call then compiles + executes.
  Exposed on the CLI as `agentflow workflow dynamic --goal ... --model ...`, where
  the LLM-authored plan runs against a restrictive built-in tool sandbox
  (`--allow-path` / `--allow-domain` grant access; shell is never registered),
  `--dry-run` prints the plan without executing, and `--approve` routes every tool
  call through the Harness approval pipeline.
- Harness governance shell (`agentflow-harness`): hooks, interactive approval,
  sandbox, audit, run limits, background tasks, and the `HarnessEvent` envelope.
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
- Sandbox and code-execution hardening (the **S** track, closed 2026-07-24
  through 2026-07-27): file+script combination-chain and skill-script
  integrity fixes; a real Linux 6.12 VM (via Apple's `container` CLI) used to
  compile- and kernel-enforce the Landlock + cgroup v2 backend; OS sandbox now
  defaults **on** (`security.os_sandbox = true`); `code_exec` — a mandatory,
  strongly-isolated `ContainerBackend` tool for running LLM-generated Python,
  separate from the OS-sandbox tier, with zero network access, hardcoded
  resource limits, and automatic Harness approval escalation
  (`ToolIdempotency::NonIdempotent`). See `docs/RFC_LLM_CODE_EXECUTION.md`.
- Long-horizon tasks and retrieval hardening (the **L** track, closed
  2026-07-2x): replan-loop closure for stalled/failed plan steps, task-summary
  recovery so a resumed session doesn't re-derive prior progress from scratch,
  project-level memory persisted across sessions, RAG retrieval
  strengthening, and a delegation contract (`agentflow-agent-spi::delegation`
  + `aggregation`) for sub-agent hand-off with schema-validated answers and
  conflict-flagged result aggregation.

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

The short-term execution queue remains in [`TODOs.md`](../TODOs.md). As of
this update, the H (Harness follow-ups), P-A (contract kernel), S (sandbox
hardening), L (long-horizon/RAG), R (2026-07-28 engineering-readiness),
T (2026-07-29 architecture-evaluation), and U (2026-07-30 re-review)
segments have all closed and been archived to `docs/archive/`. The active
segment is **V (2026-08-05 production-readiness remediation)** — findings
from six independent parallel sub-agents that each read one architecture
layer (L0 contract kernel / L1 execution core / L2 capability adapters /
L3 agent orchestration / L4 platform), cross-checked by the orchestrator
running `cargo test/clippy/fmt/tree` locally. V0 (blocking correctness/
security bugs), V1 (core execution/agent robustness), V2 (structured
output, token streaming, generic HITL interrupt/resume, agent-loop
checkpoint), and V3 (fail-closed defaults, execution-side sandbox
enforcement, SSRF hardening, run admission control, supply-chain
tightening, expr recursion-depth limit) have all closed. **V4
(documentation and engineering hygiene)** is the remaining segment —
regenerating this project's stale agent-facing docs (this update is part
of that), splitting `agentflow-cli/src/main.rs`, wiring `cargo-audit` into
CI, and a handful of completeness follow-ups (Semantic chunker factory
exposure, `input_mapping` whitespace drift, a Gemini `base_url` fix, and a
`~/.agentflow/runs` retention policy).

The ongoing documentation-convergence convention:

- keep this file as the current status entrypoint;
- keep `RoadMap.md` focused on future direction;
- keep `TODOs.md` focused on short-term execution;
- leave historical evaluations marked as historical references.

## Historical References

- [`PROJECT_EVALUATION_2026-08-05.md`](PROJECT_EVALUATION_2026-08-05.md):
  most recent evaluation (6 parallel layer sub-agents + orchestrator
  cross-check), B+/A- composite, production-readiness C+; drove the
  active V segment above.
- [`PROJECT_EVALUATION_2026-05-19.md`](archive/PROJECT_EVALUATION_2026-05-19.md):
  prior module-by-module evaluation (A overall, v1.0.0-rc.1 candidate).
- [`PROJECT_EVALUATION_2026-05-14.md`](archive/PROJECT_EVALUATION_2026-05-14.md):
  prior evaluation that informed the P6/P7/P-H/M segment additions.
- [`PROJECT_EVALUATION_2026-05-01.md`](archive/PROJECT_EVALUATION_2026-05-01.md):
  historical module-by-module evaluation that informed the P0-P4 task queue.
- [`RoadMap.md`](../RoadMap.md): roadmap and future direction.
- [`TODOs.md`](../TODOs.md): active execution queue and task completion record.
