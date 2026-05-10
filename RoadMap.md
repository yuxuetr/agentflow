# AgentFlow Roadmap

Last updated: 2026-05-10

This roadmap is forward-looking only. For implemented behavior and current
stability boundaries, see:

- `docs/CURRENT_STATUS.md`
- `docs/STABILITY.md`
- `docs/API_COMPATIBILITY.md`

Historical evaluation context remains in `PROJECT_EVALUATION_2026-05-01.md`.
Short-term execution items live in `TODOs.md`.

## Direction

AgentFlow is converging on a v1-ready framework with two first-class execution
paths:

- deterministic DAG workflows for repeatable production automation;
- agent-native loops for planning, tool use, reflection, memory, and
  multi-agent collaboration.

The framework should keep those paths composable through stable node, tool,
skill, plugin, trace, and server contracts.

## Near-Term Priorities

### V1 Contract Hardening

- Keep `docs/STABILITY.md` and `docs/API_COMPATIBILITY.md` current whenever a
  public trait, manifest, trace schema, or server envelope changes.
- Add schema round-trip tests for every stable serialized contract.
- Document migration notes for every beta-to-stable promotion.

### Platform Reliability

- Continue replacing scaffold server/database behavior with production run
  storage, retention, and multi-tenant policy.
- Exercise `/v1/runs`, cancellation, SSE, and Web UI workflows against real
  persisted runs.
- Define backup/restore expectations for run, trace, and artifact data.

### Distributed Execution

- Move the distributed scheduler from foundation to production readiness.
- Add worker admission, authentication, resource limits, and failure-domain
  tests.
- Validate large DAG scheduling with mixed local and worker-executed nodes.

### Tool Calling and Policy

- Promote provider-native `tool_calls` / `tool_choice` support across LLM
  providers with prompt fallback.
- Keep `ToolMetadata`, idempotency, and capability requirements aligned with
  partial resume and sandbox policy.
- Expand policy tests for file/network/process/MCP/workflow tools.

### Ecosystem Packaging

- Turn `examples/ecosystem/` into installable official example packages.
- Add signed marketplace fixture artifacts for Skills and Plugins.
- Keep all official examples runnable in offline/mock mode by default, with
  live provider paths as opt-in.

### Evaluation and Release Quality

- Maintain live provider tests as explicit opt-in suites with cost guards.
- Add RAG evaluation datasets with recall/MRR/nDCG baselines.
- Keep release gates focused on formatting, clippy, workspace tests, schema
  compatibility tests, and example validation.

## Later Tracks

### Web UI Productization

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
