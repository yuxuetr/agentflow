# AgentFlow Roadmap

Last updated: 2026-05-14

This roadmap is forward-looking only. For implemented behavior and current
stability boundaries, see:

- `docs/CURRENT_STATUS.md`
- `docs/STABILITY.md`
- `docs/API_COMPATIBILITY.md`

Historical evaluation context remains in `docs/archive/PROJECT_EVALUATION_2026-05-01.md`.
Short-term execution items live in `TODOs.md`.

## Direction

AgentFlow is converging on a Rust-native runtime for reliable AI workflows and
agents. The near-term strategy is **Core Runtime Stabilization**: make the
execution kernel, agent runtime, tool/MCP/Skill composition, tracing, security
governance, CLI, Rust SDK, and local server/daemon surfaces dependable before
adding product-channel integrations.

The framework has two first-class execution paths:

- deterministic DAG workflows for repeatable production automation;
- agent-native loops for planning, tool use, reflection, memory, and
  multi-agent collaboration.

Those paths should remain composable through stable node, tool, skill, plugin,
trace, and server contracts. Access is intentionally CLI-first and code-first,
with a local server/daemon as an execution control plane. Slack, Telegram,
Discord, desktop tray, and other channel integrations are deferred until the
runtime contracts and security model are stronger.

## Target Product Shape

AgentFlow should expose three primary access modes:

1. **Rust SDK** for embedding `Flow`, `AgentRuntime`, `ToolRegistry`,
   `SkillBuilder`, MCP, RAG, memory, and tracing in applications.
2. **CLI** for local workflows, skills, agents, diagnostics, trace replay,
   evaluation, plugin/marketplace management, and automation-friendly JSON
   output.
3. **Local server / daemon** for run submission, cancellation, event streaming,
   graph/history retrieval, Skill execution, and Web UI debugging. The daemon
   is a runtime service, not a multi-channel messaging hub.

## Near-Term Priorities

### P0 — V1 Contract Hardening

- Keep `docs/STABILITY.md` and `docs/API_COMPATIBILITY.md` current whenever a
  public trait, manifest, trace schema, or server envelope changes.
- Add schema round-trip tests for every stable serialized contract.
- Document migration notes for every beta-to-stable promotion.

### P1 — Security And Tool Governance

- Add strict security profiles for trusted local development, normal local
  operation, and production/server operation.
- Fail closed in production server mode when authentication is missing.
- Make CORS, request body limits, run limits, and tool policy explicit.
- Harden HTTP/file/shell/script/plugin execution against SSRF, traversal,
  unsafe defaults, non-idempotent replay, and missing sandbox enforcement.
- Record tool policy, capability, sandbox, and idempotency decisions in traces
  and server events.

### P2 — Local Server / Daemon Reliability

- Add `agentflow serve` as the supported local runtime service command.
- Continue hardening server/database behavior with production run storage,
  retention, cleanup, and tenant/session policy.
- Exercise `/v1/runs`, cancellation, SSE, and Web UI workflows against real
  persisted runs.
- Define backup/restore expectations for run, trace, and artifact data.
- Allow the CLI to optionally submit to a local daemon while keeping direct
  in-process execution as the default dependable path.

### P3 — Rust SDK And CLI Experience

- Maintain clear code-first examples for workflows, agents, Skills, MCP, RAG,
  tracing, and tool security.
- Keep human-readable CLI output ergonomic and machine-readable JSON output
  stable enough for automation.
- Expand `agentflow doctor` into a full environment and security diagnostic
  covering config, providers, feature flags, MCP, sandbox, server, database,
  and plugin readiness.
- Keep official examples offline/mock runnable by default, with live provider
  execution opt-in.

### P4 — Memory, RAG, And Eval Foundations

- Keep RAG evaluation datasets and recall/MRR/nDCG baselines under versioned
  fixtures.
- Extend evaluation from RAG-only quality checks to local agent/task harnesses.
- Define memory layers for session memory, semantic memory, user preferences,
  entity facts, and retention/forgetting policy.
- Add prompt assembly, memory compaction, and agent trace compatibility
  golden tests.

### P5 — Plugin, Marketplace, And Worker Hardening

- Complete verified remote artifact cache to Skill/Plugin install directory
  handoff.
- Add signed marketplace fixture artifacts for local verification tests.
- Keep subprocess JSON-RPC as the stable plugin runtime; keep WASM as a later
  option only after real plugin usage validates the contract.
- Move the distributed scheduler from foundation to production readiness only
  after worker admission, authentication, resource limits, and failure-domain
  tests are in place.

## Later Tracks

> **Looking for the consolidated post-v1.0 picture?** See
> [`docs/ROADMAP_v2.md`](docs/ROADMAP_v2.md) — it unifies the
> sections below, the `v1.x` entries in `TODOs.md`, and the
> §7 themes from the latest project evaluation into a single
> staging ground. The prose here is preserved as historical
> rationale; the v2 doc is the source of truth for "what comes
> after v1.0 GA".

### Harness Agent Mode

AgentFlow's evolution toward a long-lived, workspace-aware, governable agent
session pattern. Full design lives in `HARNESS_MODE_EVOLUTION.md`; active
execution tasks live in `TODOs.md` under the `P-H` parallel track.

The Harness layer must remain additive — it wraps existing `AgentRuntime`,
`ToolRegistry`, Trace, and Skill contracts rather than replacing them. It is
scheduled as a parallel track to P1-P5 (not after them), with these phase
prerequisites:

- H0 (contract inventory) — no prereq, can start immediately.
- H1 (runtime MVP) — depends on H0; uses existing AgentRuntime/ToolRegistry.
- H2 (hooks + approval) — depends on H1 and `P1.7` non-idempotent resume
  visibility.
- H3 (parallel tool calls) — depends on H1 and `P3.7` LLM provider matrix.
- H4 (background task tools) — depends on H2 and an in-process task runtime
  design from H0.
- H5 (server + Web UI integration) — depends on `P2.1` (`agentflow serve`),
  `P2.2` (retention), `P2.4` (SSE robustness), `P-H.2`, and `P6.*` Web UI
  baseline.
- H6 (advanced compatibility) — open-ended; remains in this Later Track and
  is promoted to TODOs only when individual items become concrete.

Explicit Non-Goals for Harness Mode (preserved here):

- Clone of OpenHarness TUI or provider subscription bridges.
- UI-first product shell that freezes the protocol before stream-JSON
  envelopes stabilize (see `HARNESS_MODE_EVOLUTION.md` Risk 5).
- Parallel runtime bypassing AgentRuntime / ToolRegistry / Trace contracts
  (Risk 1).
- Unsafe tool approval defaults that allow mutating tools without explicit
  policy (Risk 2).

### Distributed Execution

- Move the distributed scheduler from foundation to production readiness.
- Add worker admission, authentication, resource limits, and failure-domain
  tests.
- Expand worker-executable node types beyond the current `template/file/mock`
  set to include `llm`, `http`, `mcp`, and `agent` so worker mode is useful
  for real workloads. Tracked under `P2.8` in `TODOs.md` as a prereq for the
  rest of the P5 worker hardening sequence.
- Validate large DAG scheduling with mixed local and worker-executed nodes.

### Evaluation Expansion

- Maintain live provider tests as explicit opt-in suites with cost guards.
- Add RAG evaluation datasets with recall/MRR/nDCG baselines.
- Keep release gates focused on formatting, clippy, workspace tests, schema
  compatibility tests, and example validation.

### Web UI Productization

- **Positioning (P10.17.1, committed)**: debugger / run console only.
  Operator-dashboard features (cost aggregation, retry-rate trends,
  worker utilization, policy-decision summary) are intentionally
  out of scope — Prometheus + Grafana + BI tools cover those better.
  Full rationale + in/out scope table in
  [`docs/WEB_UI.md` § Product positioning](docs/WEB_UI.md#product-positioning).
- Add richer run creation flows, provider configuration diagnostics, and trace
  comparison.
- Add durable user preferences without leaking API tokens.
- Add operator-focused filtering for agent/tool/MCP/RAG events.

### Plugin Runtime Expansion

- Keep subprocess JSON-RPC as the stable v1 runtime.
- Re-evaluate WASM only after the subprocess contract is exercised by real
  third-party plugins.
- Avoid native `dlopen` plugin loading unless there is a concrete ABI strategy.

### Operations

- Define deployment profiles for local dev, single-node server, and distributed
  worker clusters.
- Add end-to-end smoke tests that cover CLI, server, worker, tracing, and Web
  UI paths.
- Improve observability dashboards around run latency, retry rates, tool policy
  decisions, and worker utilization.

## Non-Goals For V1

- Native dynamic library plugins.
- Unbounded remote code execution from marketplace packages.
- Provider-specific agent behavior that bypasses the shared Tool/Trace/Policy
  surfaces.
- A Web UI that is required for headless operation; CLI and trace replay remain
  first-class.
- Slack, Telegram, Discord, desktop tray, webhook channel routing, or other
  channel integrations before the core runtime and local daemon are stable.
- Local OS keyboard/mouse control as a default capability. Any future OS
  control tools must be feature-gated, explicitly authorized, audited, and
  reversible where possible.
