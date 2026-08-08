# AgentFlow — Agent Guide

This file is the tool-agnostic counterpart to `CLAUDE.md` (Claude Code
reads that one; other agent runners read this one). Both describe the
same project and are kept in sync — if you're reading this because it's
stale again, `CLAUDE.md` is the fallback source of truth, and
`docs/CURRENT_STATUS.md` is the shortest maintained "what exists now"
summary.

## Project Overview

AgentFlow is a Rust workspace that supports both deterministic DAG
workflows and agent-native autonomous loops, with full LLM, MCP, RAG,
Skill, and tracing support. The workspace has 23 Rust crates plus 1 Web
UI crate (`agentflow-ui`, a Vite-built React SPA embedded by the server).

A narrow-waist **contract kernel** (L0) was extracted so the runtimes
never depend on each other, only on shared contracts, enforced by
`cargo xtask check-arch` (eight dependency laws; see
`docs/RFC_CRATE_ARCHITECTURE.md`). The four execution paradigms (static
DAG / native agent loop / harness / dynamic workflow) live in
`docs/ARCHITECTURE.md` § Four Execution Paradigms.

Two complementary execution styles compose via `AgentNode` (agent
embedded in a DAG) and `WorkflowTool` (a DAG exposed as an agent tool):

- **DAG workflows** via `agentflow-core::Flow` — explicit I/O,
  checkpoints, retry, timeout, conditional execution, and a
  dependency-ready concurrent scheduler.
- **Agent-native loops** via `agentflow-agents::AgentRuntime` (ReAct,
  Plan-Execute, Reflection, multi-agent supervisors) with structured
  `AgentStep` / `AgentEvent` / `AgentStopReason`, tool calling, memory,
  and cancellation.

Config-first YAML supports `agent` / `skill_agent` node types.

## Five-Layer Architecture

| Layer | Role | Crates |
| --- | --- | --- |
| **L0 Contract Kernel** | narrow waist — shared types only, zero runtime logic | `agentflow-value` (`FlowValue`), `agentflow-graph` (`Flow` IR / `AsyncNode` / `expr` / `AgentFlowError`), `agentflow-store-spi` (`MemoryStore` + `KnowledgeBackend`), `agentflow-agent-spi` (`AgentRuntime` façade + `Capability` lowering + HITL interrupt/resume), `agentflow-async-util` (retry/timeout/`race_with_limits`), `agentflow-tool` (the `Tool` contract: trait, `ToolRegistry`, `ToolMetadata`, `Capability`, `ToolPolicy`, `SecurityProfile`, `SandboxBackend`) |
| **L1 Execution Core** | the executor that runs the L0 `Flow` IR | `agentflow-core` (scheduler, checkpoint, retry-executor, resource manager, health, events, exposed via `FlowExt::run()`) |
| **L2 Capability Adapters** | tool-tier + capability-backed node implementations | `agentflow-nodes` (tool-tier), `agentflow-nodes-ai` (capability-backed), `agentflow-llm`, `agentflow-tools` (builtin tools + concrete OS-sandbox backends), `agentflow-mcp`, `agentflow-rag`, `agentflow-memory` |
| **L3 Agent / Orchestration** | agent runtimes, skills, harness, config assembly | `agentflow-agents` (incl. `dynamic` — LLM-authored plan → compiled `Flow`), `agentflow-skills`, `agentflow-harness` (approval/hooks/HITL/background tasks), `agentflow-config` (YAML schema + executor + diagnostics, shared by CLI and server), `agentflow-cli` |
| **L4 Operations / Productization** | observability and platform surfaces | `agentflow-tracing`, `agentflow-server`, `agentflow-db`, `agentflow-worker`, `agentflow-ui` |

Runtimes (`agentflow-agents`, `agentflow-harness`) depend on
`agentflow-tool` directly, never on `agentflow-tools` — that split (T3.3,
`docs/RFC_TOOL_CONTRACT_SPLIT.md`) keeps concrete OS-sandbox backends out
of the kernel. `agentflow-nodes` / `agentflow-nodes-ai` were split (P-A4.0)
so the tool tier carries no capability dependencies.

## What's Implemented

- **Core DAG engine** — async/await, topological sort, concurrent
  dependency-ready scheduler, Map/While/Conditional control flow, 16+
  built-in nodes (HTTP, File, Batch, Template, MarkMap, Arxiv, ...).
- **6 LLM providers** (OpenAI, Anthropic, Google, Moonshot, StepFun,
  Mock) with native `tool_calls`/`tool_choice`, multimodal, streaming,
  W3C `traceparent` propagation.
- **Agent-native runtime** — ReAct, Plan-Execute, Reflection,
  verification-before-stop, memory summary backends, hybrid DAG/agent
  composition, generic HITL interrupt/resume (`ask_user`), agent-loop
  checkpointing.
- **Multi-agent collaboration** — Handoff / Blackboard / Debate
  supervisors.
- **MCP** — client, server scaffolding, workflow nodes, CLI, bounded
  stdio reads + backpressured notification channel.
- **RAG** — chunking, embeddings (OpenAI API or local ONNX), Qdrant
  vectorstore, retrieval, reranking, eval harness (Recall@K/MRR/nDCG@K).
- **Skills** — `SKILL.md` builder wiring persona/model/tools/knowledge/
  memory/mcp_servers/security; tiered knowledge (inline files or RAG
  index); marketplace with Ed25519-verified signatures by default.
- **Harness Mode** — hooks, interactive approval, OS sandbox, audit
  trail, background tasks, parallel tool-call dispatch, flow governance.
- **Sandbox & code execution** — OS-level sandbox (macOS sandbox-exec /
  Linux seccomp+Landlock+cgroup v2) defaults **on** in production;
  `code_exec` runs LLM-generated Python inside a mandatory, strongly
  isolated `ContainerBackend` (zero network, hardcoded resource limits).
- **Observability** — `EventListener`, JSONL/SQLite/Postgres trace
  persistence, `trace replay` TUI, OTel span model + W3C context
  propagation (first-party OTLP transport is still deferred — bring your
  own `OtelSpanSink`).
- **Platform** — `agentflow-server` Axum gateway (`/v1/runs`, SSE,
  skills, Harness sessions/approvals), Postgres-backed `agentflow-db`
  (9 tables / 9 repos), per-tenant run admission control, distributed
  `agentflow-worker` (gRPC `WorkerProtocol`, 7 node payload types), and
  a React/Vite Web UI embedded at `/ui`.
- **Plugins & marketplace** — subprocess JSON-RPC plugin runtime,
  signed manifests (Ed25519), remote registry client.

Roadmap segments N8–N10 (platform skeleton, multi-agent/ecosystem,
plugin/distributed/Web UI) are closed. The active execution queue is in
`TODOs.md` — see `docs/CURRENT_STATUS.md` for the current summary of
what's active vs. closed.

## Development Guidelines

- **Indentation**: 2 spaces everywhere (NO TABS) — overrides Rust's
  default `rustfmt` tab convention project-wide.
- snake_case for functions/variables, PascalCase for types.
- Explicit error handling with custom error types per crate
  (`error.rs`, `thiserror`); library code stays `unwrap`/`expect`-free
  (test code, examples, and one-shot scripts are the only exception).
- `///` doc comments on public APIs; `async/await` with Tokio.
- Unit tests in-module (`#[cfg(test)]`), integration tests in `tests/`,
  example-driven development in `examples/`.
- YAML-based workflow/model configuration; hierarchical config
  (project → user → built-in defaults); runtime validation.

### Adding a new LLM provider
1. Provider module in `agentflow-llm/src/providers/`.
2. Implement the provider trait (auth + API calls).
3. Register in `agentflow-llm/config/models/`.
4. Update the model registry (`agentflow-llm/src/registry/`).
5. Add examples and tests.

### Adding a new node type
1. Node module in `agentflow-nodes/src/nodes/` (tool-tier) or
   `agentflow-nodes-ai/src/nodes/` (capability-backed).
2. Implement `AsyncNode` from `agentflow-core`.
3. Register the `type:` string in
   `agentflow-config/src/executor/factory.rs`.
4. Add config parsing, validation, examples, and tests.

### Adding a new CLI command
1. Define the command in `agentflow-cli/src/main.rs`.
2. Implement the handler in the matching `commands/` module.
3. Add output formatting, error handling, examples, and docs.

## Pre-Commit Requirements

- `cargo fmt` — formatting.
- `cargo clippy --workspace --all-features -- -D warnings` — lints.
- `cargo xtask check-arch` — the eight dependency-law guard.
- `cargo xtask println-lint` — no stray `println!`/`eprintln!` in
  library code.
- `cargo test --workspace --all-features` — all tests passing (the
  `code_exec` container-engine tests self-skip / fail loudly when no
  container engine is available locally — a known environment gap, not
  a regression signal).
- `cargo doc` builds; example workflows validate.

## Security Considerations

- Never commit API keys; use env vars or secure config files; mask
  secrets in logs and traces.
- Validate all user/workflow inputs (prompts, file paths, URLs);
  sanitize template inputs; validate YAML before execution.
- `SecurityProfile` (`Dev`/`Local`/`Production`) governs fail-closed
  defaults — unauthenticated non-loopback binds, unsandboxed shell
  nodes, and unsigned marketplace/plugin installs are all rejected
  under `Production` by default; see `docs/SECURITY_PROFILES.md`.
- `HttpTool` pins DNS resolution per-request to close TOCTOU
  DNS-rebinding gaps and normalizes IPv4-mapped IPv6 before SSRF
  classification — see `agentflow-tools/src/builtin/http.rs`.

## Where to look next

- `docs/CURRENT_STATUS.md` — shortest maintained "what exists now" summary.
- `docs/ARCHITECTURE.md` — the five-layer model and four execution paradigms in full.
- `TODOs.md` — active execution queue.
- `RoadMap.md` — longer-term direction.
- `docs/STABILITY.md` / `docs/API_COMPATIBILITY.md` — what's stable vs. experimental.
- `docs/LLM_PROVIDERS_MATRIX.md` — per-provider capability matrix.

---

**Last Updated**: 2026-08-08 (V4.1 — regenerated from `CLAUDE.md` +
`docs/CURRENT_STATUS.md`; the previous version dated 2026-05-03 had
drifted badly: it described 14 crates instead of the current 25-member
workspace, called `agentflow-server`/`agentflow-db` empty scaffolds
(now a 19.5K-LOC gateway and a 9-table/9-repo persistence layer), never
mentioned `agentflow-harness`/`agentflow-config`/`agentflow-value`/
`agentflow-graph`/`agentflow-store-spi`/`agentflow-agent-spi`/
`agentflow-worker`/`agentflow-ui`, and cited a stale "479 tests" figure
against an actual count in the thousands).
