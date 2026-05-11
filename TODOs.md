# AgentFlow TODOs

Last updated: 2026-05-10

## 维护约定

- 旧执行计划已归档为 `TODOs-archive-2026-05-09-n1-n10.md` 和
  `TODOs-archive-2026-05-10-p0-p4.md`。
- 本文件是短期执行队列，只记录当前需要推进的任务。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `PROJECT_EVALUATION_2026-05-01.md` 保留为历史评估上下文。
- 任务状态只使用:
  - `TODO`: 未开始或正在执行。
  - `DONE`: 已完成、已测试、已提交。

This file is the short-term execution queue only. Current implemented status is
tracked in `docs/CURRENT_STATUS.md`; future direction is tracked in
`RoadMap.md`; historical evaluation context is in
`PROJECT_EVALUATION_2026-05-01.md`.

## Active Queue

Current focus: **Core Runtime Stabilization**.

Near-term scope is CLI-first, Rust SDK-first, and local server/daemon-first.
Slack, Telegram, Discord, desktop tray, webhook channel routing, and other
channel integrations are intentionally deferred. Keep extension points open,
but do not implement channel adapters in this queue.

## Recently Closed

- P3.3 Web UI Run Console.
- P4.1 v1 stable interface inventory.
- P4.2 official ecosystem samples.
- P4.3 documentation convergence.

## P0 — V1 Contract Hardening

Goal: lock down the public runtime contracts before adding more product
surface. Treat these tasks as the first execution lane.

- DONE P0.1 Stable schema fixture inventory:
  - List every stable/beta serialized contract from `docs/STABILITY.md`.
  - Identify fixture owners for `FlowValue`, checkpoint state, `AgentStep`,
    `AgentEvent`, `ToolDefinition`, Skill manifests, plugin manifests,
    marketplace manifests, trace events, server REST envelopes, and SSE
    events.
  - Add a short fixture map to the relevant docs or test module comments.
- DONE P0.2 `FlowValue` checkpoint compatibility tests:
  - Add tagged-schema round-trip tests for `Json`, `File`, and `Url`.
  - Add legacy raw-JSON checkpoint read fixtures.
  - Verify writers continue emitting tagged values.
- DONE P0.3 Agent trace compatibility tests:
  - Add golden fixtures for `AgentStep` and `AgentEvent`, including tool
    calls, tool results, handoff, blackboard, debate, memory, and capability
    decision events.
  - Verify unknown/additive JSON fields are tolerated where supported.
- DONE P0.4 Tool contract compatibility tests:
  - Add round-trip tests for `ToolDefinition`, `ToolMetadata`,
    `ToolPermissionSet`, `ToolIdempotency`, and typed `ToolOutputPart`.
  - Verify OpenAI-style `tools` array generation remains stable.
- DONE P0.5 Manifest compatibility tests:
  - Add fixtures for `SKILL.md`, `skill.toml`, `plugin.toml`, and remote
    marketplace TOML.
  - Verify unknown optional fields are ignored or preserved according to the
    documented contract.
- DONE P0.6 Server envelope and SSE compatibility tests:
  - Add JSON fixtures for `/v1/runs`, `/v1/runs/{id}`,
    `/v1/runs/{id}/graph`, `/v1/runs/{id}/events/history`, and SSE events.
  - Verify `seq`, `kind`, `payload`, and `ts` reconnect semantics.
- DONE P0.7 Documentation convergence cleanup:
  - Update or clearly mark stale claims in historical/evaluation docs about
    native tool calling, multi-agent support, server/db scaffold status, RAG
    eval, and OS sandbox support.
  - Keep historical reports as context, but prevent them from being mistaken
    for current authoritative status.

## P1 — Security And Tool Governance

Goal: make tool execution and local/server runtime behavior conservative,
auditable, and explicit.

- DONE P1.1 Security profile model:
  - Define `dev`, `local`, and `production` security profiles.
  - Document defaults for auth, CORS, request limits, tool permissions,
    sandboxing, plugin execution, and marketplace installs.
  - Wire profile selection through CLI/server config without changing current
    local defaults unexpectedly.
- DONE P1.2 Server production auth fail-closed:
  - Add explicit dev/prod mode to `agentflow-server` or `agentflow serve`.
  - In production mode, fail startup if `AGENTFLOW_API_TOKEN` or replacement
    auth config is missing.
  - Keep test/local-dev mode easy to run.
- DONE P1.3 Configurable CORS and request limits:
  - Replace unconditional permissive CORS for non-dev profiles.
  - Add max request body size for workflow submit and Skill run routes.
  - Add tests for rejected origins and oversized bodies.
- TODO P1.4 HTTP tool SSRF protection:
  - Reject loopback, link-local, private IP ranges, unix sockets, and cloud
    metadata endpoints by default.
  - Make exceptions explicit through `SandboxPolicy`.
  - Add DNS/IP resolution tests and redirect tests.
- TODO P1.5 File and script hardening pass:
  - Re-audit path canonicalization for read, write, list, and script execution.
  - Add tests for missing parent paths, symlink races where practical,
    hardlinks, absolute paths, and traversal.
- TODO P1.6 Sandbox enforcement visibility:
  - Include OS sandbox backend name and enforcing/non-enforcing status in tool
    capability decisions, trace events, and doctor output.
  - Add tests proving missing OS sandbox is visible, not silent.
- TODO P1.7 Non-idempotent tool resume policy:
  - Make manual recovery behavior for non-idempotent or unknown tool calls more
    visible in CLI/server output.
  - Add trace fields that explain why replay was allowed or denied.
- TODO P1.8 Plugin execution policy:
  - Define default plugin execution policy per security profile.
  - Require explicit opt-in for sandbox-disabled plugin execution in local
    and production profiles.
  - Add tests for plugin spawn denial and sandbox opt-in.

## P2 — Local Server / Daemon Reliability

Goal: make the server a dependable local execution control plane without
turning it into a channel hub.

- TODO P2.1 `agentflow serve` command:
  - Add CLI entry point for starting the local server/daemon.
  - Support config for bind address, port, database URL, run dir, trace dir,
    security profile, and auth token source.
  - Print clear startup diagnostics without leaking secrets.
- TODO P2.2 Run retention and cleanup policy:
  - Add database and filesystem retention settings for runs, events, steps,
    artifacts, and run directories.
  - Add cleanup command or background cleanup path.
  - Add tests for retaining active runs and deleting expired terminal runs.
- TODO P2.3 Server end-to-end run tests:
  - Cover submit, get, list, cancel, graph, event history, and SSE reconnect
    with persisted runs.
  - Include success, failure, cancellation, and validation-error workflows.
- TODO P2.4 SSE robustness:
  - Verify `after_seq` reconnect behavior across completed and active runs.
  - Ensure broker finalization does not drop terminal persisted events.
  - Add timeout-safe tests for subscribers.
- TODO P2.5 CLI local-daemon mode design:
  - Design a minimal `--server` / `AGENTFLOW_SERVER_URL` path for selected CLI
    commands to submit to a local daemon.
  - Keep direct in-process execution as the default.
  - Document which commands are local-only vs server-backed.
- TODO P2.6 Server tenant/session boundary:
  - Bind tenant/session selection to authenticated context where auth exists.
  - Keep single-tenant local-dev defaults.
  - Add tests showing callers cannot list/cancel another tenant's run in
    secured mode.
- TODO P2.7 Backup/restore expectations:
  - Document what must be backed up for DB, run artifacts, trace files,
    marketplace cache, and installed Skills/Plugins.
  - Add restore smoke test or manual validation checklist.

## P3 — Rust SDK And CLI Experience

Goal: make code-first and CLI-first usage clear, stable, and automation-ready.

- TODO P3.1 SDK example matrix:
  - Add or refresh examples for DAG workflow, ReAct agent, Plan-Execute,
    multi-agent handoff, SkillBuilder, MCP tools, RAG eval, tracing, and tool
    security.
  - Keep each example runnable offline or with mock providers by default.
- TODO P3.2 Official example smoke tests:
  - Add smoke tests for `examples/ecosystem/` and core CLI example workflows.
  - Ensure examples do not require live API keys unless explicitly marked.
- TODO P3.3 CLI JSON output audit:
  - Identify commands that automation users are likely to call.
  - Add `--output json` or equivalent where missing.
  - Add tests for stable field names on JSON outputs.
- TODO P3.4 `agentflow doctor` expansion:
  - Diagnose model config, missing env keys, feature flags, MCP config,
    sandbox backend, server/database availability, plugin runtime, and
    marketplace cache.
  - Provide both human-readable and JSON output.
- TODO P3.5 Permission explanation improvements:
  - Improve `skill inspect --explain-permissions` and workflow dry-run output
    so capability, policy, sandbox, and idempotency decisions are easy to
    inspect.
  - Add tests for representative shell/file/http/MCP/workflow tool policies.
- TODO P3.6 Native tool calling provider matrix:
  - Keep provider-native `tool_calls` / `tool_choice` behavior documented.
  - Add or refresh provider consistency tests for OpenAI, Anthropic, Google,
    Moonshot, StepFun, and mock providers.

## P4 — Memory, RAG, And Eval Foundations

Goal: make retrieval, memory, and agent quality measurable and regression-safe.

- TODO P4.1 RAG eval CI fixture:
  - Add a small offline dataset to CI using BM25 or mock retriever.
  - Assert Recall@K, MRR, nDCG@K, and latency output schema.
- TODO P4.2 RAG eval baseline snapshots:
  - Store expected metric baselines for bundled datasets.
  - Add comparison report fixtures for candidate-vs-baseline runs.
- TODO P4.3 Agent eval design:
  - Design local `agentflow eval` dataset format for task prompts, allowed
    tools, expected assertions, cost/latency limits, and trace capture.
  - Reuse `Flow` as the eval pipeline where possible.
- TODO P4.4 Minimal agent eval implementation:
  - Implement a local/mock-provider eval runner before external benchmarks.
  - Produce JSON and human-readable reports.
  - Capture trace replay links or trace IDs for failed cases.
- TODO P4.5 Memory layering design:
  - Define boundaries for session memory, semantic memory, preference memory,
    entity facts, retention, and forgetting.
  - Keep implementation behind traits so storage backends can evolve.
- TODO P4.6 Memory and prompt golden tests:
  - Add golden tests for prompt assembly, memory compaction, summary insertion,
    token budgets, and memory hook events.

## P5 — Plugin, Marketplace, And Worker Hardening

Goal: keep extension and distributed foundations usable without over-promising
v1 stability before security and reliability gaps are closed.

- TODO P5.1 Remote marketplace install handoff:
  - Complete verified artifact cache to `~/.agentflow/skills` or
    `~/.agentflow/plugins` install directory flow.
  - Keep checksum/signature verification mandatory before unpack.
- TODO P5.2 Signed fixture artifacts:
  - Add local signed Skill and Plugin fixture archives.
  - Test strict and non-strict verification paths.
- TODO P5.3 Marketplace unpack hardening:
  - Extend archive extraction tests for nested archives, duplicate metadata,
    executable bits, very large file counts, and invalid UTF-8 paths.
- TODO P5.4 Plugin sandbox default policy:
  - Define per-profile defaults for plugin sandboxing.
  - Add tests that plugin execution is denied or sandboxed according to the
    active profile.
- TODO P5.5 Worker auth/admission checks:
  - Add worker identity, admission policy, and rejected-worker tests.
  - Keep distributed worker APIs marked experimental until this lands.
- TODO P5.6 Worker resource limit tests:
  - Add tests for worker-executed DAG nodes respecting timeouts, memory/file
    size limits where enforceable, cancellation, and retry semantics.
- TODO P5.7 Distributed failure-domain tests:
  - Cover stale heartbeat, worker crash, retryable failure, non-retryable
    failure, duplicate completion, and trace stitching.

## Deferred / Explicit Non-Goals

These are intentionally out of the current queue. Leave extension seams where
reasonable, but do not implement product features for them yet.

- TODO Deferred.1 Channel adapters:
  - Slack, Telegram, Discord, email, webhook routers, desktop tray, and
    multi-channel message normalization are deferred.
- TODO Deferred.2 Local OS control tools:
  - Screenshot, keyboard, mouse, clipboard, and window-management tools are
    deferred until security profiles, sandboxing, audit, and confirmation
    hooks are stronger.
- TODO Deferred.3 Full SaaS productization:
  - Organization management, billing, hosted multi-user UI, OAuth/JWT,
    background Skill updates, and channel-based routing are deferred.

## Execution Notes

Pick one item at a time, expand it into concrete subtasks, then commit code and
sync this file after each completed feature.

## Quality Gates

For each task:

- Read relevant code/docs first.
- Implement the smallest coherent feature.
- Run focused tests or validation commands.
- Commit the feature with a conventional message.
- Update this TODO file only after the feature commit succeeds.
