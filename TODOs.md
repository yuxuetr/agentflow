# AgentFlow TODOs

Last updated: 2026-05-14

## 维护约定

- 旧执行计划已归档为 `TODOs-archive-2026-05-09-n1-n10.md` 和
  `TODOs-archive-2026-05-10-p0-p4.md`。
- 本文件是短期执行队列，按 P-segment 组织。
- `docs/CURRENT_STATUS.md` 记录当前已实现状态。
- `RoadMap.md` 保留中长期路线。
- `PROJECT_EVALUATION_2026-05-01.md` 和 `PROJECT_EVALUATION_2026-05-14.md`
  保留为历史评估上下文。
- `HARNESS_MODE_EVOLUTION.md` 是 Harness Agent Mode 的设计规范，本文件 P-H
  段是它的可执行任务化展开。
- 任务状态只使用:
  - `TODO`: 未开始或正在执行。
  - `DONE`: 已完成、已测试、已提交。
  - `DEFERRED`: 显式推迟到 RoadMap Later Tracks 或 Non-Goals。

## Active Queue Overview

Current focus: **Core Runtime Stabilization + Harness Mode foundation**.

Near-term scope is CLI-first, Rust SDK-first, and local server/daemon-first.
Slack, Telegram, Discord, desktop tray, webhook channel routing, and other
channel integrations are intentionally deferred. Keep extension points open,
but do not implement channel adapters in this queue.

| Segment | Theme | Status |
| --- | --- | --- |
| P0 | V1 Contract Hardening | CLOSED |
| P1 | Security And Tool Governance | partially closed (P1.7-P1.9 active) |
| P2 | Local Server / Daemon Reliability | active |
| P3 | Rust SDK And CLI Experience | active |
| P4 | Memory, RAG, And Eval Foundations | active |
| P5 | Plugin, Marketplace, And Worker Hardening | active |
| P6 | Web UI Productization | NEW — active |
| P7 | Performance And Release Engineering | NEW — active |
| P-H | Harness Agent Mode (parallel track) | H0 + H1 + H2 + H3 + H4 closed; H5 next (gated on P2.1/P2.2/P2.4/P6.1) |
| M | Maintenance Tasks | NEW — ongoing |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## Recently Closed

- P0.1 - P0.7 V1 Contract Hardening (all seven items).
- P1.1 Security profile model (`dev` / `local` / `production`).
- P1.2 Server production auth fail-closed.
- P1.3 Configurable CORS and request limits.
- P1.4 HTTP tool SSRF protection.
- P1.5 File and script path hardening.
- P1.6 Sandbox enforcement visibility.
- P3.3 Web UI Run Console (alpha shell embedded in server).
- P4.1 v1 stable interface inventory.
- P4.2 official ecosystem samples.
- P4.3 documentation convergence.
- P-H.0 Harness contract inventory (new `agentflow-harness` crate, frozen envelopes, hook trait boundaries, `docs/HARNESS_MODE.md`).
- P-H.1 Harness runtime MVP (`HarnessRuntime`, four default context providers, JSONL persistence, tracing-dir bridge, `agentflow harness run|resume|list|inspect` CLI).
- P1.7 Non-idempotent tool resume policy (`ResumePlan` envelope + `Flow::load_resume_plan` / `Flow::resume_with_options` + `WorkflowEvent::ResumeDecisionRecorded` + `agentflow workflow resume-plan` CLI + `GET /v1/runs/{id}/resume-plan`).
- P2.1 `agentflow serve` command (`ServeConfig` + `run` / `run_check` library hooks + `agentflow serve --check` structured readiness diagnostic + subprocess wrapper).
- P-H.2 Hooks and approval (`HookedTool` wrapper + `wrap_registry` + 3 `ApprovalProvider`s + fail-closed Production escalation + traced approval lifecycle with scope cache).
- P3.7 LLM provider matrix documentation (`ProviderRequest` / `ToolChoice` / `ModelCapabilities` / model families / rate-limit sections + drift-detection doc-test). Unblocks P-H.3.
- P-H.3 Harness parallel tool calls (`ReActAgent` batch dispatcher: concurrent for Idempotent, serial for risky, deterministic LLM-order trace, partial-failure tolerance, atomic max-tool-calls precheck).
- P-H.4 Background task tools (`agentflow-harness::tasks`: `TaskRuntime` / `TaskHandle` / `TaskAgentFactory` + 5 built-in `task_*` tools + nested-spawn rejection + bounded output buffer + lifecycle events through parent SinkChain).
- P2.2 Run retention and cleanup policy (`agentflow-server::cleanup` module + per-profile defaults + DB/filesystem sweep + `agentflow cleanup --dry-run` CLI + background loop in `serve`). Per-run override deferred.
- P1.8 Plugin execution policy (`agentflow-tools::plugin_policy` + per-profile defaults + `agentflow plugin install --allow-unsandboxed-plugin --signed` + production opt-in rejection + `tracing::info!` trace target).
- P1.9 MCP capability + SkillSecurity merge policy (`agentflow-skills::policy::resolve_tool_policy` + `ResolvedToolPolicy` / `AdmissionSource` types + `docs/MCP_CAPABILITY_POLICY.md` precedence table; CLI flag wiring tracked under P3.5).

---

## P0 — V1 Contract Hardening (CLOSED)

Goal: lock down the public runtime contracts before adding more product
surface. All seven items DONE; kept here for navigation.

- DONE P0.1 Stable schema fixture inventory.
- DONE P0.2 `FlowValue` checkpoint compatibility tests.
- DONE P0.3 Agent trace compatibility tests.
- DONE P0.4 Tool contract compatibility tests.
- DONE P0.5 Manifest compatibility tests.
- DONE P0.6 Server envelope and SSE compatibility tests.
- DONE P0.7 Documentation convergence cleanup.

---

## P1 — Security And Tool Governance

Goal: make tool execution and local/server runtime behavior conservative,
auditable, and explicit.

- DONE P1.1 Security profile model (`dev` / `local` / `production`).
- DONE P1.2 Server production auth fail-closed.
- DONE P1.3 Configurable CORS and request limits.
- DONE P1.4 HTTP tool SSRF protection.
- DONE P1.5 File and script hardening pass.
- DONE P1.6 Sandbox enforcement visibility.

- DONE P1.7 Non-idempotent tool resume policy:
  - New `agentflow-core::resume` module exposes `ResumePlan` /
    `ResumeToolCall` / `ResumeDecision` / `ResumeIdempotency` /
    `ResumeSummary` / `ResumePlanOptions` + `build_resume_plan`. Plan
    schema version `1` (`RESUME_PLAN_SCHEMA_VERSION`).
  - `Flow::resume_with_options` blocks resume when any call is
    `requires_manual`; `Flow::resume` keeps the previous behaviour by
    threading default options. `Flow::load_resume_plan(workflow_id,
    options)` reads the plan without executing anything. Each plan
    entry emits a `WorkflowEvent::ResumeDecisionRecorded` trace event
    carrying `resume.tool_call_id`, `resume.tool`, `resume.idempotency`,
    `resume.decision`, `resume.reason`, and `resume.force_replay`.
  - CLI: `agentflow workflow resume-plan <run-id> [--checkpoint-dir]
    [--force-replay] [--format text|json]` renders the plan offline
    (no LLM, no DB).
  - Server: `GET /v1/runs/{id}/resume-plan?checkpoint_dir=…&force_replay=…`
    returns the same plan envelope. The route is registered alongside
    `/v1/runs/{id}/graph` so SSE / Web UI consumers can join on
    `run.kind = resume.decision.recorded`.
  - Tests: 10 `resume` unit tests + 7 CLI integration tests covering
    each `ResumeDecision` (replay / skip / requires_manual) plus the
    `--force-replay` opt-in and missing-checkpoint paths + 4 server
    route integration tests (auto-skip without `AGENTFLOW_DATABASE_TEST_URL`).

- DONE P1.8 Plugin execution policy:
  - New `agentflow-tools::plugin_policy` module exposes
    `PluginPolicy`, `PluginNetworkPolicy`, `PluginEvaluationInput`,
    and `PluginPolicyDecision`. `PluginPolicy::for_profile(profile)`
    returns the documented defaults:
    - `dev`: sandbox optional, signature optional,
      `network = ManifestAllowed`.
    - `local` (default): sandbox required, signature optional,
      `network = ManifestAllowed`. `--allow-unsandboxed-plugin`
      honored.
    - `production`: sandbox required, signature required,
      `network = ExplicitAllowOnly`. `--allow-unsandboxed-plugin`
      is recorded as a deny reason *unconditionally* so misuse is
      caught even when the active host happens to be sandboxed.
  - `agentflow plugin install` now evaluates the policy before any
    filesystem write. New CLI flags: `--allow-unsandboxed-plugin`,
    `--signed`. Decision fields are emitted via
    `tracing::info!(target = "agentflow.plugin.policy")` with
    structured fields (`plugin`, `profile`, `allowed`,
    `sandbox_active`, `signature_checked`, `network_policy`); a
    typed `WorkflowEvent` variant is intentionally deferred until
    enough consumers ask for it.
  - Tests:
    - 10 unit tests in `plugin_policy::tests` cover the dev/local/
      production matrix, the opt-in unconditional rejection under
      production, signature requirement, blanket-vs-explicit
      network admission, serde round-trip, and deny-reason
      aggregation.
    - 3 new CLI integration tests in `tests/plugin_cli_tests.rs`
      verify production rejects unsigned plugins, production
      rejects `--allow-unsandboxed-plugin` even with `--signed`,
      and `--help` lists the two new flags. All 9 plugin CLI
      tests still pass.
  - `docs/TOOL_PERMISSIONS.md` now has a "Plugin policy (P1.8)"
    subsection with the per-profile table and the operator-intent
    rule for `--allow-unsandboxed-plugin`.

- DONE P1.9 MCP capability + SkillSecurity merge policy:
  - New `agentflow-skills::policy` module exposes
    `resolve_tool_policy(PolicyResolutionInput) -> ResolvedToolPolicy`.
    `PolicyResolutionInput` carries every admission layer
    (`known_tools`, `skill_allowed_tools`, `skill_denied_tools`,
    `mcp_server_capabilities`, `skill_mcp_server_allowlist`,
    `cli_allow_tools`, `cli_deny_tools`, optional `fallback_policy`,
    and per-tool `tool_metadata`).
  - `ResolvedToolPolicy.decisions` is a `BTreeMap` so iteration
    order is stable for `--output json` consumers.
  - `ToolAdmission` carries `allowed`, `source` (`AdmissionSource`
    enum), `reason`, and an optional `mcp_server` field set when
    `AdmissionSource::McpServerCapability` fires.
  - Precedence (highest first): `CliDeny` → `CliAllow` →
    `SkillDeny` → `SkillAllow` → `McpServerCapability` →
    `ToolPolicyDefault`. Unmatched tools fall through to
    `NoMatch` with `allowed = false` — fail-closed by design.
  - `docs/MCP_CAPABILITY_POLICY.md` documents the rationale, the
    precedence table, the `PolicyResolutionInput` field set, and
    five worked examples (CLI deny override, skill deny beats MCP,
    MCP allowlist filter, `ToolPolicy` fall-back, no-match
    fail-closed).
  - Tests (11 in `policy::tests`): each precedence row + MCP
    allowlist gating + fallback policy allow + fallback policy
    deny + unmatched fail-closed + serde round-trip + allow/deny
    counter accuracy. All hermetic.
  - CLI surface (`agentflow skill inspect --explain-permissions`,
    `--allow-tool`, `--deny-tool`) is documented as the v1
    consumer of this surface; wiring the flags through every CLI
    entry point is tracked under `P3.5`.

---

## P2 — Local Server / Daemon Reliability

Goal: make the server a dependable local execution control plane without
turning it into a channel hub.

- DONE P2.1 `agentflow serve` command:
  - `agentflow-server::serve` exposes `ServeConfig`, `run`,
    `run_check`, `build_startup_report`, `ServeReadiness`, and
    `StartupReport`. Both the `agentflow-server` binary and the CLI
    subcommand go through the same path.
  - CLI `agentflow serve` spawns the `agentflow-server` binary (the
    inverse dep already runs cli→server in the server crate, so cli
    cannot link the server library; the subprocess hop preserves the
    one-binary deploy model). Flags supported: `--bind`,
    `--database-url`, `--run-dir`, `--trace-dir`, `--security-profile`,
    `--auth-token-env`, `--cors-origins`, `--max-body-mb`, `--check`.
    Each flag falls back to the documented env var.
  - `--check` runs the non-binding readiness diagnostic; emits a
    structured JSON report and exits with `0` / `1` / `2` for
    `ok` / `warn` / `fail`. Report carries the effective profile,
    bind, db reachability + host, sandbox backend, plugin runtime
    hint, paths, auth token env name, and a warnings / errors list.
    Auth tokens are never embedded; only the env var name and a bool
    flag.
  - Tests: 7 server-lib unit tests covering readiness promotion, host
    extraction, missing DB, production-without-token (fail),
    local-with-token (warn), plus 4 CLI integration tests covering
    the structured JSON output, production fail-closed, token-present
    happy path with secret redaction, and `--help` flag surface. All
    tests run hermetically — no Postgres or open ports required.

- DONE P2.2 Run retention and cleanup policy:
  - `agentflow-server::cleanup::cleanup_expired(db, run_dir_root,
    config)` runs the DB sweep + filesystem sweep in one call. Returns
    a structured `CleanupReport` (started_at/finished_at, per-category
    counts, targeted run id preview, `dry_run` flag).
  - `CleanupConfig::for_profile(profile)` provides the defaults the
    task spec calls for (`runs_retention_days` = 30 in `local`/`dev`,
    90 in `production`; `events_retention_days` = 14;
    `artifacts_retention_days` = 30; `run_dir_retention_days` = 14;
    `interval` = 1 h).
  - DB sweep refuses to touch `queued` / `running` runs (`WHERE status
    IN ('succeeded', 'failed', 'cancelled')` everywhere) and uses
    `INTERVAL` literals built from the retention days. Cascade FKs
    handle the `steps` rows owned by deleted runs. Dry-run mode
    swaps the `DELETE` for a `COUNT(*)` preview without mutating.
  - Filesystem sweep walks `run_dir_root` one level deep, only acts
    on UUID-named subdirectories, queries the DB to skip dirs whose
    run is still active, and gates by directory mtime against the
    cutoff.
  - `agentflow-server --cleanup [--dry-run]` runs the sweep once and
    exits with the JSON `CleanupReport` on stdout. `agentflow serve`
    spawns a background task that re-runs the sweep every
    `CleanupConfig::interval` and logs the report; failures retry on
    the next tick instead of crashing the gateway.
  - `agentflow cleanup [--database-url] [--run-dir] [--trace-dir]
    [--security-profile] [--dry-run]` CLI subcommand spawns the
    server binary in `--cleanup` mode, mirroring the `serve` pattern
    to avoid an `agentflow-cli` ↔ `agentflow-server` dep cycle.
  - Tests:
    - 7 unit tests in `cleanup::tests` covering profile defaults,
      dry-run flag, serde round-trip, UUID-name filter, and the
      missing-root short-circuit.
    - 3 server integration tests in `tests/cleanup_route.rs` that
      skip without `AGENTFLOW_DATABASE_TEST_URL`: dry-run targets
      old terminal runs without deleting; actual sweep deletes old
      terminal runs but keeps active + young; filesystem sweep
      removes orphaned UUID dirs while leaving active-run dirs in
      place.
    - 2 CLI tests assert the `--help` surface and the
      `--security-profile bogus` rejection.
  - Per-run override (`POST /v1/runs` body `retention_overrides`) is
    deferred to a follow-up; the schema would need a new table or
    JSONB column and isn't required for v1 cleanup hygiene.

- TODO P2.3 Server end-to-end run tests:
  - Add `agentflow-server/tests/e2e_runs.rs` integration suite covering:
    - Submit → poll → complete (success path).
    - Submit → cancel → terminal state.
    - Submit → fail (node error) → terminal + final event.
    - Submit invalid YAML → 400 with structured error.
    - List runs with pagination + status filter.
    - Get graph snapshot before / during / after run.
  - Use real Postgres via `testcontainers` or a startup-skipped feature.
  - Cover both authenticated and unauthenticated paths in
    `local`/`production` profiles.

- TODO P2.4 SSE robustness:
  - Verify `GET /v1/runs/{id}/events?after_seq=N` reconnect behavior across:
    - Active run (events still arriving).
    - Recently completed run (broker finalized, events persisted).
    - Long-completed run (broker dropped, only DB has events).
  - Ensure broker finalization does not drop terminal persisted events:
    add `finalize_with_grace_ms` between final event publish and broker
    teardown.
  - Add timeout-safe subscriber tests with `tokio::time::timeout`.
  - Add tests for client disconnect mid-stream (no leaked tasks).

- TODO P2.5 CLI local-daemon mode:
  - Design `--server <url>` / `AGENTFLOW_SERVER_URL` plumbing for selected
    commands. Document which commands are local-only vs server-backed:
    - `workflow run/list/cancel/logs/graph` → server-capable.
    - `skill run/list` → server-capable.
    - `mcp call-tool` / `llm` / `image` / `audio` → local-only (no server
      semantics needed for v1).
  - Keep direct in-process execution as the default when `--server` is
    omitted.
  - Add `--output json` for server-backed commands (depends on P3.3).
  - Add tests that submit via CLI and stream events back.

- TODO P2.6 Server tenant/session boundary:
  - Add `tenant_id` column to `runs`, `events`, `artifacts`, `skill_installs`
    (single-tenant default = `"default"`).
  - Bind tenant from authenticated context (header `X-Agentflow-Tenant`,
    falls back to token-bound tenant when JWT/multi-tenant lands).
  - Enforce row-level filter in repos: `WHERE tenant_id = $1`.
  - Add tests showing a caller in tenant A cannot list/cancel a run owned
    by tenant B.
  - Keep single-tenant local-dev defaults zero-config.

- TODO P2.7 Backup/restore expectations:
  - Author `docs/SERVER_BACKUP_RESTORE.md` covering:
    - DB tables that must be backed up.
    - Run artifact / trace file directories.
    - Marketplace cache directory.
    - Installed Skills / Plugins directories.
    - Recovery sequencing (DB before artifacts, why).
  - Add a `agentflow doctor --backup-check` smoke that confirms reachable
    directories and DB are writable.
  - Add a manual validation checklist for first stable release.

- TODO P2.8 Worker LLM/HTTP/MCP/Agent node execution support:
  - PREREQ for the rest of P5 worker hardening.
  - Extend `agentflow-worker::execute_supported_node_payload` to dispatch:
    - `llm` (via `agentflow-llm` provider abstraction).
    - `http` (via `agentflow-tools::builtin::http` with sandbox policy).
    - `mcp` (delegate to local or remote MCP client).
    - `agent` (run a minimal ReAct loop on worker).
  - Pass `traceparent`, run_id, step_id, tenant_id via gRPC metadata.
  - Add resource limits per node type (timeout, retry, max-output-bytes).
  - Add tests:
    - LLM call from worker with mock provider, trace stitched.
    - HTTP call from worker respects sandbox policy.
    - MCP call from worker uses the configured server URL.
    - Worker rejects unsupported node type with a structured error.
  - Document supported node types in `docs/DISTRIBUTED.md` and stamp
    `agentflow-worker` README accordingly.

---

## P3 — Rust SDK And CLI Experience

Goal: make code-first and CLI-first usage clear, stable, and automation-ready.

- TODO P3.1 SDK example matrix:
  - Refresh `examples/` to a canonical matrix with one runnable per:
    - DAG workflow with Map + While.
    - DAG workflow embedding `AgentNode`.
    - ReAct agent with native tool calling.
    - PlanExecute agent.
    - Multi-agent handoff supervisor.
    - Multi-agent blackboard supervisor.
    - Multi-agent debate supervisor.
    - SkillBuilder direct API.
    - MCP client + tool invocation.
    - RAG ingest + query + eval.
    - Tracing JSONL + OTel export.
    - Tool policy + sandbox capability decision.
  - Each example must be runnable offline with mock provider by default;
    set `AGENTFLOW_LIVE_PROVIDER=1` to opt into live.
  - Each example must compile under `--no-default-features` plus the
    relevant feature set.

- TODO P3.2 Official example smoke tests:
  - Add a `tests/examples_smoke.rs` test suite per relevant crate that
    runs each example with mock provider and asserts exit code + presence
    of key output markers.
  - Wire into CI as a separate job to avoid slowing the default test job.
  - Add a `cargo xtask examples-smoke` runner that mirrors the CI matrix
    for local debugging.

- TODO P3.3 CLI JSON output audit:
  - Identify automation-friendly commands and unify on `--output json` /
    `--output text` (default text):
    - `workflow run/list/cancel/graph/logs`.
    - `skill list/inspect/run`.
    - `llm models`.
    - `trace list/replay/show`.
    - `rag search/eval`.
    - `mcp list-tools/list-resources/call-tool`.
    - `plugin list/install/inspect`.
    - `doctor`.
  - Document the JSON envelope shape (envelope: `version`, `command`,
    `result`, `errors[]`) in `docs/CLI_JSON_OUTPUT.md`.
  - Add round-trip + stability tests for every JSON envelope shape.
  - Mark JSON outputs as stable surfaces in `docs/STABILITY.md`.

- TODO P3.4 `agentflow doctor` expansion:
  Library/CLI structural surface landed; deeper provider probes
  (MCP reachability + plugin spawn smoke) remain. Subtasks:
  - DONE Tri-state `DoctorStatus` (`ok` / `warning` / `fail`) with
    exit codes `0` / `1` / `2`. Existing `--format text|json` modes
    keep their JSON envelope; new fields are additive.
  - DONE `--profile dev|local|production` flag changes the
    pass/fail thresholds. Default is `local` (matches the security
    profile naming). `production` escalates missing API keys, missing
    auth-token env, missing run/trace dirs, and non-enforcing sandbox
    to `fail`. `dev` keeps the same checks but never escalates
    beyond `warning`.
  - DONE Disk reachability section: `run_dir`, `trace_dir`, and
    `marketplace_cache` checks (resolution via override → env →
    default, plus a per-dir write-probe). Source identifier
    (`env` / `default`) accompanies each path so operators can see
    why a directory was chosen.
  - DONE `--server <url>` reachability probe issues `GET <url>/health`
    with a 3 s timeout and surfaces `status_code` + error in the
    structured report. Unreachable server escalates to `fail`.
  - DONE Existing diagnostics (model config validation, provider API
    keys, feature flags, sandbox backend + enforcement, security
    profile) already covered by the prior shape; this slice plugs
    them into the new tri-state status calculation without changing
    their structure.
  - DONE Tests: 7 new CLI tests cover the default warning path, the
    production fail-closed path, the dev lenient path, the env-driven
    run-dir write probe, the unreachable-server probe, the text
    output sections, and the unknown-profile rejection. 2 existing
    `config_cli_tests::doctor_*` tests updated to accept exit 0/1
    as expected outcomes for missing-config scenarios.
  - TODO MCP server reachability via configured transport — defer
    until `agentflow mcp config` ships a structured config surface
    the doctor command can crawl.
  - TODO Plugin runtime spawn smoke (no-op plugin, ≤1 s) — defer
    until the plugin manifest schema includes a `dry_run` entry point
    so the smoke test does not depend on a real plugin binary.

- TODO P3.5 Permission explanation improvements:
  - Expand `agentflow skill inspect --explain-permissions <skill>`:
    - Print the resolved tool list and per-tool source / policy decisions.
    - Print effective sandbox profile.
    - Print MCP server permissions (links to P1.9).
  - Add `agentflow workflow validate --explain-permissions <yaml>` that
    does the same for non-Skill YAML workflows.
  - Add tests for representative shell / file / http / MCP / workflow
    tool policy outputs.

- TODO P3.6 Native tool calling provider consistency tests:
  - Add `agentflow-llm/tests/provider_consistency.rs` covering, per
    provider (OpenAI, Anthropic, Google, Moonshot, StepFun, Mock):
    - Streaming text deltas reach the consumer with stable framing.
    - `tool_calls` array round-trips.
    - `tool_choice = required|auto|none|named` semantics.
    - Multimodal user message (text + image URL) returns text.
    - Error mapping for 401 / 429 / 5xx.
  - Use VCR-style recorded fixtures for non-mock providers; gate live
    runs behind `AGENTFLOW_LIVE_PROVIDER=1`.
  - Block release on this suite (mock provider only) in default CI.

- DONE P3.7 LLM provider matrix documentation:
  - `docs/LLM_PROVIDERS_MATRIX.md` gains four authoritative sections:
    - `ProviderRequest contract` documents every field of
      `agentflow_llm::providers::ProviderRequest` (`model`,
      `messages`, `stream`, `parameters`, `tools`, `tool_choice`).
    - `ToolChoice modes` covers all four `ToolChoice` variants
      (`auto`, `none`, `required`, `tool` with `{ name }`).
    - `ModelCapabilities flags` covers the per-model levers
      (`model_type`, `supports_streaming`, `requires_streaming`,
      `supports_tools`, `native_tool_calling`, `max_context_tokens`,
      `max_output_tokens`, `supports_system_messages`,
      `custom_capabilities`).
    - `Model families & context windows` lists the documented
      vendor context windows per family with `tested` /
      `best_effort` / `n/a` verification status.
    - `Rate-limit handling` describes how `HTTP 429` flows through
      adapters (no auto-retry, `Retry-After` preserved in error
      message), the `LLMClient` retry plumbing, and the workflow
      `RetryPolicy` opt-in.
  - Cross-referenced from `README.md` (intro) and
    `docs/CURRENT_STATUS.md` (new LLM providers subsection).
  - Doc-test (`agentflow-llm/tests/provider_matrix_doc.rs`) catches
    drift in four ways: (a) destructuring `ProviderRequest` at
    compile time so a new field forces an update; (b) asserting each
    field name appears in the matrix wrapped in backticks; (c) the
    same for every `ToolChoice` variant; (d) every required
    `ModelCapabilities` flag and verification status string is
    present in the doc.

- TODO P3.8 Cross-hop OpenTelemetry context propagation:
  - LLM hop already propagates `traceparent` (closed). Extend to:
    - MCP transport: inject `traceparent` into stdio envelope or JSON-RPC
      `meta` field.
    - Plugin subprocess: pass `traceparent` via env var
      `TRACEPARENT` (W3C convention).
    - Worker gRPC: inject `traceparent` into gRPC metadata.
  - Add `agentflow-tracing::context::current_traceparent()` helper.
  - Add integration tests that a single DAG run produces a connected OTel
    trace across LLM → MCP → Plugin → Worker hops.
  - Update `docs/TRACE_PERSISTENCE_SCHEMA.md` "Hop continuity" subsection.

- TODO P3.9 CLI feature flag CI matrix:
  - Add a CI workflow `feature-matrix.yml` that runs `cargo check` and
    a minimal smoke test for each combination:
    - `--no-default-features`
    - `--no-default-features --features rag`
    - `--no-default-features --features mcp`
    - `--no-default-features --features audio`
    - `--no-default-features --features image`
    - `--no-default-features --features plugin`
    - `--no-default-features --features tracing-sqlite`
    - `--no-default-features --features tracing-postgres`
    - `--no-default-features --features otel`
    - `--all-features` (existing).
  - Mark broken combinations explicitly with a tracking issue.

- TODO P3.10 Examples smoke test CI:
  - Extend P3.2 into a CI job that runs every example in
    `examples/ecosystem/` and `agentflow-*/examples/` with mock providers.
  - Cap total wall time at 5 minutes; mark slow examples with a
    `slow_example` feature.
  - Fail CI if any example errors or panics.

---

## P4 — Memory, RAG, And Eval Foundations

Goal: make retrieval, memory, and agent quality measurable and
regression-safe.

- TODO P4.1 RAG eval CI fixture:
  - Add `agentflow-rag/eval_datasets/ci_offline/` with:
    - ~20 corpus docs (synthetic, public-domain only).
    - ~10 queries with graded qrels.
  - Add CI job `rag-eval-smoke.yml` running `agentflow rag eval
    eval_datasets/ci_offline --baseline bm25 --output json`.
  - Assert schema: `recall@5`, `mrr`, `ndcg@10`, `latency_ms_p50`,
    `latency_ms_p95`.
  - Block release on schema regressions.

- TODO P4.2 RAG eval baseline snapshots:
  - Store baseline metric snapshots under
    `agentflow-rag/eval_baselines/<dataset>/<retriever>.json`.
  - Add `agentflow rag eval --compare-baseline` that emits a candidate-vs-
    baseline report with paired sign test p-value.
  - CI fails when candidate is statistically worse than baseline by a
    configurable threshold (default: p < 0.05 + ≥3% absolute drop in
    `recall@5`).

- TODO P4.3 Agent eval format design:
  - Author `docs/AGENT_EVAL_FORMAT.md` defining the local
    `agentflow eval` dataset:
    - Test case fields: `id`, `prompt`, `tools_allowed`, `skill`,
      `expected_assertions[]`, `max_steps`, `max_tool_calls`,
      `cost_limit_usd`, `latency_limit_ms`.
    - Assertion DSL: `contains`, `regex`, `tool_called`, `tool_not_called`,
      `step_count_below`, `final_answer_matches_skill`.
    - Output schema: pass/fail, trace_id, cost actual, latency actual.
  - Cross-reference with `agentflow trace replay` for failed-case
    debugging.
  - Reuse `Flow` as the eval pipeline where possible.

- TODO P4.4 Minimal agent eval implementation:
  - Implement `agentflow-agents/src/eval/runner.rs` running cases from
    P4.3.
  - Implement `agentflow eval run <dataset>` CLI command.
  - Produce both JSON report and human-readable summary.
  - Capture trace IDs for failed cases.
  - Add a tiny offline dataset (mock provider) used by CI.
  - PREREQ for any release-gate quality claim.

- TODO P4.5 Memory layering design:
  - Author `docs/MEMORY_LAYERING.md` defining boundaries:
    - Session memory: in-process token-windowed.
    - Semantic memory: vector-backed, overlaps with RAG; document the
      seam (when to use which).
    - Preference memory: user-scoped key/value, durable.
    - Entity facts memory: extracted facts with provenance.
    - Retention/forgetting policy per layer.
  - Define `MemoryLayer` enum + per-layer trait extending `MemoryStore`.
  - Document migration path for current `SessionMemory` / `SqliteMemory`
    / `SemanticMemory`.
  - PREREQ for P4.7 implementation and P-H.4 background task context.

- TODO P4.6 Memory and prompt golden tests:
  - Add `agentflow-agents/tests/prompt_assembly_golden.rs`:
    - Prompt assembly determinism with session + summary + tool list.
    - Memory compaction crossover (when summary kicks in).
    - Token budget enforcement.
    - Memory hook event emission order.
  - Golden fixtures stored as JSON in `tests/fixtures/`.
  - Tolerate additive fields per P0.3 contract.

- TODO P4.7 Memory backend implementations (after P4.5 design):
  - Implement `PreferenceMemory` (SQLite-backed, encrypted-at-rest
    optional).
  - Implement `EntityFactsMemory` (SQLite-backed with provenance).
  - Extend `SemanticMemory` to align with the layering boundary from
    P4.5.
  - Add `retention.policy` config per memory layer.
  - Add tests for each backend and for cross-layer search precedence.

---

## P5 — Plugin, Marketplace, And Worker Hardening

Goal: keep extension and distributed foundations usable without
over-promising v1 stability before security and reliability gaps are closed.

PREREQ NOTE: Worker tasks (P5.5–P5.7) require P2.8 (worker node type
expansion) to be useful for non-trivial workloads.

- TODO P5.1 Remote marketplace install handoff:
  - Complete verified artifact cache → install dir flow for both Skills
    (`~/.agentflow/skills`) and Plugins (`~/.agentflow/plugins`).
  - Enforce checksum + signature verification before unpack.
  - Atomic install (temp dir + rename) so partial unpacks never leave
    half-installed state.
  - Add tests for: signature mismatch reject, checksum mismatch reject,
    partial download retry, atomic-rollback on extract failure.

- TODO P5.2 Signed fixture artifacts:
  - Add `agentflow-skills/tests/fixtures/signed/` and
    `agentflow-core/tests/fixtures/signed/` containing locally-signed
    Skill and Plugin archives.
  - Test both strict (`--require-signature`) and non-strict
    (`--allow-unsigned`) paths.
  - Document the signing flow in `docs/MARKETPLACE.md` "Local signing".

- TODO P5.3 Marketplace unpack hardening:
  - Extend archive extraction tests for:
    - Nested archives (zip inside tar).
    - Duplicate metadata (multiple `SKILL.md`).
    - Executable bits on extracted files.
    - Very large file counts (>10k entries).
    - Invalid UTF-8 paths.
    - Path traversal (`../../etc/passwd`).
    - Zip-bomb / decompression-ratio limits.
  - All should error cleanly, never write outside the target dir.

- TODO P5.4 Plugin sandbox default policy (tied to P1.8):
  - Per-profile defaults wired through `agentflow-tools::policy::PluginPolicy`.
  - Add tests that plugin execution is denied or sandboxed according to
    the active profile.
  - Document the policy resolution path in `docs/TOOL_PERMISSIONS.md`.

- TODO P5.5 Worker auth/admission checks (PREREQ: P2.8):
  - Worker identity via signed JWT or pre-shared key (configurable).
  - Server admission policy: allowlist of worker IDs, max workers, max
    concurrent tasks per worker.
  - `agentflow-server::workers::accept_admission` decides admit/reject.
  - Tests:
    - Rejected worker (unknown ID) cannot poll tasks.
    - Admitted worker can poll, heartbeat, and report.
    - Token rotation works without dropping in-flight tasks.
  - Mark distributed worker APIs experimental until this lands.

- TODO P5.6 Worker resource limit tests (PREREQ: P5.5):
  - Tests for worker-executed DAG nodes respecting:
    - Per-node timeout.
    - Memory limit (best-effort on Linux via cgroups; document caveat
      on macOS).
    - Output size limit (truncate at N bytes, recorded in trace).
    - Cancellation propagation.
    - Retry semantics.
  - Add a synthetic "runaway node" test fixture.

- TODO P5.7 Distributed failure-domain tests (PREREQ: P5.5, P5.6):
  - Cover scenarios:
    - Stale heartbeat → server marks worker dead, redistributes tasks.
    - Worker crash mid-task → task reattempted on another worker.
    - Retryable failure → retry on same or different worker.
    - Non-retryable failure → terminal state, no replay.
    - Duplicate completion → idempotency on result reporting.
    - Trace stitching across reattempts (single OTel trace).
  - Document in `docs/DISTRIBUTED.md` "Failure domains".

- TODO P5.8 Workflow `type: plugin` first-class node syntax:
  - Add `WorkflowNodeType::Plugin { plugin_id, entry_point, inputs }` in
    workflow YAML schema.
  - Map to `PluginNode` via factory.
  - Surface plugin manifest's declared node types as autocomplete data
    for `agentflow workflow validate --strict`.
  - Add `agentflow plugin generate-workflow-stub <plugin> --node <name>`
    that emits a YAML stub with the right input schema.
  - Add tests for plugin node dry-run + checkpoint roundtrip.

---

## P6 — Web UI Productization (NEW)

Goal: evolve the embedded Web UI from "alpha shell" into a usable run
console without making it a required surface.

Design constraint: Web UI must remain a client of the same `/v1/*` and SSE
contracts the CLI uses. Never bypass server APIs for UI-only features.

- TODO P6.1 Run creation form:
  - Add UI page `/ui/runs/new` with:
    - Workflow YAML editor (Monaco) with `agentflow workflow validate`
      schema integration.
    - File-pick for workflow + input pairs.
    - Profile selection (dev/local/production) when permitted.
    - Submit → redirect to run detail.
  - Persist last-used inputs in localStorage (no API tokens).
  - Add E2E test via `playwright` headless.

- TODO P6.2 Provider config diagnostics panel:
  - Add UI page `/ui/diagnostics` calling `agentflow doctor --output json`
    via a new `GET /v1/diagnostics` server route.
  - Render results as a per-component pass/warn/fail table.
  - Refresh button (no auto-poll).
  - Mask API keys to last 4 chars.

- TODO P6.3 Trace comparison view:
  - Add UI page `/ui/runs/{id}/compare?against={other_id}`:
    - Side-by-side event timeline.
    - Diff highlighting for tool calls and final answers.
    - Hop latency comparison.
  - Backend: extend `GET /v1/runs/{id}/events/history` to include the
    fields needed for diffing.

- TODO P6.4 Durable user preferences:
  - Add `user_preferences` table (single tenant initial):
    - `key`, `value` (JSONB), `updated_at`.
  - Add `GET /v1/preferences` / `PUT /v1/preferences`.
  - Persist UI preferences: theme, default profile, event filter,
    pagination size. Reject token-shaped values server-side.

- TODO P6.5 Operator-focused event filter:
  - Add a query bar in `/ui/runs/{id}` matching the trace replay TUI
    filter language: `kind=ToolCall AND step>5`, etc.
  - Filter is applied client-side first; server-side fallback when the
    filter expression matches a known indexed path.
  - Persist last filter per run id (links to P6.4).

---

## P7 — Performance And Release Engineering (NEW)

Goal: establish a perf baseline + release rehearsal so v1.0 ships with
known characteristics, not surprises.

- TODO P7.1 `cargo bench` baselines:
  - Add Criterion benches:
    - `agentflow-core/benches/scheduler.rs`: 10/100/1000-node DAGs,
      serial vs concurrent, p50/p95.
    - `agentflow-llm/benches/provider_hop.rs`: mock provider latency
      overhead.
    - `agentflow-rag/benches/retrieval.rs`: 1k/10k corpus BM25.
    - `agentflow-tracing/benches/event_write.rs`: JSONL vs SQLite
      throughput.
  - Check in baseline JSON in `benches/baselines/<host>.json` (note: host
    differences are expected; baselines are signals, not gates).

- TODO P7.2 CI perf regression gate:
  - Add `bench.yml` workflow that runs benches on a fixed runner.
  - Compare against the checked-in baseline.
  - Fail when median time ≥1.25× baseline.
  - Post a summary comment on PRs.

- TODO P7.3 Examples smoke test in CI (links to P3.10):
  - All examples must compile and run with mock provider in <5 min.
  - Make `examples/ecosystem/` the official entry surface.

- TODO P7.4 v1.0 release dress rehearsal:
  - Tag `v1.0.0-rc.1` from a release branch.
  - Run the full `docs/RELEASE_CHECKLIST.md`.
  - Cut a docker image from `Dockerfile`.
  - Verify `agentflow serve` boots in production profile against a real
    Postgres.
  - Verify Web UI loads in the docker image.
  - Verify `agentflow doctor --profile production` passes on a fresh
    machine.
  - Capture findings in `docs/RELEASE_NOTES_DRESS_REHEARSAL.md` and
    refile gaps as targeted tasks.

---

## P-H — Harness Agent Mode (Parallel Track, NEW)

Designed in `HARNESS_MODE_EVOLUTION.md`. Six phases H0-H6 (overall
difficulty ~5.5/10 for a practical AgentFlow-native version).

This is a **parallel track** to P1-P5, not a successor. Schedule by
prereqs below. Stable contracts (`HarnessEvent` envelope, `ApprovalRequest`
/ `ApprovalDecision`, hook traits) must land before any UI work to avoid
the "UI-first drift" risk (HARNESS_MODE_EVOLUTION Risk 5).

Architectural rules (enforced via review):

- Harness Mode MUST wrap existing `AgentRuntime`, `ToolRegistry`, Trace,
  and Skill contracts. New behavior must be additive through hooks,
  events, and session context (HARNESS_MODE_EVOLUTION Risk 1).
- Default to ask/deny for mutating tools in `local`/`production`
  profiles (Risk 2).
- Resume must honor `ToolIdempotency` and surface manual recovery
  instructions for non-idempotent calls (Risk 3, links to P1.7).
- Context providers must emit structured items with priority and token
  cost; never dump files blindly (Risk 4).
- Treat any UI as a client of the stable JSON event envelope, not the
  source of truth (Risk 5).

### Foundation — closed

- DONE P-H.0 Harness contract inventory (Phase H0):
  - `docs/HARNESS_MODE.md` is the implementation spec, promoted from
    `HARNESS_MODE_EVOLUTION.md` (the rationale doc).
  - JSON envelopes frozen in new `agentflow-harness` crate:
    - `HarnessEvent` (closed kind set: `session_started`, `step_started`,
      `tool_call_requested`, `approval_requested`, `approval_decided`,
      `tool_call_completed`, `background_task_updated`,
      `memory_summary_added`, `stopped`).
    - `ApprovalRequest` (tool name, args summary, risk classification,
      idempotency, requested_at, expires_at).
    - `ApprovalDecision` (decision, scope: once/session/run, decided_by,
      decided_at, reason).
  - Hook trait boundaries defined: `PreToolHook`, `PostToolHook`,
    `ApprovalProvider`, `ContextProvider`.
  - `agentflow-harness` shipped as a new crate (additive boundary;
    Phase H1 wires runtime execution on top).
  - Round-trip contract tests + frozen JSON fixtures under
    `agentflow-harness/tests/fixtures/`.
  - `docs/STABILITY.md` lists the new envelopes at `Experimental` tier
    with `HARNESS_ENVELOPE_SCHEMA_VERSION = harness/1`.

- DONE P-H.1 Harness runtime MVP (Phase H1):
  - `HarnessRuntime` wraps any `AgentRuntime` impl (typically
    `ReActAgent`) via `Box<dyn AgentRuntime>`; persona is assembled
    from context providers under a priority-aware token budget; the
    monotonic `seq` event stream is fanned through a `SinkChain`.
  - Four default context providers (`AgentsMdProvider`,
    `TodosMdProvider`, `RoadmapMdProvider`, `WorkspaceLayoutProvider`)
    with priority + token-cost estimates.
  - Persistence: `InMemoryEventSink`, `JsonlEventSink`,
    `StdoutEventSink`, `SinkChain` fan-out. SQLite / Postgres sinks
    stay deferred to P-H.5 alongside the server integration.
  - Tool / Skill composition only: callers supply a pre-built
    `ReActAgent` (typically from `SkillBuilder::build()`); the runtime
    never touches `ToolRegistry` directly.
  - Tracing bridge (`agentflow_harness::tracing_bridge`): honors the
    `AGENTFLOW_TRACE_DIR` convention so trace replay / TUI tooling
    can find Harness session logs without bespoke wiring. Deeper
    integration with `agentflow-tracing::TraceStorage` (one storage
    layer for both agent and Harness events) tracked under P-H.5.
  - CLI surface (`agentflow harness …`):
    - `run "<input>"` with `--skill`, `--model`, `--session`,
      `--workspace`, `--profile`, `--runtime`, `--output
      text|json|stream-json`, `--run-dir`, `--max-steps`,
      `--max-tool-calls`, `--timeout-ms`, `--no-default-context`.
      Final answer trailer prints `Session: <id>`.
    - `resume <session_id>` replays the persisted JSONL log
      (`--output text|json|stream-json`). Full memory rehydration is
      Phase H2 work because it requires a persistent `MemoryStore`
      and an idempotency-aware resume policy (P1.7).
    - `list` enumerates session logs (text + JSON formats).
    - `inspect <session_id>` summarises a session log.
  - Tests: 41 harness unit tests + 6 envelope fixtures + 1 ReAct+mock
    smoke + 9 CLI end-to-end tests (list / inspect / resume / help /
    arg validation).

### After P1.7 — Hooks And Approval

- DONE P-H.2 Hooks and approval (Phase H2):
  - New `agentflow-harness::hooks_runtime` module decorates every
    registered [`Tool`] with a `HookedTool` wrapper via
    `wrap_registry(registry, HookConfig)`. Callers build the
    `ToolRegistry` first, wrap it with hooks + approval, then pass
    `Arc::new(registry)` to `ReActAgent::new` (or any
    `AgentRuntime`). The `HookedTool` delegates metadata + capability
    surface to the inner tool and intercepts only `execute()`.
  - `PreToolHook` / `PostToolHook` invocation under a
    `HookConfig::with_hook_timeout` bound (default 5 s). Pre-hook
    timeouts and errors are fail-closed: the call is denied with a
    structured reason that names the offending hook. Post-hook
    failures are advisory (logged via `tracing::warn!`) and never
    undo the tool result.
  - Three `ApprovalProvider` implementations in
    `approval_providers`:
    - `AutoAllowApprovalProvider` (CI smoke, dev override).
    - `AutoDenyApprovalProvider` (`with_stop_on_deny` flag for the
      production fail-closed default).
    - `CliApprovalProvider` (blocking stdin prompt, scriptable via
      `with_streams(writer, reader)` for tests; honours
      `ApprovalRequest::expires_at` by racing
      `tokio::time::sleep` against `spawn_blocking` stdin).
  - Fail-closed production default: when the active
    `HarnessProfile` is `Production` and the call's
    `ToolIdempotency` is `NonIdempotent`, the wrapper escalates even
    an unanimous `Allow` to `RequireApproval` (Risk 2). The fresh
    `ApprovalRequest` carries `risk = Critical` and a reason that
    points at the production-profile escalation.
  - Approval lifecycle is fully traced through the existing
    `SinkChain`: every approval emits one
    `HarnessEvent::ApprovalRequested` followed by exactly one
    `ApprovalDecided` (including a synthetic `cached` decision when a
    prior `Session`/`Run` scope decision is reused). `DenyAndStop`
    short-circuits subsequent tool calls in the session without
    re-prompting.
  - Tests (12 new in `hooks_runtime::tests`, 9 in
    `approval_providers::tests`): allow path, pre-hook deny,
    require-approval routing, auto-deny denial, production
    escalation, Session-scope caching, Once-scope re-prompt, slow
    pre-hook timeout, provider-error treated as cancellation, deny-
    and-stop blocking, post-hook fires on success + failure, each
    `CliApprovalProvider` response parse, `ApprovalRequest::expires_at`
    deadline.

### After P3.7 — Parallel Native Tool Calls

- DONE P-H.3 Parallel tool calls (Phase H3):
  - `ReActAgent::run_with_context` adds a new batch path: when the
    LLM returns `>= 2` native tool calls in one turn, the agent
    dispatches them through `dispatch_native_tool_calls_batch`
    atomically. The previous single-call path keeps working for
    `len == 1` and for prompt-based ReAct turns.
  - **Concurrent + serial split.** Tools whose
    `ToolIdempotency::Idempotent` flag is set run concurrently via
    `futures::future::join_all`; everything else (`NonIdempotent` /
    `Unknown`) runs serially in array order. The harness
    `HookedTool` wrapper continues to gate risky calls through the
    `ApprovalProvider` flow (P-H.2), so "approval-gated" behaviour
    is composed, not duplicated.
  - **Deterministic trace ordering.** `ToolPolicyDecision`,
    `ToolCapabilityDecision`, `ToolCallStarted`, and the `ToolCall`
    step rows fire in LLM-returned order before any execution
    begins. `ToolCallCompleted` events and `ToolResult` step rows
    also follow that order, so trace replay reproduces the same
    timeline whether the wire-level completion order matched the
    LLM order or not.
  - **Pre-flight atomicity.** A batch that would push the running
    tool-call counter past `RuntimeLimits::max_tool_calls` is
    refused before any inner tool runs (stop reason
    `MaxToolCalls`). Pre-cancelled tokens short-circuit before the
    concurrent group spawns.
  - **Partial failure tolerance.** A single tool failing inside a
    batch produces a `ToolOutput::error` for that call, the other
    calls still complete, and the agent loop continues to the next
    LLM turn. A single combined reflection records the error list
    instead of emitting one reflection per failed call.
  - Tests (4 new): batch ordering + LLM-order trace; partial
    failure (1 errors, 2 succeed); pre-cancelled token returns
    `Cancelled`; `max_tool_calls=2` with a 3-call batch returns
    `MaxToolCalls` without executing any tool.
  - `futures = "0.3"` added to `agentflow-agents` dependencies.

### After P-H.0 Spec + In-Process Task Runtime Design

- DONE P-H.4 Background task tools (Phase H4):
  - New `agentflow-harness::tasks` module implements an in-process
    task runtime. Each task is a `tokio::spawn`-backed future running
    an inner `Box<dyn AgentRuntime>` produced by a caller-supplied
    `TaskAgentFactory`. The factory keeps the runtime agnostic of
    LLM config, memory backend, and tool registry.
  - `TaskHandle` captures id, prompt, status, skill, allowed tools,
    timestamps, final answer / error, captured output, and a
    `output_truncated` flag. Lifecycle:
    `Pending → Running → Completed | Failed | Cancelled` with
    `is_terminal()` short-circuit on stops + cancels.
  - Five built-in tools (`task_create`, `task_get`, `task_list`,
    `task_stop`, `task_output`) wrap the runtime as standard
    `agentflow_tools::Tool` impls. `task_tools(runtime)` helper
    returns them in a registration-ready vec.
  - Every lifecycle transition emits one
    `HarnessEvent::BackgroundTaskUpdated` through the parent
    session's `SinkChain` using the shared `seq_counter`, so child
    task events interleave deterministically with approval / tool
    events.
  - Cancellation: `task_stop` flips an `AgentCancellationToken` the
    runtime threads through `AgentContext`, so the inner agent
    aborts promptly. The runtime overrides any factory-supplied
    token so `task_stop` always wins.
  - **Nested spawn rejection.** The spawned task runs inside
    `tokio::task_local!`-scoped `IN_BACKGROUND_TASK`. `TaskRuntime
    ::create_task` returns `HarnessError::InvalidState` when called
    from inside that scope — so a task agent calling `task_create`
    again fails fast with a clear error, no runaway hierarchies.
  - Output capture: `TaskWriter::push_line` is bounded by
    `max_output_bytes` (default 64 KiB). Overflow flips
    `output_truncated` instead of failing the task. `task_output`
    accepts `tail_lines` to return only the most recent lines.
  - Tests (8 new): spawn → complete with full lifecycle event
    sequence; spawn → fail; spawn → stop yields Cancelled; nested
    spawn rejection; list filter + sort; output truncation;
    `TaskCreateTool` routes through runtime; `task_tools` helper
    name set.

### After P2.1+P2.2+P2.4 + P6 Web UI Baseline

- TODO P-H.5 Server + Web UI integration (Phase H5; ~3-5 weeks; PREREQ:
  P2.1, P2.2, P2.4, P-H.2, P6.1):
  - Server routes:
    - `POST /v1/harness/sessions`.
    - `GET /v1/harness/sessions/{id}`.
    - `POST /v1/harness/sessions/{id}:cancel`.
    - `GET /v1/harness/sessions/{id}/events` (SSE with backfill).
    - `GET /v1/harness/sessions/{id}/approvals` (pending approvals).
    - `POST /v1/harness/sessions/{id}/approvals/{request_id}` (decide).
  - DB: extend `runs` schema with `kind = workflow|harness` or add
    `harness_sessions` table (decide in P-H.0 spec).
  - Web UI:
    - `/ui/harness/sessions` list page.
    - `/ui/harness/sessions/{id}` timeline + tool call panel.
    - Approval panel with allow/deny + scope choice.
    - Session resume action.
  - Tests across CLI submit → server stream → UI render.

### Deferred to RoadMap Later Tracks

- DEFERRED P-H.H6 Advanced compatibility (Phase H6; open-ended):
  - Slash-command ecosystem expansion.
  - TUI product shell (separate from CLI run).
  - OpenHarness-style config import.
  - Plugin compatibility adapters.
  - Provider subscription bridge.
  - Promote individual H6 items to TODOs only when concretely required.

---

## M — Maintenance Tasks (NEW)

Ongoing housekeeping that should ride along with feature work but doesn't
fit a P-segment.

- DONE M.1 `CLAUDE.md` sync after worker/ui.

- TODO M.2 `docs/AGENT_SDK.md` trait-change sync:
  - Add a doc maintenance checklist: every change to `AgentRuntime`,
    `ReflectionStrategy`, `MemorySummaryBackend`, `ToolPolicy`, or
    `EventListener` traits must update the doc and the `custom_*`
    examples in the same PR.
  - Add a `cargo xtask check-agent-sdk-doc` step that greps for stale
    type names.

- TODO M.3 Test coverage gaps:
  - `agentflow-db`: currently 4 smoke tests. Add per-repo CRUD tests
    (Run/Step/Event/Artifact/SkillInstall/McpSession), tenant isolation,
    migration roundtrip on a fresh schema.
  - `agentflow-memory`: currently 16 tests. After P4.5 design, add
    backend-specific tests and cross-layer search tests.
  - Worker (P5): coverage grows with P5.5–P5.7.

- DONE M.4 Historical eval doc cleanup.

- DONE M.5 CI workflow audit (see `docs/CI_WORKFLOWS.md`).

- TODO M.6 Workspace edition pin:
  - All 15 Rust crates are on edition 2024 now. Add a `cargo xtask
    verify-edition` step that fails CI if any new crate is added at a
    different edition.

---

## Deferred / Explicit Non-Goals

These are intentionally out of the current queue. Leave extension seams
where reasonable, but do not implement product features for them yet.

- DEFERRED Channel adapters:
  - Slack, Telegram, Discord, email, webhook routers, desktop tray, and
    multi-channel message normalization are deferred.
- DEFERRED Local OS control tools:
  - Screenshot, keyboard, mouse, clipboard, and window-management tools
    are deferred until security profiles, sandboxing, audit, and
    confirmation hooks are stronger.
- DEFERRED Full SaaS productization:
  - Organization management, billing, hosted multi-user UI, OAuth/JWT,
    background Skill updates, and channel-based routing are deferred.
- DEFERRED Native dynamic library plugins:
  - Subprocess JSON-RPC remains the only v1 plugin runtime. WASM is a
    later evaluation only after subprocess is exercised by real
    third-party plugins.
- DEFERRED P-H.H6 Harness advanced compatibility:
  - Promoted to RoadMap Later Tracks; individual items return here only
    when concretely required.

---

## Execution Notes

- Pick one item at a time, expand it into concrete subtasks if not
  already enumerated, then commit code and sync this file after each
  completed feature.
- For each new task added to this file, add at least one round-trip or
  smoke test that proves the acceptance criteria.
- Prefer the smallest coherent feature per commit; resist mixing P-H
  work into a P1/P2 commit.
- When in doubt about a placement, prefer keeping it in `RoadMap.md`
  until a concrete subtask emerges.

## Quality Gates

For each task:

- Read relevant code/docs first.
- Implement the smallest coherent feature.
- Run focused tests or validation commands.
- Commit the feature with a conventional message.
- Update this TODO file only after the feature commit succeeds.
- Cross-reference the task ID in the commit body (e.g., `Refs P-H.0`).

## Cross-References

- `RoadMap.md` — forward direction; Harness Agent Mode under Later
  Tracks mirrors this file's P-H section.
- `docs/CURRENT_STATUS.md` — what currently exists.
- `docs/STABILITY.md` and `docs/API_COMPATIBILITY.md` — stable surfaces.
- `HARNESS_MODE_EVOLUTION.md` — full Harness Mode design spec.
- `PROJECT_EVALUATION_2026-05-14.md` — most recent project evaluation;
  drove the P6/P7/P-H/M segment additions.
- `PROJECT_EVALUATION_2026-05-01.md` — historical evaluation that drove
  the original P0-P5 task queue.
- `TODOs-archive-2026-05-09-n1-n10.md` and
  `TODOs-archive-2026-05-10-p0-p4.md` — completed history.
