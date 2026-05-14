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
| P-H | Harness Agent Mode (parallel track) | NEW — H0/H1 can start now |
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

- TODO P1.7 Non-idempotent tool resume policy:
  - Extend resume CLI output (`agentflow workflow run --resume <run-id>`)
    to print:
    - The list of unfinished tool calls.
    - Each call's `ToolIdempotency` classification.
    - The replay decision (`replayed` / `skipped` / `requires_manual`).
    - The reason string.
  - Add trace fields: `resume.tool_call_id`, `resume.idempotency`,
    `resume.decision`, `resume.reason`.
  - Expose the same data in `GET /v1/runs/{id}/resume-plan` server route.
  - Add tests for: idempotent call replay, non-idempotent call denial,
    `Undeclared` call denial with `--force-replay` opt-in, and resume audit
    log presence.
  - Prereq for P-H.2 (Harness hooks/approval).

- TODO P1.8 Plugin execution policy:
  - Define default plugin execution policy per security profile in
    `agentflow-tools::policy::PluginPolicy`:
    - `dev`: sandbox optional, network/file allowed by manifest.
    - `local`: sandbox required, manifest must declare permissions.
    - `production`: sandbox required, signature required, network
      explicit-allow only.
  - Require explicit `--allow-unsandboxed-plugin` CLI flag in `local`
    and `production` profiles.
  - Wire plugin policy decision into trace events.
  - Add tests for plugin spawn denial, sandbox opt-in, and signature
    rejection.
  - Document in `docs/TOOL_PERMISSIONS.md` "Plugin policy" subsection.

- TODO P1.9 MCP capability + SkillSecurity merge policy:
  - Author `docs/MCP_CAPABILITY_POLICY.md` describing how an MCP server's
    declared capabilities interact with:
    - SkillSecurity `allowed_tools` / `denied_tools`.
    - Top-level `ToolPolicy` allow/deny rules.
    - CLI `--allow-tool` / `--deny-tool` runtime overrides.
  - Implement merge resolution in `agentflow-skills::resolve_tool_policy`.
  - Add a decision precedence table: CLI > SkillSecurity > MCP server
    capability > ToolPolicy default.
  - Add tests for each precedence ordering case.
  - Surface the resolved policy in `agentflow skill inspect
    --explain-permissions` (P3.5).

---

## P2 — Local Server / Daemon Reliability

Goal: make the server a dependable local execution control plane without
turning it into a channel hub.

- TODO P2.1 `agentflow serve` command:
  - Add `agentflow-cli/src/commands/serve.rs` invoking
    `agentflow_server::run()` with config.
  - Support flags / env:
    - `--bind <host:port>` / `AGENTFLOW_SERVE_BIND` (default `127.0.0.1:8080`).
    - `--database-url` / `AGENTFLOW_DATABASE_URL`.
    - `--run-dir` / `AGENTFLOW_RUN_DIR`.
    - `--trace-dir` / `AGENTFLOW_TRACE_DIR`.
    - `--security-profile dev|local|production` (default `local`).
    - `--auth-token-env <var>` (default `AGENTFLOW_API_TOKEN`).
    - `--cors-origins <list>`.
    - `--max-body-mb`.
  - Startup diagnostics printed to stdout (without leaking secrets):
    - effective profile, bind, db reachable y/n, trace dir, run dir,
      auth token source, sandbox backend, plugin runtime status.
  - Add `agentflow serve --check` non-binding readiness mode for CI.
  - Add integration tests that start/stop the server in tests.

- TODO P2.2 Run retention and cleanup policy:
  - Add settings to `agentflow-db`:
    - `runs.retention_days` (default 30 in `local`, 90 in `production`).
    - `events.retention_days` (default 14).
    - `artifacts.retention_days` (default 30).
    - `run_dir.retention_days` (default 14).
    - Per-run override via `POST /v1/runs` body `retention_overrides`.
  - Implement `agentflow-server::cleanup::cleanup_expired()`:
    - DB sweep using `WHERE finished_at < now() - interval`.
    - Filesystem sweep over `AGENTFLOW_RUN_DIR` matching DB.
    - Never delete runs in `Running` or `Pending` state.
  - Add `agentflow cleanup --dry-run` CLI subcommand.
  - Add background cleanup task in `agentflow serve` (configurable interval,
    default 1 hour).
  - Add tests: retain active runs, delete terminal runs past TTL, dry-run
    output stability, retention override.

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
  - Diagnose:
    - Model config files reachable, syntactically valid.
    - Provider API keys present (without leaking values).
    - Feature flags compiled in.
    - MCP server reachability via configured transport.
    - Sandbox backend name + enforcement level (links to P1.6).
    - Server + DB availability when `--server` provided.
    - Plugin runtime spawn smoke (no-op plugin, ≤1s).
    - Marketplace cache readable.
    - Disk space for run_dir / trace_dir.
  - Provide both human-readable and `--output json` modes.
  - Exit code 0 / 1 / 2 reflect ok / warn / fail respectively.
  - Add a `--profile dev|local|production` flag that changes the
    pass/fail thresholds.

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

- TODO P3.7 LLM provider matrix documentation:
  - Author `docs/LLM_PROVIDERS_MATRIX.md` as the single source of truth:
    - Per-provider table: streaming, tool_calls, tool_choice modes,
      multimodal types, max context, supported model families, error code
      mapping, rate-limit handling.
    - Per-feature capability flags exposed by `ModelCapabilities`.
    - Tested-vs-best-effort badges (matching P3.6 coverage).
  - Cross-reference from `docs/CURRENT_STATUS.md` and `README.md`.
  - Add doc-test that the matrix headers stay in sync with the
    `ProviderRequest` field set.
  - PREREQ for P-H.3 (Harness parallel tool calls).

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

### Foundation — Start Now (no platform prereq)

- TODO P-H.0 Harness contract inventory (Phase H0; ~2-4 days):
  - Promote `HARNESS_MODE_EVOLUTION.md` content into
    `docs/HARNESS_MODE.md` as the implementation spec.
  - Define and freeze JSON envelopes:
    - `HarnessEvent` (kinds: `SessionStarted`, `StepStarted`,
      `ToolCallRequested`, `ApprovalRequested`, `ApprovalDecided`,
      `ToolCallCompleted`, `BackgroundTaskUpdated`, `MemorySummaryAdded`,
      `Stopped`).
    - `ApprovalRequest` (tool name, args summary, risk classification,
      idempotency, requested_at, expires_at).
    - `ApprovalDecision` (decision, scope: once/session/run, decided_by,
      decided_at, reason).
  - Define hook trait boundaries:
    - `PreToolHook`, `PostToolHook`, `ApprovalProvider`,
      `ContextProvider`.
  - Decide `agentflow-harness` as a new crate vs initial module in
    `agentflow-agents`. (Default recommendation: new crate to enforce
    additive boundary.)
  - Add round-trip contract tests for new envelopes.
  - Update `docs/STABILITY.md` with the new envelopes' stability tier
    (likely `beta` initially).

- TODO P-H.1 Harness runtime MVP (Phase H1; ~1-2 weeks; PREREQ: P-H.0):
  - Scaffold `agentflow-harness/` crate with:
    - `HarnessRuntime` wrapping `ReActAgent` via `AgentRuntime`.
    - `HarnessContext` with session_id, workspace_dir, profile, limits.
    - `HarnessEvent` emission wired to `agentflow-tracing`.
  - Implement default context providers:
    - `AgentsMdProvider` (reads `AGENTS.md` if present, with priority +
      token cost).
    - `TodosMdProvider` (reads `TODOs.md` short queue).
    - `RoadmapMdProvider` (reads `RoadMap.md` Direction section).
    - `WorkspaceLayoutProvider` (top-level dir listing).
  - Integrate with `ToolRegistry` (no new tool wiring).
  - Integrate with `SkillBuilder` for explicit Skill loading.
  - CLI entry: `agentflow harness run "..."` with `--output
    text|json|stream-json`.
  - Session id printed in final answer.
  - Persist session events to JSONL by default; SQLite/Postgres
    feature-gated.
  - Tests for the doc's "Acceptance Criteria For MVP" list
    (HARNESS_MODE_EVOLUTION L815-828).

### After P1.7 — Hooks And Approval

- TODO P-H.2 Hooks and approval (Phase H2; ~1-2 weeks; PREREQ: P-H.1,
  P1.7):
  - Hook registry inside `HarnessRuntime`.
  - `PreToolHook` / `PostToolHook` execution with bounded timeout.
  - `ApprovalProvider` trait:
    - `CliApprovalProvider` (blocking, prompt + reason).
    - `NonInteractiveAutoApprovalProvider` (auto-approve safe tools).
    - `NonInteractiveDenyApprovalProvider` (deny all risky tools).
  - Fail-closed: production profile defaults to deny for mutating tools
    without explicit policy.
  - Approval events recorded in trace (request, decision, scope).
  - Tests for: denied, allowed-once, allowed-for-session,
    cancelled-during-prompt, timeout.

### After P3.7 — Parallel Native Tool Calls

- TODO P-H.3 Parallel tool calls (Phase H3; ~1-2 weeks; PREREQ: P-H.2,
  P3.7):
  - Modify ReAct dispatch to consume `tool_calls` array atomically per
    LLM turn.
  - Concurrent execution for safe (Idempotent) tools.
  - Serial + approval-gated for risky tools.
  - Deterministic trace ordering: emit `ToolCallStarted` events in the
    LLM-returned order even when execution is concurrent.
  - Tests for mixed safe/risky batches, partial failure (one tool fails,
    others continue), cancellation mid-batch.

### After P-H.0 Spec + In-Process Task Runtime Design

- TODO P-H.4 Background task tools (Phase H4; ~2-3 weeks; PREREQ: P-H.2):
  - Implement in-process task runtime in `agentflow-harness::tasks`:
    - `TaskHandle` with id, status, output buffer, cancellation.
    - Lifecycle: `Pending → Running → Completed | Failed | Cancelled`.
  - Built-in tools for the agent to invoke:
    - `task_create(prompt, skill?, tools_allowed?)`.
    - `task_get(task_id)`.
    - `task_list(filter?)`.
    - `task_stop(task_id)`.
    - `task_output(task_id, tail_lines?)`.
  - Trace and cancellation integration with the parent session.
  - Output capture with `max_output_bytes` enforcement.
  - Tests for: spawn → complete, spawn → cancel, spawn → fail,
    nested task spawn rejection (avoid runaway hierarchy).

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

- TODO M.4 Historical eval doc cleanup:
  - Add a one-line current-status pointer to `OVERALL_EVALUATION_REPORT.md`
    (2026-04-28) similar to what `PROJECT_EVALUATION_2026-05-01.md`
    already has.
  - Decide whether to archive `IMPLEMENTATION_STATUS.md` (2025-10-25)
    given `docs/CURRENT_STATUS.md` is authoritative.

- TODO M.5 CI workflow audit:
  - Inventory all `.github/workflows/` files.
  - Ensure `feature-matrix.yml` (P3.9), `bench.yml` (P7.2),
    `examples-smoke.yml` (P3.10), `rag-eval-smoke.yml` (P4.1) exist or
    are tracked.
  - Document any flake-prone job and its retry policy.

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
