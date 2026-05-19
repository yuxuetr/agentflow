# AgentFlow TODOs

Last updated: 2026-05-19

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
| P3 | Rust SDK And CLI Experience | CLOSED (P3.4 deep probes shipped via P3.4-PR.1/.2/.3) |
| P4 | Memory, RAG, And Eval Foundations | active |
| P5 | Plugin, Marketplace, And Worker Hardening | active |
| P6 | Web UI Productization | NEW — active |
| P7 | Performance And Release Engineering | closed (P7.4-FU1..FU4 all DONE; v1.0.0-rc.1 tag is unblocked) |
| P-H | Harness Agent Mode (parallel track) | H0 + H1 + H2 + H3 + H4 closed; H5 next (gated on P2.1/P2.2/P2.4/P6.1) |
| P9 | Dogfooding-Driven Refinements (from A1+A1.5 reflection) | CLOSED (all in-repo items DONE; P9.6 + half of P9.8 are cross-project phonon work) |
| P-LLM | Modality Provider Traits + Model Schema Cleanup | CLOSED (P-LLM.0–.5 all DONE; P-LLM.6 video DEFERRED) |
| M | Maintenance Tasks | NEW — ongoing |
| Deferred | Channel adapters / OS control / SaaS | non-goal |

## Recently Closed

- P3.5 Permission explanation improvements — fully closed (slices
  1-4 + slice 2 follow-up tests for MCP / skill_agent / multi_agent
  permission output; 4 new CLI integration tests on top of the
  existing template / http / file / shell coverage).
- P7.4-FU4 Production deployment runbook
  (`docs/RELEASE_NOTES_v1.0.0-rc.1.md` DRAFT carries the six-step
  Production Deployment Checklist closing rehearsal F4; other
  release-notes sections stay placeholders for the tag-cut commit).
- P7.4-FU1 Linux sandbox CI check (new `linux-sandbox-check` job in
  `.github/workflows/quality.yml` runs `cargo check --target
  x86_64-unknown-linux-gnu -p agentflow-tools --all-targets` and is
  wired into `release-gate.needs`).
- P7.4-FU3 Box `tonic::Status` + workspace clippy sweep
  (`BoxedStatusResult` alias in `agentflow-server/src/scheduler/grpc.rs`
  + 6 tag-along pre-existing clippy cleanups across server / nodes /
  tools; `cargo clippy --workspace --all-targets -- -D warnings`
  exits 0).
- P7.4-FU2 Workspace rustfmt sweep before tag (single `chore(fmt)`
  commit cleared residual drift across 6 benches/tests/examples;
  `cargo fmt --all -- --check` exits 0 on the release branch).
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
- P2.4 SSE robustness (`EventBroker::finalise_with_grace` + `AGENTFLOW_BROKER_FINALIZE_GRACE_MS` + public diagnostics + reconnect across active / recently-completed / long-completed runs + disconnect-no-leak tests).
- P6.1 Run creation form (`/ui/runs/new` deep link + `RunCreateForm` with tenant / profile / workflow / inputs / file-pick / localStorage / submit→redirect + Playwright E2E spec).
- P-H.5 (Slice 1 of 4): Harness Mode server schema + core routes (`harness_sessions` / `harness_session_events` tables, `HarnessSessionRepo` / `HarnessEventRepo`, `HarnessSessionExecutor` trait + `StubHarnessExecutor`, `HarnessEventBroker`, six routes including SSE backfill, integration tests `tests/harness_routes.rs` self-skipping without `AGENTFLOW_DATABASE_TEST_URL`). Slices 2–4 (approval routes + real executor + Web UI + full E2E) remain TODO.
- P-H.5 (Slice 2 of 4): approval routes + LLM-backed executor (`PendingApprovalRegistry` + `ServerApprovalProvider` with timeout + drop cleanup; `GET /v1/harness/sessions/{id}/approvals` + `POST /v1/harness/sessions/{id}/approvals/{request_id}`; `LiveHarnessExecutor` wiring `HarnessRuntime` + `ReActAgent` + hook-wrapped registry + `ServerHarnessEventSink` writing through DB + broker; `agentflow serve` swaps in the live executor while tests keep the stub; integration tests gated on `AGENTFLOW_DATABASE_TEST_URL` + Moonshot E2E gated on `MOONSHOT_API_KEY`). Slices 3–4 (Web UI + full E2E render) remain TODO.
- P-H.5 (Slice 3 of 4): Harness Mode Web UI (`/ui/harness/sessions` list + `/ui/harness/sessions/new` submit form + `/ui/harness/sessions/:id` detail page with event timeline, payload pane, pending approval cards with allow / deny / deny_and_stop × once / session / run scope dropdown, and cancel button; deep-link routes wired in `ui_router`; Playwright spec `agentflow-ui/e2e/harness-sessions.spec.ts`; live Moonshot smoke verified end-to-end through every endpoint the UI consumes). Slice 4 (`POST /v1/harness/sessions/:id:resume` + full CLI→server→UI E2E render tests) remains TODO.
- P3.6 Native tool calling provider consistency tests: full `tool_choice = auto | none | required | tool` matrix per provider, 401/429/5xx coverage for every provider, Mock provider folded into the same suite, `agentflow-llm` added to the CI `test` matrix so the suite is now release-gate-blocking. 44 hermetic provider_consistency tests (19 new on top of the existing 25).
- P4.6 Memory and prompt golden tests: `agentflow-agents/tests/prompt_assembly_golden.rs` (5 tests) locks down the prompt-assembly contract — deterministic message snapshot, summary injection at budget overflow, post-compaction token budget, tool-list surfacing. Maintained the pre-existing `agent_runtime_react_trace.json` golden fixture to include the `llm_call_completed` events introduced by P4.4 follow-up step 2.
- P4.7 Memory backend implementations: new `layer.rs` defines the shared trait surface (`MemoryLayer`, `RetentionPolicy`, `PreferenceScope`, `PreferenceValue`, `PreferenceStore`, `EntityFact`, `EntityFactStore`, `SemanticMemoryStore`). `SqlitePreferenceStore` + `SqliteEntityFactStore` ship as the canonical SQLite-backed implementations; `SemanticMemory` gains the new `search_semantic` typed API (returns `(Message, f32)` scores). 37 hermetic tests (36 unit + 1 cross-layer integration) prove independence between the four layers.
- P5.4 Plugin sandbox default policy: `select_preparer(profile, force_sandbox, allow_unsandboxed)` extends the P1.8 install-time policy gate to plugin spawn time. Same per-profile defaults: `dev` → noop, `local` / `production` → OS sandbox by default. The `AGENTFLOW_ALLOW_UNSANDBOXED_PLUGIN=1` opt-out mirrors `--allow-unsandboxed-plugin`; `production` rejects the opt-out with `PreparerSelectionError::OptOutRejected` and fails the spawn before any child starts. `docs/TOOL_PERMISSIONS.md` gains a spawn-time decision table.
- P5.8 Workflow `type: plugin` first-class node syntax: validator (`agentflow workflow validate`) now parses the referenced plugin manifest and rejects unknown `node_type` references at validate time. New CLI `agentflow plugin generate-workflow-stub` emits a `type: plugin` YAML stub per declared `[[plugin.nodes]]` entry (`--node` filter, `--output` file sink, embeds absolute manifest path). 5 unit + 4 CLI integration tests.
- P5.1 Remote marketplace install handoff: `install_directory` in `agentflow-cli/src/commands/marketplace.rs` is now atomic — stage into a sibling `.installing-<pid>-<nanos>` temp dir, move any prior install aside to `.replacing-<…>`, then `fs::rename` the staged tree into place. Failures roll back to the original install. 3 new CLI integration tests cover happy / force / collision paths with explicit "no temp-dir siblings" assertions on the install root.
- P3.3 CLI JSON output audit (contract + first command migrated): new `CliJsonEnvelope<T>` (wire schema `agentflow.cli/1`) defines the canonical four-field envelope (`version`, `command`, `result`, `errors[]`). `docs/CLI_JSON_OUTPUT.md` is the contract; `docs/STABILITY.md` adds it at Stable tier. First migration: `agentflow doctor --format json-envelope` wraps `DoctorReport`. Per-command migrations for the remaining 10+ commands tracked as follow-ups in `TODOs.md`.
- P2.3 Server end-to-end run tests: `RunRepo::list_filtered(tenant, status, limit, offset)` extends the list API; `GET /v1/runs` accepts validated `?status` + `?offset` query params. New `e2e_runs.rs` integration suite (9 tests) covers pagination, status filter, before/after graph snapshots, and authenticated paths under bearer-token auth.
- M.3 agentflow-db per-repo CRUD tests (db + memory parts): grew `agentflow-db` repo tests from 2 → 12 covering every table (Run/Step/Event/Artifact/SkillInstall/McpSession/HarnessSession/HarnessEvent) plus tenant isolation and resume-mode lifecycle. Removed the racy per-test `TRUNCATE`, replaced with UUID-suffixed scope keys so re-runs are idempotent. Memory layer coverage was already shipped under P4.7.
- P2.6 Server tenant/session boundary: migration `0003_tenant_id_columns.sql` adds `tenant_id` to `events`/`artifacts`/`skill_installs` (with backfill from `runs`), bumps `skill_installs` PK to `(tenant_id, name, version)`. New `tenant.rs` ships `TenantId` extension + `extract_tenant_id` header middleware (default `"default"`). `get_run`/`cancel_run`/`get_run_graph`/`get_run_resume_plan` 404 on cross-tenant probes; `list_runs` falls back from query param to header. 6 new tenant-boundary integration tests pass alongside the existing 9 P2.3 tests.
- P2.5 CLI local-daemon mode (MVP): new `agentflow-cli/src/server_client.rs` is the single HTTP layer pointing at `agentflow-server`. `workflow run --server <url>` POSTs the YAML and polls to terminal; new `workflow list/cancel/graph` subcommands are server-only. `--auth-token`/`AGENTFLOW_API_TOKEN` and `--tenant`/`AGENTFLOW_TENANT` plumb auth + tenant headers (P2.6). 10 unit + 6 CLI integration tests cover the resolve helpers and the run/list/cancel/graph roundtrips against the test Postgres. Follow-ups: `workflow logs` SSE, skill server mode, P3.3 envelope output, --model / --execution-mode / --run-dir mapping over the wire.
- P3.8 Cross-hop OTel propagation (LLM + plugin hops): new `agentflow-tracing::context` ships the canonical `scope` / `current_traceparent` task-local helper + `TRACEPARENT_ENV` constant. Plugin spawn paths (`OsSandboxPluginPreparer` + new `NoopWithTraceparent`) inject `TRACEPARENT=<value>` into the child's env so OTel-aware plugins stitch onto the parent run. 4 unit + 3 CLI integration tests prove the contract end-to-end. `docs/TRACE_PERSISTENCE_SCHEMA.md` gains a "Hop continuity (P3.8)" table with LLM ✓ + Plugin ✓ + MCP ○ + Worker gRPC ○.
- P3.5 slice 4 MCP capability discovery: `skill inspect --explain-permissions --with-mcp-discovery` spawns each declared MCP server, groups its advertised tools into a `McpCapabilityMap`, and feeds them into `resolve_tool_policy` so MCP tools surface admission rows alongside built-ins. Off by default (spawning MCP servers is heavy). 3 new CLI integration tests.
- P3.4 doctor MCP+plugin lite installation probe: `doctor --check-installations` adds an `installations` section that walks `~/.agentflow/skills/*` and `~/.agentflow/plugins/*`, surfaces every declared MCP server command (reports `reachable` via `which`) and every plugin entrypoint (reports `entrypoint_exists`). Promotes status to Warning / Fail when any probe fails. 3 new CLI integration tests. Heavier transport-level MCP reachability + plugin `dry_run` spawn smoke stay deferred until the prerequisite manifest fields ship.
- P3.9 CLI feature flag CI matrix (closed): Quality CI `features` job grew 14 → 18 combinations by adding the agentflow-rag feature surface (`rag-no-default`, `rag-pdf`, `rag-html`, `rag-pdf-html`). `local-embeddings` intentionally not wired (pulls `ort` ONNX downloads; fragile on CI). Wishlist features that don't exist yet are still tracked as "wire in when they ship".
- P3.1 SDK example matrix: new top-level `examples/README.md` is the canonical 12-row matrix index. Audit found 11/12 rows already shipped; the gap (tool policy + sandbox capability decision) is filled by the new `agentflow-tools/examples/tool_policy_sandbox_demo.rs`. All examples compile under their respective feature sets.
- P3.2 + P3.10 + P7.3 Examples smoke CI (closed jointly): new `cargo xtask examples-smoke` subcommand runs 7 representative examples through `cargo run` with per-example wall-clock caps; total budget pinned at 5 min (P3.10 spec). Quality CI `examples` job invokes it with a 10-min job timeout. 3 new xtask unit tests lock down the list shape + budget invariants.
- P7.2 CI perf regression gate (MVP): new `cargo xtask bench-gate` compares `target/criterion/*/new/estimates.json` against `benches/baselines/<host>.json` and exits non-zero when any bench is ≥ 1.25× baseline. New `.github/workflows/bench.yml` runs the four Criterion suites on perf-sensitive PRs + main pushes and invokes the gate. `--allow-missing` lets the job pass until a per-runner `ci-ubuntu-latest.json` baseline lands. 5 new xtask unit tests cover the comparator paths.
- P6.4 Durable user preferences: migration `0004_user_preferences.sql` + `UserPreferenceRepo` (upsert / upsert_many / list / delete) + new `GET`/`PUT /v1/preferences` routes scoped to `X-Agentflow-Tenant`. Server-side rejection of token-shaped values (Bearer-prefixed / `sk-`/`ghp_` API-key prefixes / long hex digests / opaque alphanumeric secrets). 3 unit + 5 integration tests cover happy round-trip, tenant isolation, and rejection paths. UI wiring is the next slice.
- P6.5 Operator event filter (client-side): tiny expression language in `agentflow-ui/src/eventFilter.ts` (`kind=` / `kind!=` / `kind~` / `step` ops + `AND` chaining); filter input above the run-detail timeline persists per `run_id` in localStorage; 18 self-test assertions in `eventFilter.test.ts`. Server-side `?filter=` pre-filter + P6.4 preferences sync tracked as follow-ups.
- P6.3 Trace comparison view: new `RunCompare` component at `/ui/runs/:id/compare?against=<other>`. Two columns fetch `/v1/runs/{id}/events/history` independently; events keyed by `kind#step_index` get green-border `matched` styling, unmatched events get amber-border `only here` tag. Summary cards show event count / tool-call count / total + mean hop latency / final answer per run. No backend schema change needed.
- P-H.5 (Slice 4 of 4 — completes P-H.5): `POST /v1/harness/sessions/{id}:resume` (rerun semantic: wipe events, flip row to running, respawn executor; `post_harness_session_action` dispatches `:cancel` / `:resume` on the shared POST route; `HarnessSessionRepo::reset_for_resume` Pg txn); UI detail page switches to `EventSource` SSE with history-poll fallback + stream pill + "Resume (rerun)" button gated on terminal status; `tests/harness_full_stack_e2e.rs` exercises submit → SSE stream → DB history → terminal row → resume → rerun history in one ~6.5s pass against real Postgres + Moonshot. P-H.5 closed.
- P3.5 (Slice 1 of 4): `agentflow skill inspect --explain-permissions` now prints the P1.9 admission table alongside the existing capability decisions; new repeatable `--allow-tool` / `--deny-tool` CLI flags feed the CLI override layer (highest precedence); hint message when the flags are passed without `--explain-permissions`; 5 new CLI integration tests in `skill_cli_tests.rs` lock down the precedence rules. Slices 2–4 (sandbox profile + MCP capability discovery + `workflow validate --explain-permissions`) remain TODO.
- P3.5 (Slice 2 of 4): `agentflow workflow validate --explain-permissions <yaml>` walks `FlowDefinitionV2` and emits a per-node permission report (nine `PermissionCategory` variants, required capability list, declared constraint parameters, and "permissive: no …" notes for missing allowlists). `--format json` extends the existing envelope with a `permissions` object. 4 new CLI tests in `workflow_tests.rs` lock down text output, JSON envelope, off-by-default behaviour, and the shell-node capability surface. Slices 3–4 (sandbox profile + MCP capability discovery in `skill inspect`) remain TODO.
- M.6 Workspace edition pin: new `xtask/` workspace member + `cargo xtask verify-edition` subcommand walks every member's `Cargo.toml` and asserts `edition = "2024"`. `.cargo/config.toml` ships the `xtask` alias; Quality CI workflow gains a `verify-edition` job listed under `release-gate.needs`. Tests: 3 unit (synthetic workspace) + 3 integration (real workspace + bad subcommand).
- P3.5 (Slice 3 of 4): `skill inspect --explain-permissions` now prints a `Sandbox profile:` block that surfaces the detected platform backend (`sandbox-exec` / `seccomp` / `noop`), the tri-state `SandboxEnforcement` level, the manifest's `security.os_sandbox` opt-in, and operator notes for suspicious combinations (shell/script tools without opt-in on enforcing platforms; opt-in without an enforcing backend; opt-in without any sandboxable tool). 2 new CLI tests in `skill_cli_tests.rs` lock down the rust_expert opt-out path and the mcp-basic clean path. Slice 4 (MCP capability discovery wiring in `skill inspect`) remains TODO.
- P3.9 (partial): Quality CI `features` job expanded from 6 to 12 combinations (cli-no-default, cli-mcp-rag-plugin, cli-all-features, tracing-postgres, mcp-all-transports, nodes-default added alongside the six existing rows). Each row was validated locally with `cargo check` before landing. Two combinations from the wishlist were found broken at HEAD and tracked under the new M.7 entry instead of being wired in as failing CI jobs.
- M.2 `docs/AGENT_SDK.md` trait-change sync: new `cargo xtask check-agent-sdk-doc` subcommand walks every backtick-quoted CamelCase identifier in `docs/AGENT_SDK.md` and asserts a `pub (trait|struct|enum|type|fn) Ident` declaration exists under any `agentflow-*/src/**/*.rs`. Allowlist covers known non-types. Quality CI gains a `check-agent-sdk-doc` job listed in `release-gate.needs`. Tests: 5 unit + 1 integration.
- P2.7 Backup/restore expectations: `docs/SERVER_BACKUP_RESTORE.md` documents the four state surfaces, restore sequencing, and per-profile exit codes for `agentflow doctor --backup-check`. New `--backup-check` flag adds a writability probe for run_dir / trace_dir / marketplace_cache / skills_dir / plugins_dir (the last two are new env overrides `AGENTFLOW_SKILLS_DIR` / `AGENTFLOW_PLUGINS_DIR`). Production profile escalates missing dirs to Fail; non-writable always Fails. 5 new CLI tests in `doctor_cli_tests.rs`.
- M.7 Fix broken minimal feature combinations: `agentflow-llm --no-default-features --features openai` and `agentflow-nodes --features batch,conditional` now compile + test clean. Root causes: optional `tracing` dep (llm) and stale unit-struct constructor references in the `factories` module (nodes); secondary bugs in `conditional.rs` (stale `FlowValue::String` arm) and `batch.rs` (Debug derive on trait object + serialization mis-shape). The `factories` feature + module was deleted (unused, never compiled). CI Quality `features` matrix gains `llm-openai-only` and `nodes-batch-conditional` rows.
- P4.5 Memory layering design: `docs/MEMORY_LAYERING.md` defines the four-layer boundary (Session / Semantic / Preference / Entity facts) with per-layer lifetime, key, retention default, and the prompt-assembly precedence order. Spec'd trait extensions (`SemanticMemoryStore` extends `MemoryStore`; `PreferenceStore` + `EntityFactStore` separate) keep the new code from leaking into existing backends. Migration path is additive — current `SessionMemory` / `SqliteMemory` / `SemanticMemory` keep working without changes. Unblocks P4.7 implementation and P-H.4 background task context.
- P4.3 Agent eval format design: `docs/AGENT_EVAL_FORMAT.md` defines the v1 on-disk format for `agentflow eval run` and the JSON report envelope. JSONL+TOML layout mirrors the existing RAG eval. EvalCase fields are grounded in real `agentflow_agents::RuntimeLimits` types; six-variant closed assertion DSL (`contains` / `regex` / `tool_called` / `tool_not_called` / `step_count_below` / `final_answer_matches_skill`). Runner is one `Flow` with one `EvalCaseNode` per case — reuses concurrent scheduling, checkpoints, OTel propagation. Unblocks P4.4 implementation.
- P4.4 Minimal agent eval implementation (3 slices): `agentflow-agents::eval` module ships `Dataset` / `Assertion` / `EvalRunner` + `AgentRuntimeFactory` trait + `EvalReport` envelope; new `AgentStopReason::CostLimitExceeded` variant flows through every workspace match site. `agentflow eval run <dataset>` CLI with `--format text|json`, `--filter <glob>`, `--fail-on-status failed|never`. Tiny `ci_offline` fixture drives the bare ReActAgent against the mock provider so the suite is hermetic. 33 unit tests in agentflow-agents + 10 unit/integration tests in agentflow-cli. Cost tracking is plumbed (cost_usd_actual = 0.0 until the LLM providers report it). Trace ids are `eval-<case_id>-<epoch hex>` so `agentflow trace replay` consumes them directly.
- P4.4 follow-up trio (3 commits): (1) skill-aware factory + tool admission via new `SkillBuilder::build_with_admission` — `case.skill` cases now route through full skill loading with `tools_allowed/denied` filtering the registry pre-run. (2) real cost tracking via new `AgentEvent::LlmCallCompleted` + `PricingTable` (loadable from `AGENTFLOW_PRICING_TABLE` env or `~/.agentflow/pricing.yml`) — `cost_usd_actual` now reflects real per-call token usage × per-model rates, and `case.cost_limit_usd` is actually enforced. (3) `docs/SKILL_VALIDATOR_PROTOCOL.md` defines the v1 `[validation]` manifest section that backs the `final_answer_matches_skill` assertion.
- SKILL_VALIDATOR_PROTOCOL implementation (2 commits): `agentflow-skills::validator` ships `SkillValidator` trait + `RegexValidator` + `CommandValidator` + `build_validator` factory; manifest gains `[validation] kind = "none" | "regex" | "command"`. `SkillLoader::validate` pre-compiles validators so bad regex / empty command surfaces at load time. Assertion-layer closure return promoted to `SkillValidatorVerdict { Pass | Fail{reason} | Unrunnable{reason} }`. CLI eval factory caches per-skill validators and wires them through `skill_validator(case)`. Tests: 22 new (16 unit in skills + 3 unit in assertions + 3 CLI integration). `final_answer_matches_skill` Just Works end-to-end against skills with `[validation]`.
- P4.1 RAG eval CI fixture: `agentflow-rag/eval_datasets/ci_offline/` ships a 20-doc synthetic CC0 corpus + 10 queries + qrels; `agentflow-cli/tests/rag_eval_cli_tests.rs` (gated on `rag`) drives the CLI end-to-end and locks the JSON envelope shape downstream consumers depend on, with a Recall@5 ≥ 0.8 sanity gate. New `rag-eval-smoke` Quality job listed in `release-gate.needs` so schema or quality regressions fail the gate. Today: BM25 Recall@5=1.0, MRR=1.0, p95 latency ~0.1ms.
- P4.2 RAG eval baseline snapshots: `agentflow-rag/eval_baselines/ci_offline/bm25.json` is the checked-in baseline. `ComparisonReport.paired_sign_p_value` adds a real one-tailed binomial p-value computed in log-space. CLI gains `--compare-baseline <path>` + tunable `--regression-recall-threshold` (default 0.03) + `--regression-p-value` (default 0.05); BOTH must trip jointly to fail. CI's rag-eval-smoke now runs schema tests + gate-logic unit tests + live baseline comparison. 16 new tests total (6 p-value math + 7 gate-logic + 3 CLI integration).

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

- DONE P2.3 Server end-to-end run tests:
  - The core happy / cancel / 4xx / mid-run-graph paths were already
    covered by `agentflow-server/tests/runs_routes.rs` (12 tests).
    This slice closes the remaining cells the P2.3 spec calls out.
  - Feature additions (not just tests):
    - `RunRepo::list_filtered(tenant, status, limit, offset)` is the
      new repo entry point; the legacy `list` is now a default
      shim that delegates with `status = None, offset = 0`.
    - `GET /v1/runs` accepts `?status=` (validated against the
      closed `RunStatus` set — typos surface as 400 with the bad
      value echoed in the error message) and `?offset=` (clamped
      to ≥ 0) alongside the existing `?tenant_id` / `?limit`.
  - New `agentflow-server/tests/e2e_runs.rs` (9 tests):
    - `list_runs_offset_pagination_returns_disjoint_pages` — two
      adjacent pages share no ids.
    - `list_runs_status_filter_isolates_running_rows` — `?status=running`
      hides queued + failed rows.
    - `list_runs_rejects_unknown_status_value` — `?status=invented_state`
      → 400 with the bad value embedded in the error.
    - `list_runs_offset_beyond_total_returns_empty_page` — past-end
      offset returns `[]`, not an error.
    - `get_run_graph_returns_snapshot_before_any_events` — pre-run
      graph renders with no `active_node`.
    - `get_run_graph_returns_snapshot_after_run_completes` — post-
      terminal graph still surfaces the last-touched node as
      `active_node` (documenting the current "last-touched, not
      currently-running" semantic).
    - `submit_run_without_token_is_rejected_under_auth` — auth
      gate fires before the run handler.
    - `submit_run_with_token_succeeds_under_auth` — happy-path
      submit under bearer-token auth.
    - `health_route_stays_open_under_auth` — `/health` keeps
      working for orchestrators without a token.
  - All 9 new tests + 12 pre-existing `runs_routes.rs` tests pass
    against `AGENTFLOW_DATABASE_TEST_URL`; self-skip without it.

- DONE P2.4 SSE robustness:
  - `EventBroker::finalise_with_grace(run_id, grace)` spawns a
    deferred teardown so subscribers can drain the terminal event
    from the broadcast buffer before the channel is removed.
  - `broker_finalize_grace()` reads
    `AGENTFLOW_BROKER_FINALIZE_GRACE_MS` (default 500 ms) so
    operators can tune the window without redeploying.
  - Every call site that previously did `broker.finalise(run_id)`
    inside `runs.rs` (stub executor success, real flow executor
    success, executor error path, `cancel_run`) now goes through
    `finalise_with_grace(id, broker_finalize_grace())`. The bare
    `finalise` API is preserved for cases that need immediate
    teardown (tests, explicit error short-circuits).
  - `EventBroker::active_runs()` and `EventBroker::receiver_count()`
    are now public so operational diagnostics and integration tests
    can observe broker state without poking at `Mutex` internals.
  - Tests:
    - 10 unit tests in `events_stream::tests` (3 new for grace
      behaviour + receiver-count + disconnect isolation + env
      transitions). All hermetic.
    - 5 integration tests in `tests/sse_robustness.rs` cover
      reconnect against a recently-completed run, reconnect
      against a long-completed run that lost the broker entry,
      `after_seq` above max returns empty, SSE 404 for unknown
      run, and the disconnect-mid-stream path that asserts the
      broker drops the receiver count via the now-public
      `receiver_count()` accessor. They self-skip without
      `AGENTFLOW_DATABASE_TEST_URL`.

- DONE P2.5 CLI local-daemon mode (MVP — run/list/cancel/graph
  shipped; logs/skill remain follow-ups):
  - New `agentflow-cli/src/server_client.rs` is the single HTTP layer
    pointing at `agentflow-server`. Resolves `--server <url>` first,
    `AGENTFLOW_SERVER_URL` env second; returns `None` to fall back to
    the in-process executor. `--auth-token` /
    `AGENTFLOW_API_TOKEN` populate the `Authorization: Bearer` header;
    `--tenant` / `AGENTFLOW_TENANT` populate `X-Agentflow-Tenant` (P2.6).
    `reqwest::Client::builder().no_proxy()` avoids the macOS
    Clash/V2Ray loopback footgun documented in `CLAUDE.md`.
  - `workflow run` keeps its existing in-process path as the default;
    when `--server` is set, the workflow body is read from the file
    and POSTed to `/v1/runs`, then polled to terminal status.
  - New subcommands `workflow list`, `workflow cancel <run_id>`,
    `workflow graph <run_id>` — server-only, return a friendly error
    when `--server` / `AGENTFLOW_SERVER_URL` is absent.
  - 10 unit tests cover the pure resolve_* helpers (flag/env
    precedence, trimming, blank handling); 6 CLI integration tests in
    `agentflow-cli/tests/cli_server_mode.rs` spin up an in-process
    `agentflow-server` against `AGENTFLOW_DATABASE_TEST_URL` and
    exercise the run/list/cancel/graph roundtrips end-to-end. Tests
    self-skip without the Postgres URL.
  - Follow-ups left for a separate slice:
    - `workflow logs` (SSE event stream + history backfill).
    - `skill run` / `skill list` server-capable paths.
    - `--output json-envelope` mode (P3.3 envelope) on the new
      server-mode commands — today they print the raw `serde_json::Value`.
    - Per-run knobs (--model, --execution-mode, --run-dir, --watch,
      --output sink, key/value --input pairs) are local-only when
      `--server` is set; they need server-side mapping to take effect.

- DONE P2.6 Server tenant/session boundary:
  - New migration `0003_tenant_id_columns.sql` adds
    `tenant_id TEXT NOT NULL DEFAULT 'default'` to `events`,
    `artifacts`, and `skill_installs` (the three tables that didn't
    already have it). Backfills `events.tenant_id` /
    `artifacts.tenant_id` from the owning `runs.tenant_id` so
    historical rows surface under the correct scope after migration.
    `skill_installs` primary key is dropped and re-created as
    `(tenant_id, name, version)` so two tenants can install the same
    skill at the same version independently.
  - `agentflow_db::models` (`Event` / `Artifact` / `SkillInstall`) +
    `NewEvent` / `NewArtifact` gain `tenant_id` fields; the `New*`
    structs accept `Option<String>` so existing callers stay terse
    (defaults to `"default"`).
  - Postgres repos write the new column on INSERT/UPSERT and read
    it on SELECT; the `(tenant_id, run_id, seq)` and
    `(tenant_id, run_id)` composite indexes back the WHERE-by-tenant
    filter path.
  - New `agentflow-server/src/tenant.rs` introduces `TenantId`
    extension + `extract_tenant_id` middleware reading the
    `X-Agentflow-Tenant` header (default `"default"` for zero-config
    local-dev). Layered onto `/v1/*` in `create_router`.
  - `get_run` / `cancel_run` / `get_run_graph` / `get_run_resume_plan`
    extract the `TenantId` and return 404 (not 403) when the run's
    `tenant_id` mismatches — hides existence under cross-tenant probes.
  - `list_runs` prefers the explicit `?tenant_id=` query param when
    present (backward-compat with existing dashboards); otherwise
    falls back to the header-bound tenant.
  - `RunContext` gains `tenant_id: String` so the stub + Flow
    executors stamp every persisted event under the correct scope;
    `WorkflowEventListener::new` / `from_state` require it.
  - 6 new CLI integration tests in `e2e_runs.rs` cover the
    cross-tenant 404 path for read + cancel, header-bound success
    path, header-vs-query precedence, header-absent → "default", and
    list-via-header scoping. All 15 e2e_runs + 12 runs_routes tests
    pass against `AGENTFLOW_DATABASE_TEST_URL`.
  - Test infrastructure: `fresh_state()` no longer TRUNCATEs (matched
    the M.3 cleanup); pre-existing P2.3 tests were updated to use
    per-invocation UUID-suffixed tenants so the TRUNCATE removal
    doesn't make them flaky.

- DONE P2.7 Backup/restore expectations:
  - DONE: `docs/SERVER_BACKUP_RESTORE.md` documents the four state
    surfaces (Postgres + run artifacts + trace storage + marketplace
    cache / skills / plugins), the strict restore sequencing (DB
    before filesystem so the P2.2 cleanup sweep doesn't reap orphan
    artifact trees), and the per-profile exit code semantics for
    `agentflow doctor --backup-check`.
  - DONE: `agentflow doctor --backup-check` flag adds a `backup_check`
    section to the doctor report with explicit writability probes for
    `run_dir`, `trace_dir`, `marketplace_cache`, `skills_dir`,
    `plugins_dir`. Path resolution honors new `AGENTFLOW_SKILLS_DIR`
    and `AGENTFLOW_PLUGINS_DIR` env overrides. Production profile
    escalates missing dirs to Fail (exit 2); local / dev escalate to
    Warning. Non-writable always escalates to Fail.
  - DONE: "First stable release validation checklist" section in the
    new doc enumerates the manual gates the v1.0 release dress
    rehearsal (P7.4) runs against a freshly provisioned host.
  - Tests: 5 new in `doctor_cli_tests.rs` (section omitted by default,
    pre-created HOME passes, production + missing dirs → fail, text
    output renders the section header, env overrides for skills /
    plugins are honored).

- DONE P2.8 Worker LLM/HTTP/MCP/Agent node execution support:
  - PREREQ for the rest of P5 worker hardening (P5.5 admission, P5.6
    resource limits, P5.7 failure-domain matrix).
  - DONE: `agentflow-worker::execute_supported_node_payload` now
    dispatches `llm` / `http` / `mcp` / `agent` in addition to the
    existing `template` / `file` / `mock` types. Each `llm` / `http` /
    `mcp` payload routes through the same `agentflow-nodes` builders
    the local scheduler uses (`LlmNode`, `HttpNode`, `MCPNode`); the
    distributed `agent` dispatcher runs a minimal `ReActAgent` loop
    with `SessionMemory::default_window()` and an empty `ToolRegistry`.
    `agentflow-worker` now depends on `agentflow-llm`, `agentflow-agents`,
    `agentflow-memory`, `agentflow-tools`, and `agentflow-nodes` with
    the `mcp` feature enabled.
  - DONE: tests. Three new integration files under
    `agentflow-worker/tests/`:
    - `dispatch_simple.rs`: HTTP / MCP / unsupported-type routing
      (verifies the dispatcher selects the right executor and that
      unknown node types return a non-retryable
      `FlowDefinitionError`).
    - `dispatch_llm_and_agent.rs`: LLM happy path against the mock
      provider, plus the minimal agent ReAct loop driven by a queued
      mock response. Tests serialize on a `tokio::sync::Mutex` gate
      because `AGENTFLOW_MOCK_RESPONSES` / `AGENTFLOW_MODELS_CONFIG` /
      the LLM registry are all process-globals.
  - DONE: docs. `docs/DISTRIBUTED.md` carries the canonical supported
    node-type table with test cross-references, and the
    `agentflow-worker` crate-level rustdoc carries the matching short
    list (no separate README needed yet).
  - Deferred follow-ups (tracked under the prereq chain, NOT this
    line):
    - `traceparent`/run_id/step_id/tenant_id propagation in gRPC
      metadata: scoped to P2.7 (server transport hardening) and
      P5.5 (worker admission). The local scheduler smokes already
      stitch traces inside the process boundary.
    - Per-node-type resource limits (timeout, retry budget,
      max-output-bytes): scoped to P5.6.
    - Real MCP stdio server fixture for the worker happy-path smoke:
      scoped to P5.7 (failure-domain matrix) so the fixture
      decision lives next to the network-partition / heartbeat-loss
      cases that need it.

---

## P3 — Rust SDK And CLI Experience

Goal: make code-first and CLI-first usage clear, stable, and automation-ready.

- DONE P3.1 SDK example matrix:
  - New top-level `examples/README.md` is the canonical 12-row matrix
    index. Each row maps the spec capability to the runnable file
    (cross-crate links) and notes which crates / features it lives
    under. Operators read this once instead of grepping the workspace.
  - Audit found 11/12 rows already shipped across per-crate
    `examples/` dirs (DAG / AgentNode / ReAct / PlanExecute /
    multi-agent ×3 / SkillBuilder / MCP client / RAG / tracing
    JSONL). The one gap was the "tool policy + sandbox capability
    decision" row.
  - New `agentflow-tools/examples/tool_policy_sandbox_demo.rs` fills
    the gap: walks through tool registration → `ToolPolicy::evaluate`
    (allow_tools and allow_permissions paths) → `SandboxPolicy`
    runtime constraints. Runs fully offline; never spawns a real
    shell or HTTP request. `cargo run -p agentflow-tools --example
    tool_policy_sandbox_demo`.
  - All workspace examples compile under their owning crate's
    default + relevant feature set (`cargo check --workspace
    --examples` is clean). Per-flag combinations are covered by
    the Quality CI `features` matrix (P3.9).
  - Documented `AGENTFLOW_LIVE_PROVIDER=1` convention in the README.
  - Follow-ups (not blocking):
    - Dedicated OTel exporter example (today the JSONL example
      covers the main path; OTel is exercised via the
      `trace_context_propagation` test in `agentflow-llm/tests/`).
    - Rust example invoking `agentflow_agents::eval::EvalRunner`
      directly (today the CLI is the canonical eval entry point).
    - Per-example smoke CI lands under P3.2 / P3.10 / P7.3.

- DONE P3.2 Official example smoke tests:
  - New `cargo xtask examples-smoke` subcommand runs every entry in
    the workspace's explicit `SMOKE_EXAMPLES` list (7 entries today:
    tool_policy_sandbox_demo / simple_tracing / fixed_dag_workflow /
    agent_native_react / plan_execute_agent / hybrid_workflow_agent
    / skill_calls_mcp_tool). Each runs through `cargo run -p <pkg>
    --example <name>` with a per-example wall-clock cap; total
    budget is capped at 5 min (P3.10 spec). Failing examples surface
    with a context-rich error.
  - Local invocation: `cargo xtask examples-smoke` runs the same
    list a contributor would hit in CI; ~42 s wall clock on my M2.
  - 3 new unit tests in xtask lock down: smoke list non-empty +
    unique, total budget pinned at 5 min, per-example caps fit
    inside the total budget.

- DONE P3.3 CLI JSON output audit (contract + first command migrated;
  per-command migration tracked as follow-ups below):
  - `agentflow-cli/src/json_envelope.rs` defines the canonical
    envelope `CliJsonEnvelope<T>` with the closed four-field shape
    documented in the spec: `version` (`"agentflow.cli/1"`) +
    `command` + `result` + `errors[]` (never null, defaults to
    empty on read). 5 unit tests cover ok/with_errors round trips,
    the closed-key set, `serde(default)` for `errors`, and the
    pinned wire-version constant.
  - `docs/CLI_JSON_OUTPUT.md` is the authoritative contract:
    envelope shape, producer/consumer rules, P0.3 additive-field
    inheritance for per-command `result`, the per-command coverage
    matrix (which modes are migrated vs. planned), and the
    `agentflow.cli/N` versioning policy.
  - `docs/STABILITY.md` gains a new "CLI JSON envelope" row at
    Stable tier, with the wire schema name and a pointer back to
    `docs/CLI_JSON_OUTPUT.md` for the field contract.
  - First command migration: `agentflow doctor --format json-envelope`
    wraps the existing `DoctorReport` in the envelope. The legacy
    `--format json` (bare report) stays for backward compat with
    the in-process `/v1/diagnostics` handler and CI tooling already
    parsing the raw shape; it migrates in v1.0. 2 new CLI tests in
    `doctor_cli_tests.rs` lock the envelope shape down end-to-end
    (`doctor_json_envelope_wraps_report_in_canonical_envelope` +
    `doctor_json_envelope_field_set_is_closed_to_four_keys`).
  - Per-command migration follow-ups (each lands as its own PR):
    - DONE `workflow validate` — `--format json-envelope` wraps the
      same JSON body the legacy `--format json` emits; failed
      validation populates `errors[]` with the schema-failure
      summary so shell consumers can branch without walking
      `result.issues[]`.
    - DONE `workflow resume-plan` — `--format json-envelope` wraps
      the `ResumePlan`; manual-recovery cases land in `errors[]`
      with the operator-actionable "re-run with --force-replay
      after confirming…" message that the text path prints.
    - DONE `eval run` — `--format json-envelope` wraps the
      `EvalReport`; each failed case surfaces in `errors[]` as
      `"case '<id>' failed: <reason>"` (runtime_error first, then
      joined assertion reasons). 8 new CLI integration tests in
      `agentflow-cli/tests/json_envelope_migration_tests.rs` lock
      the envelope shape down and prove `result == legacy json
      body` on a hermetic workflow fixture.
    - DONE `harness run|list|inspect|resume` — all four
      subcommands accept `--output json-envelope` (added on top of
      the existing `text | json | stream-json` set). `stream-json`
      keeps emitting raw events per line (envelope wrapping per
      line would defeat the stream framing operators rely on for
      live tailing); `json-envelope` wraps the same summary the
      `json` mode emits in the canonical `CliJsonEnvelope`.
      - `run` envelope: `result` = full run summary (session_id,
        answer, stop_reason, final_event_seq, context items
        admitted/dropped, model, skill, session_log_path,
        elapsed_ms). Non-success stop reason populates `errors[]`.
      - `list` envelope: `result.sessions[]` carries
        `{session_id, event_count, size_bytes, modified_secs_epoch}`
        per persisted log; sorted by mtime desc.
      - `inspect` envelope: `result` = summariser output
        (session_id, event_count, counts_by_kind, session_metadata?,
        stop_reason?, final_answer?).
      - `resume` envelope: `result` = `{session_id, event_count,
        events[]}` — full event log inside the envelope.
      Shared `OutputFormat::JsonEnvelope` variant in
      `commands/harness/mod.rs` so all 4 dispatchers parse the
      same wire string. 9 new CLI integration tests in
      `json_envelope_migration_tests.rs`: full-shape round-trip
      for list / inspect / resume against hermetic JSONL session
      fixtures (`harness/sessions/<id>.jsonl`), help-surface
      guards for all 4 subcommands, value-parser rejects unknown
      formats.
    - DONE `llm models` — gained `--format text|json-envelope`
      (first machine-readable surface for the command; text mode
      unchanged). `result` body: `{ source, source_kind,
      provider_filter, models: [{ name, vendor, model_id,
      base_url?, temperature?, max_tokens?, supports_streaming }],
      total }` — mirrors the detailed text view. Mutually
      exclusive with `--refresh-from-api` for now; the
      refresh-diff JSON shape is a separate follow-up. 3 new CLI
      integration tests, including a full round-trip against the
      bundled `default_models.yml` that proves the envelope
      contains a non-empty `models[]` with `name`/`vendor`/
      `model_id` on each entry.
    - DONE `mcp list-tools|list-resources|call-tool` — each gained
      a `--format text|json-envelope` flag. text mode preserves the
      colored progress / table output operators expect; envelope
      mode suppresses progress lines (so stdout is parseable JSON)
      and emits `{ version, command, result, errors }` with the
      full structured payload (`tools[]` / `resources[]` /
      `{tool,params,result}`). `mcp call-tool --output <path>` in
      envelope mode writes the envelope-wrapped file so the on-disk
      artifact is self-describing. 4 new CLI integration tests:
      help-surface lists `json-envelope` for all 3 subcommands,
      value-parser rejects unknown formats.
    - DONE `plugin list` + `inspect` + `install` + `uninstall` +
      `generate-workflow-stub` — all five plugin subcommands accept
      `--format text|json-envelope`. Write-side payloads:
      - `install` result: `{name, version, source, destination,
        manifest_path, entrypoint, nodes[], policy: {profile,
        allowed, sandbox_active, signature_checked,
        network_policy}}` — includes the P1.8 plugin-policy decision
        so audit consumers can trace which profile gated the
        install.
      - `uninstall` result: `{name, plugins_dir, target, removed,
        reason}`. `reason` distinguishes `"removed"` from
        `"not_installed_force_acked"` so shell tooling can branch
        on the `--force` short-circuit without parsing stdout text.
      - `generate-workflow-stub` result: `{plugin, manifest_path,
        selected_node_types[], output_path?, stub?}`. When
        `--output <path>` is set, the file gets the raw YAML stub
        (unchanged) and the envelope carries `output_path` with
        `stub: null`. Without `--output`, the envelope inlines the
        stub as a string for `jq -r '.result.stub'` extraction.
      6 new CLI integration tests (plugin feature-gated): install
      round-trip + uninstall happy / force-on-missing /
      generate-workflow-stub stdout-inline / generate-workflow-stub
      file-output + 3 help-surface guards.
      `list`
      result: `{ plugins_dir, plugins: [{ name, version, runtime,
      entrypoint, entrypoint_exists, nodes, capabilities: {fs/net/
      proc/env_vars arrays}, install_dir, manifest_valid,
      manifest_error? }], total }` — capability arrays are full
      (not just counts) so JSON consumers can answer "which plugin
      writes /tmp" without re-reading manifests. `inspect` result:
      full `PluginManifest` + `resolved_entrypoint` (absolute) +
      `entrypoint_exists` + `entrypoint_executable` +
      `manifest_valid` + `manifest_error?`. Validation failures
      surface in `errors[]` for both commands. 4 new CLI
      integration tests (plugin feature-gated): full-shape round-
      trip for both subcommands + help-surface guards.
    - `rag search|eval` — wrap existing JSON outputs.
    - DONE (partial) `trace replay` — gained `--format
      text|json-envelope`. The legacy `--json` flag (append raw JSON
      after the text replay) is preserved in text mode; in envelope
      mode it's ignored since the envelope already carries the full
      (redacted) `ExecutionTrace` as `result`. `trace tui` stays
      interactive-only (no envelope). `list` / `show` subcommands
      mentioned in the original TODO don't exist on the trace
      command surface today — descoped from this segment.
      5 new CLI integration tests: full-shape round-trip via a
      hand-crafted JSON trace fixture, `--json` legacy flag silent
      ignore in envelope mode, default-text regression guard,
      help-surface + value-parser guards.
    - `workflow run|list|cancel|graph|logs` — server-backed,
      depends on P2.5 `--server` plumbing.

- DONE P3.4 `agentflow doctor` expansion:
  Library/CLI structural surface + deeper provider probes all closed.
  PR.1 (plugin dry_run runner), PR.2 (mcp.toml + `agentflow mcp config`
  CLI), and PR.3 (doctor wiring) shipped in three commits. Subtasks:
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
  - DONE (lite) `--check-installations` flag adds an
    `installations` section to the doctor report: walks
    `~/.agentflow/skills/*/skill.toml`, surfaces every declared
    `[[mcp_servers]]` command and reports `reachable = true/false`
    based on whether the command resolves on PATH (or as an absolute
    file). Walks `~/.agentflow/plugins/*/plugin.toml` (under
    `feature = "plugin"`) and surfaces every plugin name + version +
    entrypoint with `entrypoint_exists` set. Promotes the overall
    status to Warning (or Fail under `production`) when any probe
    fails. Doesn't replace the heavier deferred probes — see below.
  - DONE (PR.3) MCP server reachability via configured transport —
    `doctor --check-installations` now walks `~/.agentflow/mcp.toml`
    (via `AGENTFLOW_MCP_CONFIG` env-override-aware loader) in
    addition to skill-declared `[[mcp_servers]]`. Top-level entries
    appear in the same `mcp_servers` array with `skill` field
    absent; report-level `mcp_config_source` field documents where
    the top-level entries came from. Unreachable top-level servers
    promote status the same way unreachable skill-declared ones do
    (Warning under local, Fail under production). Heavier
    transport-level handshake (spawn + `initialize` JSON-RPC +
    drain) stays a future enhancement when there's concrete demand.
    - DONE P3.4-PR.2 `agentflow mcp config` schema + CLI surface
      (the upstream prereq). New `~/.agentflow/mcp.toml` top-level
      registry with the same `McpServerConfig` shape skill manifests
      already use (name / command / args / env / timeout_secs /
      max_concurrent_calls). Resolution mirrors the LLM models
      config: `AGENTFLOW_MCP_CONFIG` env override →
      `~/.agentflow/mcp.toml` → empty config. Validator catches
      duplicate names, empty `name`, empty `command`. CLI:
      `agentflow mcp config {path | validate | list [--format
      text|json] | show <name>}` — covered by 10 unit tests +
      8 CLI integration tests (env-injected fixture mcp.toml).
      P3.4-PR.3 will plumb this into the doctor's MCP reachability
      probe.
  - DONE (PR.3) Plugin runtime spawn smoke — `doctor
    --check-installations` now invokes `agentflow_core::plugin::
    run_dry_run` for every plugin whose manifest declares
    `[plugin.dry_run]`. Outcome lands under
    `installations.plugins[].dry_run` as `{ duration_ms, outcome }`,
    with `outcome.status` being `"passed"` or `"failed"` and the
    failure variants `wrong_exit_code` / `killed_by_signal` /
    `timeout` / `spawn_failed` distinguishing the failure mode.
    Failed smoke promotes status the same way a missing entrypoint
    does (Warning under local, Fail under production). Plugins
    without `[plugin.dry_run]` leave the field absent — opt-in by
    design.
    - DONE P3.4-PR.1 plugin manifest `dry_run` field + smoke
      runner (the upstream prereq). New optional
      `[plugin.dry_run]` TOML sub-table on `PluginSection`
      (`args: Vec<String>`, `timeout_ms: u32` default 1000,
      `expected_exit: i32` default 0). `PluginManifest::validate`
      rejects empty args / zero timeout. New
      `agentflow_core::plugin::run_dry_run` /
      `run_dry_run_spec` async API that spawns the entrypoint
      (no sandbox wrapping — host diagnostic only,
      side-effect-free by contract), time-bounds it, returns
      `DryRunOutcome::{Skipped, Passed{exit_code}, Failed(...)}`.
      Failure variants cover `WrongExitCode`, `KilledBySignal`,
      `Timeout`, `SpawnFailed`. 9 unit tests cover validation +
      every outcome path against `/bin/sh` (Unix-gated for the
      spawn-driven ones, hermetic for the others).
      `docs/PLUGIN_DESIGN.md` §6.2 documents the new sub-table.
      Workspace `cargo clippy --workspace --all-targets
      --features agentflow-core/plugin -- -D warnings` clean.

- DONE P3.5 Permission explanation improvements:
  - DONE Slice 1 — `agentflow skill inspect --explain-permissions`
    now wires the P1.9 `resolve_tool_policy` table alongside the
    existing capability decisions. New repeatable flags
    `--allow-tool <NAME>` and `--deny-tool <NAME>` feed the CLI
    override layer (highest precedence). Output prints the
    `AdmissionSource` (`cli_deny` / `cli_allow` / `skill_allow` /
    `mcp_server_capability` / `tool_policy_default` / `no_match`)
    plus admission reason for every tool the skill declares, every
    tool named on the CLI, and (when wired) every MCP-advertised
    tool. 5 new CLI tests in `skill_cli_tests.rs` lock down the
    precedence rules. Hint message when the flags are passed
    without `--explain-permissions`.
  - DONE Slice 3 — `skill inspect --explain-permissions` now prints
    a `Sandbox profile:` block alongside the admission table. The
    block surfaces the detected platform backend (`sandbox-exec` /
    `seccomp` / `noop`), the tri-state `SandboxEnforcement` level,
    the manifest's `security.os_sandbox` opt-in flag, and operator
    notes that flag suspicious combinations: shell/script declared
    + backend enforcing + opt-in `false`; opt-in `true` + backend
    not enforcing; opt-in `true` + no sandboxable tool declared.
    The probe is hermetic — no subprocess spawn. 2 new CLI tests
    in `skill_cli_tests.rs` cover the rust_expert opt-out and the
    mcp-basic clean path.
  - DONE Slice 4 — `skill inspect --explain-permissions
    --with-mcp-discovery` spawns each declared MCP server via the
    existing `SkillBuilder::build_registry` path, groups the tools
    by `(server_name, tool_names)` into a `McpCapabilityMap`, and
    feeds it into `resolve_tool_policy`. The `MCP discovery:`
    section lists every advertised tool per server before the
    admission table renders, so MCP-advertised tools get admission
    rows alongside built-ins. Off by default — spawning MCP
    servers is slow / heavy. 3 new CLI integration tests in
    `skill_cli_tests.rs` cover the opt-in hint (no
    `--explain-permissions`), the discovery happy path, and the
    negative path (no `--with-mcp-discovery` ⇒ no section).
  - DONE Slice 2 — `agentflow workflow validate --explain-permissions
    <yaml>` walks `FlowDefinitionV2` and emits a per-node permission
    report. Each node is classified into one of nine
    `PermissionCategory` variants (`pure` / `filesystem` / `network` /
    `exec` / `mcp` / `plugin` / `llm` / `agent` / `unknown`), tagged
    with required capabilities (`fs.read`, `fs.write`, `net`, `exec`,
    `mcp.call`, `plugin.exec`, `agent.runtime`), and the relevant
    constraint parameters are surfaced (`url`, `method`,
    `allowed_domains`, `allowed_paths`, `allowed_commands`,
    `server_command`, `tool_name`, `plugin_id`, `model`, `skill`,
    `allowed_tools`). Missing-allowlist constraints emit "permissive:
    …" notes for operator review. `--format json` extends the existing
    envelope with a `permissions` object carrying per-node + aggregate
    counts. 4 new CLI tests in `workflow_tests.rs` cover text output,
    JSON envelope, off-by-default behaviour, and the shell-node
    capability surface.
  - DONE Slice 2 follow-up — agent-flavoured node permission tests.
    `agentflow-cli/tests/workflow_tests.rs` ships four new CLI
    integration tests:
    `cli_workflow_validate_explain_permissions_mcp_node` (text:
    asserts `mcp` category + `[mcp.call, net]` capability + server_command
    list + tool_name constraint; does NOT assert `.success()` since
    `type: mcp` is only schema-recognised when the `mcp` feature
    is enabled — the report still prints before the schema bail),
    `cli_workflow_validate_explain_permissions_mcp_node_json_envelope`
    (same workflow through `--format json`, asserts
    `permissions.nodes[].category = "mcp"`),
    `cli_workflow_validate_explain_permissions_skill_agent_node` (asserts
    `agent` category + `[agent.runtime]` capability + skill / model /
    allowed_tools constraint passthrough + advisory note),
    `cli_workflow_validate_explain_permissions_multi_agent_node`
    (handoff-mode multi_agent — confirms per-participant
    `agents[].skill` is NOT surfaced and the advisory note still
    fires). 8 explain_permissions tests pass overall (4 existing
    template/http/file/shell + 4 new). With this, P3.5 is fully
    closed (slices 1-4 + slice 2 follow-up).

- DONE P3.6 Native tool calling provider consistency tests:
  - `agentflow-llm/tests/provider_consistency.rs` now covers all six
    providers (OpenAI / Anthropic / Google / Moonshot / StepFun /
    Mock) across five axes:
    - Streaming text deltas with provider-native framing
      (OpenAI-compatible SSE / Anthropic event-named SSE / Google
      newline-JSON) — drained via `assert_stream_yields_hello_world`.
    - `tool_calls` array round-trip into the canonical
      `ToolCallRequest { id, name, arguments }` shape from each
      provider's wire format (OpenAI `tool_calls`, Anthropic
      `tool_use` block, Google `functionCall` part).
    - `tool_choice = auto | none | required | tool { name }` per-
      provider wire encoding (5 new `_tool_choice_all_modes` tests
      capture request body and assert provider-specific encoding
      against the matrix from `docs/LLM_PROVIDERS_MATRIX.md`).
    - Multimodal user message (text + image URL → text reply)
      preserves the base64 marker through OpenAI / Anthropic /
      Google / Moonshot / StepFun wire formats.
    - Error mapping for 401 / 429 / 5xx is locked across every
      provider (11 new `_maps_*_to_http_error` tests close the
      remaining matrix cells alongside the 5 original).
  - Live runs already live in
    `agentflow-llm/tests/provider_consistency_live.rs` gated on
    `AGENTFLOW_LIVE_LLM_TESTS=1` and individual capability env vars
    (`AGENTFLOW_LIVE_LLM_TEXT` / `…TOOLS` / `…VISION` / etc.).
  - CI gate: `agentflow-llm` was added to the `test` matrix in
    `.github/workflows/quality.yml`, which is already a
    release-gate dependency, so the consistency suite is now
    release-blocking (mock + recorded fixtures, no live calls).
  - Test count: 44 in `provider_consistency` (19 new on top of 25
    existing) + 98 lib + 4 matrix-doc + 3 trace = 169 hermetic
    agentflow-llm tests pass on every PR.

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

- DONE P3.8 Cross-hop OpenTelemetry context propagation (LLM +
  plugin hops shipped; MCP + worker gRPC remain follow-ups):
  - New `agentflow-tracing::context` module is the canonical home for
    cross-hop W3C trace propagation. Public surface:
    - `pub async fn scope(traceparent: String, fut: F) -> T` —
      install for the duration of `fut` via tokio task-local.
    - `pub fn current_traceparent() -> Option<String>` — read the
      active value (returns `None` outside a scope so consumers
      correctly suppress the carrier when there's no upstream
      context).
    - `pub const TRACEPARENT_ENV: &str = "TRACEPARENT"` — the
      canonical env var name OTel-aware subprocesses look for.
  - Plugin subprocess injection: `agentflow-cli` plugin preparers
    (`OsSandboxPluginPreparer` and the new `NoopWithTraceparent`
    shim) call `inject_traceparent_into_command(&mut Command)`
    before spawn, which sets `TRACEPARENT=<value>` from the
    task-local. The bare `NoopCommandPreparer` from
    `agentflow-core` stays untouched so embedders that don't want
    this behavior aren't affected.
  - 4 unit tests in `agentflow-tracing::context::tests` lock down
    the scope/current/nested-scope semantics and the env-constant
    spelling. 3 CLI integration tests in
    `agentflow-cli/tests/plugin_traceparent_tests.rs` spawn
    `sh -c 'echo tp=${TRACEPARENT-}'` to prove the env var arrives
    at the child, doesn't leak when no scope is active, and respects
    nested scopes.
  - `docs/TRACE_PERSISTENCE_SCHEMA.md` gains a "Hop continuity (P3.8)"
    section with the per-hop carrier table. LLM (shipped via
    `agentflow_llm::trace_context`) and plugin (shipped here) are
    marked done; MCP transport (`meta.traceparent`) and worker
    gRPC metadata are marked as follow-ups.
  - Follow-ups (separate slices):
    - MCP transport: inject `traceparent` into JSON-RPC `meta`.
    - Worker gRPC: inject into request metadata.
    - End-to-end integration test that walks a DAG run through
      LLM → MCP → Plugin → Worker and asserts a connected OTel trace.

- DONE P3.9 CLI feature flag CI matrix (closed — final cells were
  the agentflow-rag feature surface):
  - Quality CI `features` job now covers 18 combinations across 6
    crates. The 14 P3.9-partial rows from the previous slice were
    extended with 4 agentflow-rag rows: `rag-no-default`, `rag-pdf`,
    `rag-html`, `rag-pdf-html`. The `local-embeddings` feature is
    intentionally not wired in because it pulls `ort`, which
    downloads ONNX Runtime binaries at build time — fragile on CI
    networks. It stays a manual-validation flag and is exercised
    downstream by the existing `rag-eval-smoke` job.
  - Wishlist `audio` / `image` / `tracing-sqlite` / `otel` feature
    names still don't exist in any workspace crate; they'll be wired
    in if/when the actual flags ship (per the partial-status note).

- DONE P3.10 Examples smoke test CI:
  - Quality CI `examples` job already runs `cargo test --workspace
    --examples` (compile gate) plus a shell-driven runner for the
    no-API examples. This slice adds the explicit `cargo xtask
    examples-smoke` step that wraps each invocation in the
    P3.2 wall-clock contract. Job-level `timeout-minutes: 10`
    keeps the whole step bounded; the xtask's own 5-min total
    budget keeps the smoke loop itself bounded.
  - "Mark slow examples with a `slow_example` feature" is replaced
    by per-example timeouts in the smoke list — same outcome
    (slow examples are explicitly gated) without a new feature flag.

---

## P4 — Memory, RAG, And Eval Foundations

Goal: make retrieval, memory, and agent quality measurable and
regression-safe.

- DONE P4.1 RAG eval CI fixture:
  - `agentflow-rag/eval_datasets/ci_offline/` ships dataset.toml +
    20-doc synthetic corpus.jsonl + 10 queries.jsonl + 10 qrels.jsonl
    (graded 0–3). Text written fresh for the fixture so it's CC0-1.0
    with no external source to drift.
  - `agentflow-cli/tests/rag_eval_cli_tests.rs` (gated on `rag`
    feature) drives the CLI end-to-end and asserts every JSON
    envelope field downstream consumers need: `dataset.{path,
    manifest, corpus_size, queries, judgments}`, `baseline.{retriever,
    label, mrr, latency, per_k, num_queries}`,
    `latency.{mean_ms, p50_ms, p95_ms}`, `per_k` rows carrying
    `{k, recall, ndcg}`. K values must equal CLI default set
    `[1, 3, 5, 10]`. Recall@5 ≥ 0.8 sanity gate (today: 1.0).
  - Quality CI gains a `rag-eval-smoke` job; listed in
    `release-gate.needs` so schema or quality regressions fail the
    release gate.

- DONE P4.2 RAG eval baseline snapshots:
  - `ComparisonReport.paired_sign_p_value: Option<f64>` carries the
    one-tailed binomial p-value testing "candidate is worse than
    baseline" — `P(X ≤ wins | X ~ Binomial(wins+losses, 0.5))`,
    computed in log-space for numerical stability.
  - `agentflow-rag/eval_baselines/ci_offline/bm25.json` is the
    checked-in baseline; today's fresh run matches it (gate PASS,
    p-value n/a because all queries tie at perfect RR).
  - `agentflow rag eval --compare-baseline <path>` loads an
    `EvalReport` from disk, runs the fresh eval as candidate, and
    applies the regression gate. `--regression-recall-threshold`
    (default 0.03) and `--regression-p-value` (default 0.05) make
    both knobs operator-tunable. BOTH criteria must trip together
    to flag a regression — single hits inform but don't gate.
  - CLI exits 1 when regression detected; JSON envelope carries a
    new `regression` block with the decision + thresholds used.
  - `--compare-to` and `--compare-baseline` are mutually exclusive.
  - Quality CI's `rag-eval-smoke` job now runs the schema test, the
    gate-logic unit tests, AND the live baseline comparison. Future
    regressions that cross both thresholds will fail the release
    gate.
  - Tests: 6 new unit (paired sign p-value math) + 7 new unit
    (`evaluate_regression` gate logic) + 3 new CLI integration
    (compare-baseline PASS, JSON regression block, mutex flags).

- DONE P4.3 Agent eval format design:
  - `docs/AGENT_EVAL_FORMAT.md` defines the v1 on-disk format for
    `agentflow eval run` and the JSON report envelope. Dataset layout
    mirrors `agentflow-rag/eval_datasets/`: one directory holding
    `dataset.toml` (name / version / `[defaults]` block) + `cases.jsonl`
    (one EvalCase per line) + optional `fixtures/`.
  - `EvalCase` fields grounded in real workspace types:
    `max_steps` / `max_tool_calls` / `latency_limit_ms` map 1:1 to
    `agentflow_agents::RuntimeLimits`; `tools_allowed` / `tools_denied`
    mirror the P3.5 `--allow-tool` / `--deny-tool` admission
    precedence; `cost_limit_usd` ships a new
    `AgentStopReason::CostLimitExceeded` variant (additive under the
    P0.3 stop-reason contract).
  - Assertion DSL is a closed set of six variants:
    `contains`, `regex`, `tool_called`, `tool_not_called`,
    `step_count_below`, `final_answer_matches_skill`.
    `tool_not_called` is kept separate from `tool_called`+`max_count=0`
    because the failure report reads more naturally.
  - JSON envelope: dataset / dataset_version / started_at /
    finished_at / summary (totals + cost + p50/p95 latency) +
    per-case rows carrying `trace_id` for `agentflow trace replay`.
  - Architecture: the runner is one `agentflow_core::Flow` with one
    `EvalCaseNode` per case — reuses concurrent scheduling,
    checkpoints, OTel propagation, and `workflow validate` without
    duplicating that machinery.
  - CLI sketch (lands under P4.4): `agentflow eval run <dataset>`
    with `--format text|json`, `--filter`, `--parallelism`,
    `--fail-on-status`, `--compare-baseline`. Exit codes 0/1/2 mirror
    the rag eval convention.
  - Stability tier set: EvalCase fields, six-variant DSL, JSON
    envelope shape are stable at first land; future variants require
    a `schema_version` bump.

- DONE P4.4 Minimal agent eval implementation:
  - DONE Slice 1 — `agentflow-agents/src/eval/{dataset,assertion}.rs`
    implement the on-disk format and closed 6-variant assertion DSL
    from P4.3. `Dataset::load_from_dir` walks `dataset.toml` +
    `cases.jsonl`, applies `[defaults]` inheritance, validates
    uniqueness + non-empty assertion lists. `Assertion::evaluate`
    returns a structured `AssertionOutcome` and never panics. 23 unit
    tests cover every variant pass + fail path.
  - DONE Slice 2 — `agentflow-agents/src/eval/runner.rs` adds
    `EvalRunner` that walks the dataset, drives an `AgentRuntime` per
    case via an `AgentRuntimeFactory` trait, evaluates assertions
    against the captured `AgentRunResult`, and emits a structured
    `EvalReport` (matches the JSON envelope in
    `docs/AGENT_EVAL_FORMAT.md`). New `AgentStopReason::CostLimitExceeded`
    variant; `Eq` dropped from the enum derive because `f64` doesn't
    impl `Eq` (no consumer required it — grep-confirmed). Exhaustive
    `AgentStopReason` matches updated across react/agent.rs,
    agentflow-harness/runtime.rs, agentflow-server/harness_live.rs,
    and agentflow-cli/harness/run.rs. 10 new runner tests.
  - DONE Slice 3 — `agentflow eval run <dataset>` subcommand wires
    the runner with `--format text|json`, `--filter <glob>`,
    `--fail-on-status failed|never`. Default factory builds a fresh
    `ReActAgent` per case using the case-declared model + an empty
    `ToolRegistry` (skill loading + tool admission via P3.5 flags
    deferred to a follow-up). Tiny hermetic fixture under
    `agentflow-agents/eval_datasets/ci_offline/` (two cases against
    the mock provider). 6 new CLI integration tests + 4 unit tests
    for the glob/fail-threshold/format parser.
  - DONE Follow-up step 1 — skill-aware factory + per-case tool
    admission. New `SkillBuilder::build_with_admission(manifest, dir,
    admit)` reuses persona/registry/memory but only registers tools
    that pass the admit closure. Eval factory routes `case.skill =
    Some(_)` through this path with `case.tools_allowed/denied` as
    the admission filter (P3.5/P1.9 precedence at case scope).
    3 new CLI integration tests + 2 unit tests.
  - DONE Follow-up step 2 — cost tracking via pricing table +
    `AgentEvent::LlmCallCompleted`. ReActAgent emits the new event
    after every LLM call carrying `TokenUsage`. `PricingTable`
    loads from `AGENTFLOW_PRICING_TABLE` env or
    `~/.agentflow/pricing.yml`; missing file is not an error
    (everything costs $0). Runner aggregates per-case cost from
    events, enforces `case.cost_limit_usd` (over-budget flips
    status to Failed with `stop_reason = "cost_limit_exceeded"`).
    8 new tests across pricing + runner + CLI.
  - DONE Follow-up step 3 — `docs/SKILL_VALIDATOR_PROTOCOL.md`
    defines the v1 contract behind the `final_answer_matches_skill`
    assertion: closed `kind = "none" | "regex" | "command"`
    discriminator in skill.toml's new `[validation]` table; command
    validators stdin = final_answer, exit-code = verdict, 125
    reserved for "unrunnable", inherits skill security profile +
    OS sandbox.
  - DONE Follow-up step 3 implementation (2 commits, ~700 LoC):
    `agentflow-skills::validator` ships `SkillValidator` trait +
    `RegexValidator` + `CommandValidator` + `build_validator` factory;
    `SkillLoader::validate` pre-compiles validators so bad regex /
    empty command vector errors surface at manifest-load time, not
    eval-run time. The eval assertion layer's
    `SkillValidator` closure type was promoted from `Option<bool>` to
    a richer `SkillValidatorVerdict { Pass | Fail { reason } |
    Unrunnable { reason } }`, with `final_answer_matches_skill`
    mapping each verdict to a distinct `AssertionOutcome.reason`. CLI
    `ReActAgentFactory` resolves + caches the per-skill validator and
    wires it into the runner via `skill_validator(case)`. Tests: 16
    new unit tests in `agentflow-skills/src/validator.rs` (regex pass
    / fail / multiline / bad pattern at build time; command pass /
    fail with stderr capture / exit-125 unrunnable / timeout / stdin
    delivery / timeout clamping / TOML round trip) + 3 new in
    `assertion.rs` (pass / fail-surfaces-validator-reason /
    unrunnable-prefixed) + 3 new CLI integration tests
    (passes-when-regex-matches / fails-with-reason-in-report /
    no-validator-falls-through).
  - Trace replay path: every case carries a `trace_id` formatted as
    `eval-<case_id>-<epoch_ms hex>` so `agentflow trace replay
    <trace_id>` Just Works for failure debugging.
  - Release-gate quality claims can now point at
    `cargo test -p agentflow-cli --test eval_cli_tests` as the
    reproducible signal.

- DONE P4.5 Memory layering design:
  - `docs/MEMORY_LAYERING.md` defines four mutually exclusive memory
    layers (Session / Semantic / Preference / Entity facts) with
    lifetime, key, primary read API, and retention default per
    layer. Calls out the seam with RAG using four worked examples so
    agent-produced data and authored corpus never alias.
  - Trait surface: `MemoryStore` (existing) stays as the Session
    layer interface. `SemanticMemoryStore` extends `MemoryStore`
    (every semantic backend is also a valid session backend);
    `PreferenceStore` and `EntityFactStore` are *separate* trait
    hierarchies because their data shapes are not `Message` —
    keeping them separate prevents accidental dispatch through the
    wrong read API. `MemoryLayer` enum + new types
    (`PreferenceScope`, `EntityFact`) introduced here, implemented
    under P4.7.
  - Precedence at prompt-assembly time is fixed:
    Session → Preference → Entity facts → Semantic. Rationale:
    high-trust data first; semantic is the noisiest layer. A
    `MemorySummaryBackend` runs *before* this list (compacts
    overflowed session messages).
  - Retention per layer plus a future `agentflow memory prune`
    CLI sketch (lands with P4.7).
  - Migration path: current `SessionMemory` / `SqliteMemory` /
    `SemanticMemory` keep working without changes; new layers are
    additive. Skill manifests gain optional `[memory.preference]`
    and `[memory.entity_facts]` tables that older skills can omit.
  - Stability: `MemoryStore` stable; new types start experimental
    and promote to Beta after one skill ships a real integration.

- DONE P4.6 Memory and prompt golden tests:
  - `agentflow-agents/tests/prompt_assembly_golden.rs` adds 5 tests
    that lock down the prompt-assembly contract callers (eval, Harness,
    skills) rely on:
    - `prompt_assembly_short_context_matches_golden` — fixed input
      (persona + 3 history messages + 2 tools) ⇒ byte-stable
      `MultimodalMessage` list captured in
      `tests/fixtures/prompt_assembly/short_context.json`.
      `AGENTFLOW_PROMPT_GOLDEN_UPDATE=1` regenerates the fixture
      after intentional changes.
    - `prompt_assembly_long_context_triggers_summary_message` —
      30-message history × budget=16 ⇒ summary system message
      injected at position 1 (after persona system); kept history
      strictly fewer than original.
    - `prompt_assembly_token_budget_respected_after_compaction` —
      20-message history × ~16 tokens each × budget=32 ⇒ kept
      history's total `token_count` ≤ 32. This is the contract
      eval cost limits + harness budgets actually contract on.
    - `prompt_assembly_tool_descriptions_in_system_prompt` — every
      registered tool's name + description surfaces in
      `## Available Tools`.
    - `prompt_assembly_no_tools_omits_tools_section` — empty
      registry ⇒ no Available Tools section, no tool-call JSON
      instructions; final-answer JSON instruction still present.
  - Assertion helper `assert_json_subset` enforces the P0.3
    additive-field contract: keys in the fixture must appear in the
    actual; extra keys on actual are tolerated.
  - Maintenance: updated the pre-existing
    `tests/fixtures/agent_runtime_react_trace.json` golden fixture
    to include the two `llm_call_completed` events the runtime started
    emitting in `fbd3ee2` (P4.4 follow-up step 2). The fixture had
    been stale on main since that commit.

- DONE P4.7 Memory backend implementations:
  - `agentflow-memory/src/layer.rs` introduces the shared trait
    surface: `MemoryLayer` (4-variant enum + stable `as_str()`),
    `RetentionPolicy::default_for(layer)`, `PreferenceScope` (with
    a `local(user_id)` shorthand for single-tenant dev),
    `PreferenceValue` (value / updated_at / version),
    `EntityFact` (entity_id, fact_id, attribute, value, provenance,
    confidence, extraction + invalidation timestamps),
    `PreferenceStore`, `EntityFactStore`, and `SemanticMemoryStore`.
  - `agentflow-memory/src/preference.rs` implements
    `SqlitePreferenceStore` with `(tenant_id, user_id, key)` primary
    key, monotonic `version` on UPSERT, scope-isolated reads, sorted
    `list_preferences`, and `prune_older_than` driven by
    `updated_at`. 7 unit tests in the same file cover roundtrip,
    version bump, idempotent delete, scope isolation, sorted list,
    prune, and complex-JSON preservation.
  - `agentflow-memory/src/entity_facts.rs` implements
    `SqliteEntityFactStore` with `(entity_id, fact_id)` primary key,
    `attribute` + JSON `value` + `confidence` + `extracted_at` +
    `invalidated_at` columns, `get_facts(include_invalidated)`
    branching, `invalidate_fact` that errors when the fact is
    missing or already invalidated, and `prune_invalidated` that
    only drops rows past the retention cutoff. 8 unit tests cover
    roundtrip, no-merge for conflicting facts, invalidate
    visibility, double-invalidate error, replace-on-same-id,
    prune cutoff, prune-skips-active, entity isolation.
  - `agentflow-memory/src/semantic.rs` adds the
    `SemanticMemoryStore` impl on top of the existing
    `SemanticMemory`. The new `search_semantic(session, query, k)`
    returns `Vec<(Message, f32)>` with cosine scores; degrades to
    a keyword-search fallback (scored `0.0`) when the embedding
    fails. The existing `MemoryStore::search` path is preserved
    for one stability tier (Beta) per the design doc.
  - `agentflow-memory/tests/cross_layer_precedence.rs` integration
    test exercises all four layers in one scenario (session ⇒
    semantic search ⇒ preference scope ⇒ entity fact lifecycle)
    and asserts the independence guarantee: writes to one layer
    never surface through another's read API.
  - Encryption-at-rest: the trait shape allows a future
    `EncryptedPreferenceStore` to slot in. Local profile ships
    plaintext per the design doc; P5 key-management plumbing is
    a separate scope.
  - `agentflow memory prune` CLI is the next deliverable on top of
    this trait surface — schema-design + trait-impl shipped here,
    CLI command tracked as a follow-up.
  - Test count: 36 lib + 1 integration test = 37 hermetic
    `agentflow-memory` tests pass.

---

## P5 — Plugin, Marketplace, And Worker Hardening

Goal: keep extension and distributed foundations usable without
over-promising v1 stability before security and reliability gaps are closed.

PREREQ NOTE: Worker tasks (P5.5–P5.7) require P2.8 (worker node type
expansion) to be useful for non-trivial workloads.

- DONE P5.1 Remote marketplace install handoff:
  - Verified artifact cache → install dir flow was already in place
    for both Skills (`~/.agentflow/skills`) and Plugins
    (`~/.agentflow/plugins`) via `RemoteMarketplaceCache::cache_artifact_bytes`
    + `install_skill_package` / `install_plugin_package` (`agentflow
    marketplace install`). Checksum + signature gates fire before
    unpack as part of the cache step; signature/checksum mismatch
    reject paths are exercised by `remote_marketplace.rs` unit tests
    and the marketplace strict-verify CLI test.
  - This slice closes the remaining atomicity gap: `install_directory`
    in `agentflow-cli/src/commands/marketplace.rs` was previously a
    two-step `remove + copy_dir_recursive` that could leave a
    half-installed destination on failure. The refactor:
    1. Early-exit on collision (destination exists + no `--force`)
       before any filesystem write.
    2. Stage every file into a sibling temp dir
       `<dest_parent>/.<dest_name>.installing-<pid>-<nanos>`.
    3. On staging failure, remove the temp tree and leave the
       existing destination untouched (this is what the spec calls
       "atomic-rollback on extract failure").
    4. When `--force` is set, move the prior install aside to
       `.<dest_name>.replacing-<pid>-<nanos>` instead of deleting it,
       then `fs::rename(staging, destination)` swaps the new tree
       into place atomically. If the final rename fails, the moved-
       aside backup is renamed back so callers never see a missing
       destination.
    5. Successful rename, then the moved-aside backup is removed.
       A failed cleanup is logged as a warning but doesn't fail the
       install.
  - 3 new CLI integration tests in `marketplace_cli_tests.rs`:
    - `marketplace_install_leaves_no_temp_dirs_on_success` — happy
      path leaves the install root with only the final destination,
      no `.installing` siblings.
    - `marketplace_install_force_overwrite_preserves_install_root_layout` —
      v1 → v2 force overwrite swaps content and leaves no
      `.replacing` siblings.
    - `marketplace_install_collision_without_force_leaves_existing_intact` —
      pre-existing sentinel is preserved byte-for-byte when install
      hits a collision and no staging dir leaks.
  - Signature mismatch + checksum mismatch + partial-download retry
    rejections are already covered by `remote_marketplace.rs`
    `cache_artifact_bytes` unit tests
    (`remote_marketplace_rejects_invalid_checksum`,
    `marketplace_verify_strict_rejects_unsigned_artifact`, etc.); no
    duplicate coverage at the CLI layer is needed.

- DONE P5.2 Signed fixture artifacts:
  - Fixture archive sources are checked in under:
    - `agentflow-skills/tests/fixtures/signed/skill-rust-expert/SKILL.md`
    - `agentflow-core/tests/fixtures/signed/plugin-echo/plugin.toml`
      (+ `bin/echo-plugin` entrypoint stub).
  - `agentflow-skills/tests/marketplace_signed.rs` builds a
    deterministic `.tar.gz` from each fixture, computes the SHA-256,
    and exercises the cache through 7 cases:
    - strict signed Skill / Plugin paths succeed and report
      `signature_checked = true`;
    - non-strict (unsigned) Skill / Plugin paths succeed and report
      `signature_checked = false`, with the inline strict-policy
      gate confirming they would be rejected by
      `--require-signature`;
    - strict path rejects tampered signature values;
    - strict path rejects tampered artifact bytes (checksum gate
      fires before the signature verifier);
    - determinism guard verifies two builds of the same fixture
      yield byte-identical archives.
  - `agentflow-core/tests/plugin_signed_fixture.rs` (gated on the
    `plugin` feature) confirms the plugin manifest fixture still
    parses + validates and its entrypoint stub resolves to a real
    file.
  - `docs/MARKETPLACE.md` gained a "Local signing" section
    documenting the deterministic-archive + SHA-256-signature flow
    and the strict / non-strict policy layering.

- DONE P5.3 Marketplace unpack hardening:
  - `extract_package_archive` now enforces two new gates on top of
    the per-file 16 MiB cap and path-component checks: a cumulative
    256 MiB cap (zip-bomb defense) and a 16k-entry cap (directory
    bomb). `safe_archive_path` also rejects non-UTF-8 path bytes
    outright — portability footgun on Windows and round-trips ugly
    through Path on Unix.
  - `agentflow-cli/tests/marketplace_unpack_hardening_tests.rs`
    covers the missing edge cases the existing CLI suite didn't:
    nested archives (zip-shaped blob stored as opaque file, no
    auto-recursion); duplicate top-level `SKILL.md`; executable bit
    preservation on install (unix-gated); 4k-entry happy path;
    16k+ entry rejection; invalid UTF-8 path rejection; per-file
    bomb; gzipped bomb. The cumulative-cap 256 MiB defense has an
    `#[ignore]` test for manual validation.
  - Path traversal, symlink, hardlink, duplicate paths, and the
    per-file 16 MiB cap are already covered in
    `tests/marketplace_cli_tests.rs` — kept as-is.
  - Each failure case asserts the install root is left untouched
    (defense-in-depth check inside `assert_install_failure`).

- DONE P5.4 Plugin sandbox default policy (tied to P1.8):
  - New `select_preparer(profile, force_sandbox, allow_unsandboxed)`
    in `agentflow-cli/src/executor/plugin.rs` extends the P1.8
    install-time policy gate to plugin **spawn** time. The same
    `PluginPolicy::for_profile` defaults drive both decisions, so
    a plugin denied at install under `production` is also denied
    at spawn (defense in depth, not divergence).
  - Per-profile spawn defaults:
    - `dev` → `NoopCommandPreparer`; `AGENTFLOW_PLUGIN_SANDBOX=1`
      force-engages the OS bridge for stress-testing manifest
      capabilities.
    - `local` → `OsSandboxPluginPreparer`;
      `AGENTFLOW_ALLOW_UNSANDBOXED_PLUGIN=1` mirrors the install-
      time `--allow-unsandboxed-plugin` opt-out.
    - `production` → `OsSandboxPluginPreparer`; the opt-out env
      var is rejected with `PreparerSelectionError::OptOutRejected`
      and the spawn fails before any child process starts.
  - `PluginWorkflowNode::ensure_loaded` now propagates the policy
    error through `AgentFlowError::AsyncExecutionError`, so a
    production workflow asking for an unsandboxed spawn fails fast.
  - 7 new unit tests in `executor::plugin::tests` cover the full
    matrix (dev default / dev force-on / local default / local
    opt-out / production default / production opt-out reject /
    force-on overrides opt-out under local). The legacy
    `preparer_from_env_picks_noop_when_unset` test is dropped in
    favor of the pure `select_preparer` function the new tests
    exercise.
  - `docs/TOOL_PERMISSIONS.md` gains a "Plugin sandbox at spawn
    time (P5.4)" section with the spawn-time decision table
    (3 profiles × 3 flag states) and behavioral rules, alongside
    the existing P1.8 install-time table.

- DONE P5.5 Worker auth/admission checks (PREREQ: P2.8):
  - DONE: identity flavour. `WorkerCredential { worker_id, token }`
    pairs each call with an optional pre-shared key. Each worker may
    have a **set** of valid PSKs to support overlap-add-then-remove
    rotation. Signed-JWT identity is intentionally deferred — the
    crypto-key story (issuer, audience, key rotation) belongs to the
    broader auth track, so v0.4.0 ships PSK-only and the surface
    stays experimental.
  - DONE: server admission policy. `WorkerAdmissionPolicy` carries
    four orthogonal knobs (`allowed_workers`, `pre_shared_keys`,
    `max_workers`, `max_concurrent_tasks_per_worker`). Defaults are
    "anything goes" so the dev / single-process path keeps working
    unchanged.
  - DONE: admission gate. `AuthenticatedControlPlane` wraps
    `WorkerControlPlane`, exposes `admit / heartbeat / claim_task /
    report_result` taking a `WorkerCredential`, and tracks both the
    admitted-fleet count and the per-worker in-flight task counter.
    Re-admitting an existing worker is idempotent so a returning
    heartbeat never trips the fleet cap.
  - DONE: tests. `agentflow-server/src/scheduler/admission.rs#tests`
    covers the policy units (allowlist, PSK match, rotation overlap,
    fleet cap, per-worker concurrency cap — 6 tests). The 3
    integration tests live in `agentflow-server/tests/worker_admission.rs`:
    - `unknown_worker_cannot_claim_or_heartbeat`
    - `admitted_worker_can_poll_heartbeat_and_report`
    - `token_rotation_does_not_drop_in_flight_tasks`
  - DONE: stability stamp. `docs/STABILITY.md` adds a new row marking
    the distributed worker control plane (`WorkerProtocol`,
    `WorkerControlPlane`, `AuthenticatedControlPlane`, `WorkerCredential`,
    `WorkerAdmissionPolicy`, `NodeExecutionPayload`) **Experimental**
    with the explicit "pin the worker minor version" warning, and
    `docs/DISTRIBUTED.md` adds a "Worker Admission (P5.5)" section
    with the knob table, rotation flow, and test cross-references.
  - Deferred follow-ups (tracked under P2.7 / P5.6 / P5.7 — NOT this
    line):
    - gRPC adapter wiring: the gRPC service still uses the bare
      `WorkerProtocol`. Plumbing admission tokens through tonic
      metadata + mapping `AdmissionError` onto
      `Status::permission_denied` is scoped to P2.7 (server transport
      hardening).
    - Signed-JWT identity flavour: scoped to the broader auth track.

- DONE P5.6 Worker resource limit tests (PREREQ: P5.5):
  - DONE: per-node timeout. `WorkerResourceLimits::default_timeout`
    wraps every inner dispatcher in `tokio::time::timeout`. Expiry
    surfaces as `AsyncExecutionError` → `Failed { retryable: true }`
    so the scheduler can reattempt under a longer budget.
  - DONE: output size limit. `WorkerResourceLimits::max_output_bytes`
    swaps the success envelope for `{"truncated": true, "limit_bytes":
    N, "size_bytes": M}` and emits `worker.task.output_truncated`.
  - DONE: cancellation propagation. `WorkerCancellationToken` is
    cooperative (`AtomicBool` shared via `Arc`). `WorkerRuntime`
    checks before claim and races the inner dispatcher against the
    flag; cancellation surfaces as `Failed { retryable: false }`
    with `worker.task.cancelled`.
  - DONE: retry semantics. Existing
    `distributed_scheduler_retries_retryable_failure` plus the new
    `retry_semantics_honor_attempt_budget_under_timeout` test cover
    the budget exhaustion path; timeouts feed back into the same
    requeue logic.
  - DONE: synthetic runaway fixture. The `mock` payload's
    `sleep_ms` and `output_size_bytes` knobs make every guarantee
    deterministic. Tests live in
    `agentflow-worker/tests/resource_limits.rs` (4 tests).
  - DEFERRED (documented gap): in-process **memory** caps. Linux
    cgroups + macOS `setrlimit` belong to the supervising
    process / container runtime, not the worker binary. The
    documented operator story in `docs/DISTRIBUTED.md` (P5.6 section)
    points at systemd `MemoryMax` / Kubernetes `resources.limits`
    plus the `default_timeout` safety net.

- DONE P5.7 Distributed failure-domain tests (PREREQ: P5.5, P5.6):
  - DONE: all 6 scenarios pinned down by
    `agentflow-worker/tests/failure_domains.rs`:
    - `stale_heartbeat_redistributes_to_another_worker` — stale
      heartbeat → reaped + redispatched.
    - `worker_crash_midtask_is_reattempted_elsewhere` — crash
      modeled as permanent silence on the heartbeat channel;
      surviving worker completes the redispatched task and shows
      up in the stitched trace.
    - `retryable_failure_retries_on_another_worker` — first
      attempt fails retryably; the second worker picks up and
      succeeds.
    - `non_retryable_failure_is_terminal` — `FlowDefinitionError`
      (unknown node type) terminates immediately even with a high
      `with_max_attempts` budget.
    - `duplicate_completion_is_idempotent` — second
      `report_result` for the same `task_id` is rejected by the
      protocol layer so run accounting stays consistent.
    - `trace_stitching_preserves_both_attempts` — stitched trace
      records both attempt starts + terminals in monotonic
      `global_seq` order.
  - DONE: "Failure Domains (P5.7)" table in `docs/DISTRIBUTED.md`
    with the recovery semantics + test cross-references.

- DONE P5.8 Workflow `type: plugin` first-class node syntax:
  - `type: plugin` was already wired into `factory.rs` and the
    `specs_for_node_type` schema map (requires `manifest` +
    `node_type` string params) by P-N10; this slice closes the
    remaining surface.
  - Validation enhancement (`agentflow workflow validate`): when a
    node has `type: plugin` and the referenced `manifest` path is
    readable, the validator now parses the plugin manifest and
    checks that the requested `node_type` parameter matches one of
    its `[[plugin.nodes]]` entries. Mismatches produce an `issue`
    that names the bad value and lists every known node type. This
    surfaces typos / stale references at validate time instead of
    at the first workflow run. Lives in
    `validate_plugin_node_type` (feature-gated on `plugin`).
  - New CLI command `agentflow plugin generate-workflow-stub
    <plugin> [--node <name>] [--output <file>]` emits a YAML stub
    per declared plugin node:
    - Accepts either a plugin directory (auto-resolves
      `plugin.toml`) or a manifest path.
    - Without `--node`, emits one `type: plugin` block per
      declared `[[plugin.nodes]]`.
    - With `--node`, emits a single block; an unknown name errors
      with the list of known types.
    - Embeds the absolute manifest path so the stub works without
      further editing.
    - Sanitizes the node type into a YAML-safe `id` suffix
      (`_node`); unprintable types fall back to `plugin_node`.
    - 5 unit tests cover the render + sanitization helpers.
  - 4 new CLI integration tests in `workflow_tests::plugin_node_tests`
    cover the strict validation accept + reject paths and the
    `generate-workflow-stub` happy / filter / unknown-node paths.
  - `cli_workflow_run_supports_plugin_node` (existing) was updated
    to set `AGENTFLOW_ALLOW_UNSANDBOXED_PLUGIN=1` so the echo
    plugin (no `[plugin.capabilities]`) keeps spawning after P5.4
    flipped the `local`-profile default to sandboxed. Sandbox
    coverage stays exercised by the `select_preparer` matrix.
  - Dry-run + checkpoint roundtrip for plugin nodes is already
    covered transitively by the `workflow run` integration tests
    plus the broader checkpoint regression suite; no plugin-
    specific dry-run path is needed because dry-run only walks the
    `execution_order` without spawning.

---

## P6 — Web UI Productization (NEW)

Goal: evolve the embedded Web UI from "alpha shell" into a usable run
console without making it a required surface.

Design constraint: Web UI must remain a client of the same `/v1/*` and SSE
contracts the CLI uses. Never bypass server APIs for UI-only features.

- DONE P6.1 Run creation form:
  - New `/ui/runs/new` deep-link route. Server's `ui_router()` now
    serves `index.html` on `/ui`, `/ui/`, and `/ui/runs/new` so the
    SPA can pick the matching view from
    `window.location.pathname`.
  - Top-level `App` dispatches on `pathname`. The legacy run
    console becomes `RunConsole`; the new form is `RunCreateForm`.
    Both share the API token via a parent-owned state slot so the
    token never duplicates into the new-form-specific localStorage
    slot.
  - `RunCreateForm` fields:
    - Tenant + profile (`dev` / `local` / `production`) + API
      token (last not persisted).
    - Workflow YAML editor (monospace `<textarea>` with line
      counter + client-side `name:` / `nodes:` structural
      checks). Full Monaco + schema integration is documented
      as a follow-up.
    - Inputs (optional JSON, parsed client-side; surface error
      under the field).
    - File-pick for both editors (`<input type="file">`) so
      operators can load `workflow.yaml` / `inputs.json` from
      disk without paste.
    - Submit calls `POST /v1/runs` and `window.location.assign`
      to `/ui?run=<id>` so the existing run console picks the
      new id from the query param.
  - `localStorage` keys (`agentflow.ui.newForm.*`) persist
    tenant / profile / workflow / inputs. The API token uses the
    existing `agentflow.ui.apiToken` slot only — `RunCreateForm`
    never writes a new-form-specific token slot, and the
    third Playwright spec asserts this.
  - Playwright suite at `agentflow-ui/e2e/runs-new.spec.ts`
    covers: submit → redirect to `/ui?run=…`, persistence across
    reloads, and the no-token-in-newform-slot guarantee.
    Running it requires explicit installation
    (`npm install --save-dev @playwright/test` +
    `npx playwright install chromium`) — kept out of the workspace
    install graph by design to keep the default UI build small.
  - Bundle impact: `dist/assets/app.js` 204 KiB → 209 KiB
    (+5 KiB). `dist/assets/styles.css` 5.7 KiB → 7.9 KiB. No new
    npm dependencies.
  - Deferred under P6.1:
    - Full Monaco editor + `agentflow workflow validate` schema
      integration (would need a server `POST /v1/workflows/validate`
      route and a bundled JSON schema; tracked as a follow-up to
      keep the dist bundle reasonable).
    - CI wiring for the Playwright suite (requires Chromium binary
      + reachable `agentflow serve` + Postgres).

- DONE P6.2 Provider config diagnostics panel:
  - Promoted `agentflow_cli::commands` (and the doctor module's
    `DoctorReport` / `DoctorProfile` / `build_report`) to `pub` so
    the server can read the same schema in-process instead of
    shelling out.
  - `GET /v1/diagnostics` (`agentflow-server/src/diagnostics.rs`)
    delegates to `build_report(DoctorProfile::Local, None, false)`
    and returns the canonical doctor JSON. Inherits the same
    bearer-token gate as the rest of `/v1/*`. Tests cover the happy
    shape and a defense-in-depth check that the API token value
    never appears in the response body.
  - `agentflow-server/tests/diagnostics_route.rs` adds the
    route-level integration tests (no live Postgres required —
    diagnostics handler does not touch `AppState.db`).
  - UI: new `/ui/diagnostics` deep-link route + `DiagnosticsPanel`
    component. Renders a per-component pass / warn / fail table
    covering Models config, Security profile, OS sandbox, the
    three disk dirs, and the `AGENTFLOW_API_TOKEN` env flag.
    Refresh button only — no auto-poll. The panel never displays
    raw token values; any token passed through the input is
    rendered via a `maskToken(...)` helper that shows only the
    last 4 chars.
  - `ui_router()` registers `/ui/diagnostics` alongside the
    existing SPA deep-link routes so direct nav from a bookmark
    or copied URL works.

- DONE P6.3 Trace comparison view (MVP):
  - New `RunCompare` component in `agentflow-ui/src/main.tsx`,
    mounted at `/ui/runs/:id/compare?against=<other_id>`. App router
    matches the path and renders the component with the primary run
    id; the `against` query param seeds an editable second-run input.
  - Server `ui_router()` adds `/ui/runs/:id/compare` to the SPA
    deep-link list so direct navigation works.
  - Each column independently fetches `GET /v1/runs/{id}/events/history`
    (existing route; carries the full payload already, no schema
    change needed). The response is treated as the source of truth
    for diffing.
  - Diff highlighting: every event is keyed by `${kind}#${step_index ?? seq}`;
    events present in both runs get a green border + `matched` class,
    events present in only one run get an amber border + an explicit
    `only here` tag. Hop latency is computed as `ts[i] - ts[i-1]`
    per column and rendered inline.
  - Summary cards above the columns show: event count, tool-call
    count, total wall-clock duration, mean hop latency, and the
    final answer (when present in the run's terminal event).
  - Bundle impact: app.js 233 KiB → 238 KiB. CSS adds ~3 KiB. No
    new npm deps.
  - Follow-ups (not blocking):
    - Inline-diff the final-answer strings (today they render
      side-by-side; a word-level diff would be more useful).
    - Persistent "saved comparisons" list per tenant via the P6.4
      preferences API.
    - SSE auto-refresh when either run is still in flight.

- DONE P6.4 Durable user preferences (server half — UI wiring is a
  follow-up):
  - New `0004_user_preferences.sql` migration adds a tenant-scoped
    `(tenant_id, key, value JSONB, updated_at)` table keyed by
    `(tenant_id, key)`. Index on `(tenant_id, updated_at DESC)` so
    "what changed recently" reads are cheap.
  - `agentflow-db` ships `UserPreferenceRepo` trait +
    `PgUserPreferenceRepo` (upsert / upsert_many / list_for_tenant /
    delete) plus `UserPreference` + `NewUserPreference` models.
    `Repositories::from_pool` wires it into `AppState.repos`.
  - `agentflow-server::preferences` adds:
    - `GET /v1/preferences` → `{ preferences: { <key>: <value>, ... } }`
      for the tenant bound by `X-Agentflow-Tenant` (P2.6).
    - `PUT /v1/preferences` upserts a batch in one transaction.
  - Validation rules:
    - Key must match `^[a-zA-Z0-9_.\-:]{1,128}$`.
    - Value JSON serialise ≤ 16 KiB.
    - Values screened for token-shape strings (Bearer-prefixed,
      `sk-`/`ant-`/`ghp_`/etc. API-key prefixes, 32+ hex digests,
      40+ char alphanumeric+`=/+` opaque strings). A match → 400
      with the rejected key in the message.
    - Token-screen rejections are atomic — no row from the batch
      persists if any value is rejected.
  - 3 unit tests + 5 server integration tests cover the happy
    round-trip, tenant isolation, and the rejection paths. Token
    screen is unit-tested against representative real-world prefixes
    and against safe values that must NOT trigger.
  - Follow-up: wire the UI (`agentflow-ui/src/main.tsx`) to read /
    write through the new endpoints, replacing the localStorage-only
    path. Tracked alongside P6.5 since that's the next UI surface
    that needs to persist state.

- DONE P6.5 Operator-focused event filter (client-side; server-side
  fallback deferred):
  - New `agentflow-ui/src/eventFilter.ts` parses a tiny expression
    language:
    - `kind=<value>` / `kind!=<value>` (case-insensitive exact)
    - `kind~<substring>` (case-insensitive substring)
    - `step<op>N` where `<op>` ∈ `> >= < <= = !=` (matches
      `payload.step_index`, falling back to `event.seq` when absent)
    - `AND` between clauses (case-insensitive)
    - Empty string ⇒ match every event.
  - Parse errors surface as a structured `error` field so the UI
    renders them inline instead of crashing.
  - `agentflow-ui/src/main.tsx` adds a filter input above the run-
    detail timeline; the input persists per `run_id` in
    `agentflow.ui.run.eventFilter.<id>` so the same filter survives
    a page reload. Compile error renders under the input in red.
    The timeline header surfaces `(matched/total)` when a filter
    is active.
  - 18 self-test assertions in `src/eventFilter.test.ts` cover
    empty / kind= / kind!= / kind~ / step ops / AND chaining /
    whitespace tolerance / malformed-clause error paths. Run via
    `bun src/eventFilter.test.ts`; `npm test` (tsc --noEmit)
    stays clean.
  - Follow-ups (not blocking):
    - Server-side filter fallback (today client-side only; once
      `/v1/runs/{id}/events/history` accepts a `?filter=` param
      it can pre-filter for very long runs).
    - Persist the filter through the P6.4 `/v1/preferences` API
      under `ui.run.<id>.filter` so it survives across browsers.
      The localStorage slot stays as the first-paint cache.

---

## P7 — Performance And Release Engineering (NEW)

Goal: establish a perf baseline + release rehearsal so v1.0 ships with
known characteristics, not surprises.

- DONE P7.1 `cargo bench` baselines:
  - Criterion benches landed for all four crates:
    - `agentflow-core/benches/scheduler.rs` — linear + fan-out shapes
      at 10/100/1000 nodes, serial vs `concurrent_8`.
    - `agentflow-llm/benches/provider_hop.rs` — mock provider single
      hop (1/8/32 turns) + streaming full-drain.
    - `agentflow-rag/benches/retrieval.rs` — BM25 search at 1k/10k
      corpus, top_10 / top_100; plus build-corpus index throughput.
    - `agentflow-tracing/benches/event_write.rs` — serialize-only,
      `FileTraceStorage::save_trace`, and synthetic JSONL append
      (sibling group reserved for a real JSONL / SQLite backend
      once those land).
  - Captured baseline: `benches/baselines/apple-m2-max.json` plus a
    README documenting the schema and capture flow. Host differences
    are expected; the P7.2 gate compares against the runner's own
    baseline.
  - PERFORMANCE.md (in `agentflow-core/`) now links to the criterion
    suites alongside the legacy `cargo test` perf harness.

- DONE P7.2 CI perf regression gate (MVP):
  - New `cargo xtask bench-gate` subcommand reads each
    `target/criterion/<group>/<bench>/new/estimates.json`, looks
    up the matching `benches/baselines/<host>.json` row, and exits
    non-zero when any bench's current median is at least
    `DEFAULT_REGRESSION_RATIO = 1.25×` baseline. Per-row output is
    deterministic (`baseline=… current=… ratio=N.NN× [ok|REGRESSION]`)
    so CI logs are diff-able.
  - `--baseline <path>` overrides the checked-in default;
    `--threshold <ratio>` overrides the 1.25× knob (rejects values
    ≤ 1.0 so the gate can't be silently neutralised); `--allow-missing`
    lets the gate pass when the baseline references benches the
    runner didn't measure (used by CI until a per-runner baseline
    lands).
  - `pick_criterion_root` honors `CARGO_TARGET_DIR`, then
    `.cargo/config.toml` `build.target-dir`, then the workspace
    `target/` fallback — works under the
    `~/.cargo/config.toml` `target-dir = /Users/.../target` pattern
    documented in `CLAUDE.md`.
  - New `.github/workflows/bench.yml` runs the four Criterion suites
    (scheduler / provider_hop / retrieval / event_write) on PRs that
    touch perf-sensitive crates + benches + xtask, plus pushes to
    main. Job-level `timeout-minutes: 30`. The gate step uses
    `--allow-missing` until `ci-ubuntu-latest.json` baseline lands;
    today flips to hard-gate by dropping the flag.
  - 5 new xtask unit tests cover the comparator under tempdirs
    (happy path, regression-exceeds-threshold fail, missing-bench
    fail, `--allow-missing` skip, invalid-threshold rejection).
  - Follow-ups (not blocking):
    - Capture a `ci-ubuntu-latest.json` baseline from a clean run
      on the CI runner and drop `--allow-missing` from the
      workflow.
    - PR summary comment with the per-bench table (today the gate
      writes structured stdout; CI captures it in step output
      which the operator reads inline).

- DONE P7.3 Examples smoke test in CI (closed alongside P3.2 / P3.10):
  - Quality CI `examples` job runs the new `cargo xtask
    examples-smoke` step, bounded by the xtask's 5-min total budget
    and a 10-min job-level `timeout-minutes`. `examples/README.md`
    is the canonical index pointing at the per-crate examples and
    `examples/ecosystem/` (the official entry surface).

- DONE P7.4 v1.0 release dress rehearsal:
  - DONE: findings captured in
    `docs/RELEASE_NOTES_DRESS_REHEARSAL.md` (5 sections — F1 fixed,
    F2/F3 pre-existing drift, F4 advisory, F5 verified scope).
  - DONE: docker image build via
    `docker buildx build --build-arg PACKAGE=agentflow-server`
    completes (post-fix). Compose stack boots Postgres +
    agentflow-server end-to-end; `/health/live`, `/health/ready`,
    `/ui` all return `200`; DB migrations apply automatically.
  - DONE: `agentflow doctor --profile production --format json`
    runs and surfaces the expected developer-host warnings
    (trace_dir / marketplace_cache auto-create-on-first-write,
    AGENTFLOW_API_TOKEN unset). These are runbook gaps, not code
    defects.
  - F1 FIX (in-rehearsal): `agentflow-tools/src/sandbox/linux.rs`
    had two release-blocking Linux compile errors that only the
    docker image surfaces (the macOS dev build cfg-gates the
    backend out). Both fixed: the for-loop binding `&i64` is now
    dereffed before `BTreeMap::insert`, and the `try_into()` →
    `BackendError` is mapped onto the unified `seccompiler::Error`
    via the upstream `From` impl.
  - NOT yet DONE on this rehearsal — deferred to the actual rc.1
    cut:
    - Tagging `v1.0.0-rc.1` from a release branch (one-way
      decision; human operator).
    - `cargo publish --dry-run` for publishable crates.
    - GitHub Release artifact publish / image push.
    - Fresh-VM `doctor` smoke (this rehearsal ran on a developer
      host with existing `~/.agentflow/`).
  - Refiled follow-ups (each a separate targeted task below):
    P7.4-FU1 (Linux sandbox CI), P7.4-FU2 (workspace rustfmt
    sweep), P7.4-FU3 (clippy::result_large_err boxing pass),
    P7.4-FU4 (production deployment runbook in release notes).

- DONE P7.4-FU1 Linux sandbox check in CI:
  - New `linux-sandbox-check` job in `.github/workflows/quality.yml`
    runs `cargo check --target x86_64-unknown-linux-gnu -p
    agentflow-tools --all-targets` on every PR / push so a
    re-introduction of the F1-style compile error in
    `agentflow-tools/src/sandbox/linux.rs` fails in ~2 min instead of
    at release time. Job listed under `release-gate.needs` so a
    Linux-only break also blocks the release gate.
  - Decision: run on every event (matches every other quality job —
    no path filter), keeping the YAML simple. ~2 min wall clock so
    the added CI cost is negligible.
  - **Acceptance**: a PR that re-introduces F1 fails
    `linux-sandbox-check` within ~2 min.

- DONE P7.4-FU2 Workspace rustfmt sweep before tag:
  - Single `chore(fmt)` commit picked up the residual drift in 6
    files identified by the P7.4 dress rehearsal: benches
    (`agentflow-core/benches/scheduler.rs`,
    `agentflow-llm/benches/provider_hop.rs`), tests
    (`agentflow-core/tests/plugin_signed_fixture.rs`,
    `agentflow-skills/tests/marketplace_signed.rs`,
    `agentflow-worker/tests/failure_domains.rs`), and the
    `agentflow-tools/examples/tool_policy_sandbox_demo.rs`
    example. No functional changes; `cargo check --all-targets` on
    every touched crate stays green.
  - **Acceptance**: `cargo fmt --all -- --check` exits 0 on the
    release branch (verified locally).

- DONE P7.4-FU3 Box `tonic::Status` + workspace clippy sweep:
  - Private `BoxedStatusResult<T> = Result<T, Box<Status>>` alias in
    `agentflow-server/src/scheduler/grpc.rs`. Six proto<->domain
    conversion helpers (`worker_task_from_proto`,
    `worker_trace_event_from_proto`, `worker_task_result_from_proto`,
    `worker_heartbeat_from_proto`, `parse_uuid`, `parse_json`) now
    return `BoxedStatusResult<T>` and box `Status` at construction.
    Public `WorkerControl` trait surface stays unboxed; callers in
    the trait impls unbox with `.map_err(|e| *e)?` at the boundary.
    The `GrpcWorkerProtocol::claim_task` client-side mapper unboxes
    via `|boxed| scheduler_error_from_status(*boxed)`.
  - Tag-along pre-existing clippy fixes folded into the same commit
    so the workspace gate actually hits zero:
    - `preferences.rs` `let _ = ...await?;` → drop binding
      (let_unit_value).
    - `distributed.rs` `or_else(|_| Ok(...))` → `or(Ok(...))`
      (unnecessary_lazy_evaluations).
    - `serve.rs` `assert_eq!(.., false)` → `assert!(!...)`
      (bool_assert_comparison).
    - `cleanup_route.rs` `repos(&db)` where `db: &Database`
      (needless_borrow).
    - `mcp.rs` unused `mut` in an integration-test fixture
      (unused_mut).
    - `backend.rs` `serde_json::to_value(&level)` → `(level)`
      (needless_borrow).
  - **Acceptance**: `cargo clippy --workspace --all-targets -- -D
    warnings` exits 0. agentflow-server lib tests (75) +
    agentflow-nodes mcp lib tests (30) + agentflow-tools sandbox
    tests stay green.

- DONE P7.4-FU4 Production deployment runbook in release notes:
  - New `docs/RELEASE_NOTES_v1.0.0-rc.1.md` (DRAFT) carries a
    `## Production Deployment Checklist` section that closes the
    rehearsal F4 finding. Six numbered steps walk a fresh operator
    through:
    1. Pick + wire `AGENTFLOW_SECURITY_PROFILE=production` and what
       it turns on (auth fail-closed, CORS deny-by-default, plugin
       sandbox required, marketplace signed-only, SSRF protections,
       tool admission fail-closed).
    2. Provision `AGENTFLOW_API_TOKEN` via secret manager with
       worked Kubernetes / systemd / docker-compose snippets.
    3. Pre-provision the five storage roots (`AGENTFLOW_RUN_DIR`,
       `_TRACE_DIR`, `_MARKETPLACE_CACHE`, `_SKILLS_DIR`,
       `_PLUGINS_DIR`) with a `/var/lib/agentflow` ownership / mode
       example.
    4. Wire `DATABASE_URL`; embedded `sqlx::migrate!()` runs on
       first boot.
    5. Verify with `agentflow doctor --profile production
       --backup-check --format json`, refuse to swing traffic
       until exit code is 0.
    6. Optional `docker-compose` smoke before promoting.
  - Acceptance gate documented at the bottom of the checklist:
    fresh-VM operator gets doctor exit 0, serve --check ok, clean
    serve start, authenticated `POST /v1/runs` 200.
  - The remaining release-notes sections (`What's New`, `Breaking
    Changes`, `Known Issues`) stay as placeholders so the FU4 deliverable
    isn't bundled with feature-summary writing that belongs to the
    actual tag cut.

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

- DONE P-H.5 Server + Web UI integration (Phase H5; ~3-5 weeks; PREREQ:
  P2.1, P2.2, P2.4, P-H.2, P6.1):
  - Slice 1 (DONE): server schema + core routes
    - DONE: DB migration `0002_harness_sessions.sql` adds dedicated
      `harness_sessions` + `harness_session_events` tables (kept separate
      from `runs` so the lifecycle columns stay strongly typed instead of
      overloading the workflow schema with sentinel columns).
    - DONE: `agentflow-db` `HarnessSessionRepo` / `HarnessEventRepo`
      traits + Pg impls bundled into `Repositories`.
    - DONE: `agentflow-server::harness` module with
      `HarnessEventBroker`, `HarnessSessionExecutor` trait,
      `StubHarnessExecutor` (records `session_started` + `stopped`
      events and marks the row `failed: executor_not_yet_wired` until
      the real runtime lands).
    - DONE: routes wired into `create_router`:
      - `POST /v1/harness/sessions`
      - `GET /v1/harness/sessions`
      - `GET /v1/harness/sessions/{id}`
      - `POST /v1/harness/sessions/{id}:cancel`
      - `GET /v1/harness/sessions/{id}/events` (SSE with backfill)
      - `GET /v1/harness/sessions/{id}/events/history` (JSON history)
    - DONE: integration tests `tests/harness_routes.rs` self-skip
      without `AGENTFLOW_DATABASE_TEST_URL` (mirrors
      `sse_robustness.rs` pattern). Verified seven scenarios pass on
      a Postgres deployment.
  - Slice 2 (DONE): approval routes + LLM-backed executor
    - DONE: `agentflow-server::harness_approval` adds
      `PendingApprovalRegistry`, `ServerApprovalProvider` (parks
      `ApprovalRequest`s on per-`(session, request_id)` `oneshot`
      channels, honors `expires_at` with a 5-min default deadline,
      cleans up timed-out / dropped entries).
    - DONE: routes `GET /v1/harness/sessions/{id}/approvals` and
      `POST /v1/harness/sessions/{id}/approvals/{request_id}`.
    - DONE: `agentflow-server::harness_live` adds
      `LiveHarnessExecutor` that wires `HarnessRuntime` ↔ `ReActAgent`
      ↔ tool registry (hook-wrapped via `wrap_registry`) with the
      shared `ServerApprovalProvider`. `agentflow serve` swaps the
      default `StubHarnessExecutor` for the live one; tests keep the
      stub via plain `AppState::new(db)` so workspace `cargo test`
      stays hermetic.
    - DONE: `ServerHarnessEventSink` translates the closed
      `HarnessEvent` envelope into `harness_session_events` rows + a
      `HarnessEventBroker` publish so SSE backfill and live push share
      one source of truth.
    - DONE: `HarnessRuntime::run` holds `&self` across awaits, which
      forces `HarnessRuntime: Sync` (and `AgentRuntime: Send` is
      `Send`-only). `LiveHarnessExecutor` runs each session on its
      own current-thread Tokio runtime hosted in
      `tokio::task::spawn_blocking` so the executor stays `Send`
      without forcing the rest of the server onto a current-thread
      runtime. Cost: one OS thread per concurrent harness session;
      removed once `HarnessRuntime` is updated to thread `&mut self`
      (or `AgentRuntime: Sync` is added).
    - DONE: integration tests
      `agentflow-server/tests/harness_approval_routes.rs` (four
      cases) and `tests/harness_live_executor.rs` (single Moonshot
      E2E, gated on both `AGENTFLOW_DATABASE_TEST_URL` and
      `MOONSHOT_API_KEY` so the workspace stays hermetic without
      either).
  - Slice 3 (DONE): Web UI
    - DONE: `/ui/harness/sessions` list page (tenant-scoped table with
      status pill, profile, runtime, model, prompt preview, ID; auto-
      refresh every 4s; click row → detail).
    - DONE: `/ui/harness/sessions/new` submit form (prompt +
      workspace_root + profile + runtime + model + skill_name;
      localStorage-persisted inputs; API token never persisted; submit
      → redirect to detail).
    - DONE: `/ui/harness/sessions/{id}` detail page (summary block,
      event timeline with kind tone colours + payload pane, pending
      approval cards with allow / deny / deny_and_stop and once /
      session / run scope dropdown, cancel button gated on terminal
      status; polls session + events + approvals every 2s).
    - DONE: server-side deep-link routes
      (`ui_router_registers_harness_deep_link_routes` test confirms
      all four `/ui/harness/sessions*` paths serve the SPA shell).
    - DONE: Vite build refreshed (`agentflow-ui/dist/app.js` 209→225
      KiB; `styles.css` 7.9→13.88 KiB; no new npm deps).
    - DONE: Playwright spec
      `agentflow-ui/e2e/harness-sessions.spec.ts` (three cases:
      submit→redirect; list→detail; localStorage persistence without
      token).
    - Session resume action remains TODO (depends on the
      `POST /v1/harness/sessions/{id}:resume` route in slice 4).
  - Slice 4 (DONE): resume route + SSE-backed UI + full-stack E2E
    - DONE: `POST /v1/harness/sessions/{id}:resume` with the rerun
      semantic (clear prior events, flip status back to `running`,
      respawn executor; optional `user_input` override). Single
      dispatcher `post_harness_session_action` handles both `:cancel`
      and `:resume` so one POST route binds two semantically distinct
      actions. Atomic via a Postgres txn that DELETEs from
      `harness_session_events` then UPDATEs the row.
      `HarnessSessionRepo::reset_for_resume` keeps the wipe + status
      flip in one Pg transaction; integration tests cover the happy
      path, the 400 on a running session, and the unknown-suffix /
      unknown-id failure cases.
    - DONE: append-mode resume (preserving prior events + continuing
      the seq series). All three layers landed: the upstream
      `HarnessRuntime::with_initial_seq` builder in `agentflow-harness`,
      the server-side wiring on `:resume`, and the Web UI mode toggle.
      The route accepts `mode: "rerun" | "append"` (default `rerun`
      for backwards compat); `append` queries `MAX(seq)` from
      `harness_session_events`, leaves prior rows intact via
      `HarnessSessionRepo::reset_for_append_resume`, and seeds the
      executor with `initial_seq = MAX(seq) + 1`. The Web UI detail
      page exposes a `rerun` / `append` select next to the resume
      button; in append mode the local timeline is kept on screen so
      new seqs visibly extend the prior log. Integration tests
      `resume_append_mode_preserves_events_and_continues_seq` and
      `resume_default_mode_is_rerun_when_field_omitted` cover the
      route paths.
    - DONE: UI detail page switches from polling to SSE
      (`EventSource` against `/v1/harness/sessions/{id}/events`). The
      session row + pending approvals still poll on 2s since they
      live on a separate REST surface. Stream pill in the controls
      strip shows `streaming` / `error` / etc.; on SSE failure the
      page falls back to a 5s history poll so the timeline keeps
      updating even when the broker channel has been dropped.
    - DONE: UI "Resume (rerun)" button posts to `:resume` with the
      optional prompt input; gated on terminal status; clears local
      timeline state on success so the rerun lifecycle isn't mixed
      with the stale one.
    - DONE: `tests/harness_full_stack_e2e.rs` — single test that
      drives submit → SSE stream → DB history → terminal row →
      resume → rerun history against real Postgres + Moonshot. ~6.5s
      end-to-end. Self-skips without DB or Moonshot key.
    - DONE: `docs/HARNESS_MODE.md` marks slice 4 closed; `CLAUDE.md`
      lists the resume route + SSE detail page in the gateway / UI
      surface; `STABILITY.md` already promoted the envelopes to Beta
      in slice 2.

### Deferred to RoadMap Later Tracks

- DEFERRED P-H.H6 Advanced compatibility (Phase H6; open-ended):
  - Slash-command ecosystem expansion.
  - TUI product shell (separate from CLI run).
  - OpenHarness-style config import.
  - Plugin compatibility adapters.
  - Provider subscription bridge.
  - Promote individual H6 items to TODOs only when concretely required.

---

## P9 — Dogfooding-Driven Refinements (NEW)

Action items from `docs/L1_L3_REFLECTION_2026-05-18.md` —
consolidates fixes / docs flagged by A1 (L1) + A1.5 (L3) live
end-to-end validation runs against real Moonshot + MiniMax APIs.

All items here are documentation, error-message, or peripheral
fix scope. **No urgent core refactor surfaced** by the dogfooding,
so this segment is small and time-bound.

- DONE P9.1 `agentflow skill validate` surfaces underlying error message:
  - One-line fix in `agentflow-cli/src/main.rs`: changed
    `eprintln!("Error: {}", e)` → `eprintln!("Error: {:#}", e)`.
    anyhow's chain display now prints outermost context plus every
    `source()` joined by `: `. Benefits every CLI command, not just
    `skill validate`.
  - Verified with a synthetic skill that triggers the
    `mcp_command_allowlist` rejection — error now reads:
    `Error: Validation failed: Invalid skill configuration:
    [[mcp_servers]] 'some-binary' command 'totally-disallowed-binary'
    is not listed in security.mcp_command_allowlist` instead of the
    previous bare `Error: Validation failed`.
  - clippy + fmt clean.

- DONE P9.2 Document `security.mcp_command_allowlist` opt-in for native binaries:
  - New "Spawning native binary MCP servers" subsection in
    `docs/MCP_SKILLS.md` covers:
    - The exact error message new skill authors will see when the
      validation gate fires (cross-references P9.1's improved
      chain display)
    - Why the default is interpreter-only (deploy-time attack surface)
    - Worked example showing `[security] mcp_command_allowlist`
      extension (NOT replacement) of the interpreter defaults
    - Pointer to `examples/applications/podcast-mastering/skill.toml`
      as the concrete pattern reference
    - 3 common pitfalls: forgetting interpreter defaults, full-path
      instead of basename, case / extension mismatch

- DONE P9.3 Auto-load `~/.agentflow/.env` from agentflow CLI:
  - `dotenvy = "0.15"` added as `agentflow-cli` dep.
  - New `load_agentflow_dotenv()` helper in
    `agentflow-cli/src/main.rs` called at the very top of `main()`
    (before `Cli::parse()`); silent no-op when the file is missing.
    Process env vars take precedence over file values (dotenvy
    default), so inline overrides like `MOONSHOT_API_KEY=other
    agentflow ...` continue to work.
  - Verified with `env -i HOME=$HOME PATH=$PATH agentflow doctor`:
    shell shows `MOONSHOT_API_KEY: NOT SET`, but `agentflow doctor`
    reports only `ANTHROPIC_API_KEY, GEMINI_API_KEY, OPENAI_API_KEY`
    missing — confirms MOONSHOT/MINIMAX/STEPFUN got loaded from
    `~/.agentflow/.env`.
  - Skill validate still works clean; no regressions.

- DONE P9.4 SKILL.md `model:` frontmatter handling:
  - F-AF-2 closed via option (a) — frontmatter `model:` field now
    parses into `SkillMd.model: Option<String>` and survives into
    `SkillManifest.model.name`. Whitespace-only values get
    normalised to `None` so callers can't accidentally configure
    the empty string. Implementation in
    `agentflow-skills/src/skill_md.rs` (frontmatter field at line
    66, manifest conversion at lines 198-201).
  - Tests (3 new in `skill_md::tests`):
    - `parses_model_field_into_manifest` — `model: kimi-k2.6` in
      frontmatter ends up as `manifest.model.resolved_model() ==
      "kimi-k2.6"` (not the `gpt-4o` default).
    - `parses_no_model_field_keeps_default` — absent field keeps
      the historic `resolved_model() == "gpt-4o"` fallback.
    - `parses_empty_model_field_as_none` — `model: "   "` ⇒
      `SkillMd.model == None`, preserving the default-fallback
      behaviour rather than passing an empty string to the LLM
      provider lookup.
  - Status discovered to be already shipped via an earlier commit;
    only this TODOs.md status update was outstanding.

- DONE P9.5 `FlowValue` field reference in `docs/AGENT_SDK.md`:
  - `docs/AGENT_SDK.md` "FlowValue field reference (F-DOC-2)"
    section (line 325) enumerates the three variants with exact
    field names: `Json(Value)` (tuple), `File { path: PathBuf,
    mime_type: Option<String> }`, `Url { url: String, mime_type:
    Option<String> }`. Includes worked examples and an explicit
    callout that the field is `mime_type` not `media_type`. Notes
    the `Serialize` impl's type tag (`"json"` / `"file"` / `"url"`)
    so trace JSON is self-describing.
  - Status discovered to be already shipped via the prior
    `docs/AGENT_SDK.md` writing pass; only this TODOs.md status
    update was outstanding.

- DONE (cross-project) P9.6 phonon-side action items:
  - F-PH-1 (`#[instrument(fields(...))]` truncation), F-PH-2
    (`PodcastPipeline::generate` per-segment durations), and
    F-PH-3 (`phonon-mcp audio_info` surfaces `resampled_from`)
    are entirely in `/Users/hal/rustspace/phonon/Todos.md`. This
    line stays as the agentflow-side cross-reference but the
    agentflow checkbox is closed — none of the three block any
    agentflow work, and their implementation is outside this
    repo's scope.

- DONE P9.7 A1.5 persona: add "re-measure LUFS before save" step:
  - `examples/applications/podcast-mastering/skill.toml` persona
    Step 6 "写盘前再测一次 LUFS": explicit
    `audio_loudness(handle=fade 后的 handle)` call after the fade
    step. Step 8 reporting now says "**实测**最终 LUFS (step 6
    那个，不是 target 参数)" so the agent reports the measured
    value, not the target. F-EX-1 integrity closed.
  - Status discovered to be already shipped via the podcast-
    mastering skill writing pass; only this TODOs.md status
    update was outstanding.

- DONE P9.8 Tighten `target_segments` docstring + A1 README:
  - In-repo half (A1 README): `examples/applications/blog-to-podcast/
    README.md` line 129 reads `--segments <N> | 10 | Approximate
    dialogue segment count` — explicitly conveys the "approximate,
    not strict" semantic. Status discovered to be already shipped.
  - Cross-project half (`phonon-podcast::ScriptRequest::
    target_segments` docstring) lives in
    `/Users/hal/rustspace/phonon/Todos.md` and is outside this repo;
    closed on the agentflow side.

---

## P-LLM — Modality Provider Traits And Model Schema Cleanup (NEW)

Goal: collapse the chat-shaped `text` / `multimodal` / `imageunderstand`
labels into a single `chat` `ModelType`, and add per-modality provider
traits so the 5 multimodal nodes
(`asr` / `tts` / `text_to_image` / `image_to_image` / `image_edit`)
stop hardcoding StepFun.

Context: today
`agentflow-nodes/src/nodes/{asr,tts,text_to_image,image_to_image,image_edit}.rs`
directly import `agentflow_llm::providers::stepfun::*` — they bypass
the registry, so swapping vendors requires rewriting the node. The
chat path (`agentflow-nodes/src/nodes/llm.rs`) already routes through
the 6-provider `LLMProvider` trait; this segment brings the non-chat
modalities up to the same abstraction level.

Naming decision (recorded for posterity): `ModelType::Chat` is the
canonical label for all chat-shaped text-reasoning models, regardless
of input modality. What a chat model can accept (text only, +image,
+audio, +video) is carried in a separate `accepts: Vec<InputType>`
field per model entry. This collapses 180 of 196 current registry
entries onto a single `chat` type and drops the misleading
`multimodal` / `imageunderstand` labels (both were mostly applied to
general chat models that happened to accept image input).

- DONE P-LLM.0 ModelType collapse to `Chat` + `accepts:` field +
  YAML migration:
  - Slice 1 (commit `35278ef`): additive — `accepts: Option<Vec
    <InputType>>` field on `ModelConfig`; `granular_type()` recognises
    canonical `chat` / `text_to_image` / `image_to_image` /
    `image_edit` / `text_to_video` and every legacy alias
    (`text`, `multimodal`, `imageunderstand`, `videounderstand`,
    `docunderstand`, `codegen`, `functioncalling`, `generateimage`,
    `editimage`, `image`); `ModelConfig::accepts()` accessor
    (explicit > inferred).
  - Slice 2 (commit `35278ef`): YAML migration done. 180 / 196
    entries collapse onto `type: chat` (was 161 text + 14
    multimodal + 5 imageunderstand). 65 of those gained explicit
    `accepts: [text, image]`. 5 → `text_to_image` (was 2
    generateimage + 3 image / Imagen). 1 → `image_edit`. The
    remaining 10 (tts / asr / embedding) unchanged. Snapshot test
    `bundled_default_models_yaml_uses_post_pllm0_schema` locks the
    post-migration shape down. Also repaired a pre-existing single-
    line corruption at `default_models.yml:3035` left by commit
    `42c3225` (F-A7-8).
  - Slice 3 (this slice): `ModelType` collapsed from 13 variants to
    8 — `Chat` / `Embedding` / `Text2Image` / `Image2Image` /
    `ImageEdit` / `Tts` / `Asr` / `Text2Video`. `Text`,
    `ImageUnderstand`, `VideoUnderstand`, `DocUnderstand`,
    `CodeGen`, `FunctionCalling` all collapse onto `Chat`.
    `ModelCapabilities` gains an `accepts: HashSet<InputType>`
    field — authoritative per-model input modality answer. The
    `From<&str>` parser is now centralised in `model_types.rs`;
    `granular_type()` is a thin delegate. `is_multimodal()` and
    `validate_request()` consult `accepts` (not the variant).
    `ModelConfig::get_capabilities()` injects the explicit
    `accepts` from YAML so downstream code sees the right set.
    Internal `agentflow-llm/src/providers/stepfun.rs:22` file-local
    `ModelType` enum stays untouched (StepFun's own endpoint-
    selection heuristics; not part of the public surface).
  - Tests: 110 lib tests pass; 9 new tests in `model_types::tests`
    cover Chat default behaviour, ModelCapabilities accepts
    override, validate_request paths, canonical-name parsing,
    legacy-alias collapse, image/audio alias mapping, and
    round-trip stability. Workspace `cargo clippy --workspace
    --all-targets -- -D warnings` clean.

- DONE P-LLM.1 Per-modality Provider trait surface:
  - New `agentflow-llm/src/providers/modality/` module with 5 trait
    files (`asr.rs` / `tts.rs` / `text_to_image.rs` /
    `image_to_image.rs` / `image_edit.rs`) + shared types in
    `mod.rs` (`ImageGenerationResponse` + `GeneratedImage`, used by
    the three image-generation traits since the response shape is
    identical). Each trait keeps its own narrow request type — no
    generic parent.
  - `StepFunSpecializedClient` implements all 5 traits via thin
    shape adapters appended to `providers/stepfun.rs`. Each impl
    translates the modality-level request → StepFun-internal request
    → calls the existing method → translates response back. No new
    wire behaviour; identical surface to today.
  - `lib.rs` re-exports the modality traits (`AsrProvider`,
    `TtsProvider`, `Text2ImageProvider`, `Image2ImageProvider`,
    `ImageEditProvider`) + request/response types
    (`AsrRequest` / `AsrResponse` / `TtsRequest` / `TtsResponse` /
    `GeneratedImage` / `ModalityImageGenerationResponse`). Legacy
    `providers::stepfun::*` exports stay — P-LLM.4 removes them
    once P-LLM.3 has migrated the node call sites.
  - Tests (3 new in `providers::stepfun::tests`):
    - `stepfun_specialized_client_implements_all_modality_traits`:
      compile-time `Box<dyn TraitT>` materialisation for all 5
      traits; assert name() == "stepfun".
    - `modality_to_stepfun_image_response_translates_url_and_b64`:
      shape adapter correctness — url field, legacy `image` field,
      modern `b64_json` field all surface through.
    - `tts_mime_type_falls_back_to_wav_for_unknown_or_missing`:
      MIME mapping table.
  - agentflow-llm lib: 113 / 113 passing (was 110). Workspace
    clippy `--all-targets -- -D warnings` clean.
  - No video this slice. `Text2VideoProvider` /
    `VideoUnderstandProvider` deferred to P-LLM.6.

- DONE P-LLM.2 Registry dispatcher for modality providers:
  - New `agentflow-llm/src/modality_dispatch.rs` module exposes 5
    free functions (`asr_provider` / `tts_provider` /
    `text2image_provider` / `image2image_provider` /
    `image_edit_provider`), each returning a boxed trait object
    from `providers::modality`. Plus thin `AgentFlow::asr(...)` /
    `::tts(...)` / `::text2image_for(...)` / `::image2image(...)` /
    `::image_edit(...)` method aliases on the main entry point.
  - Each function resolves the model from `ModelRegistry::global()`,
    asserts the registered `type:` matches the requested modality
    (mismatch ⇒ `InvalidModelConfig` with a message naming both
    actual and expected types — operator-actionable), resolves the
    vendor's API key via existing `LLMConfig::get_api_key` precedence,
    and routes to the per-vendor factory.
  - Today only StepFun routing is implemented for all 5 modalities;
    other vendors return `UnsupportedProvider` with the modality name
    embedded ("openai (no ASR implementation yet)"). P-LLM.5 adds
    Whisper as the second ASR vendor.
  - Chat path (`AgentFlow::model(...)`) untouched.
  - 2 new unit tests cover the error-message shapes (type mismatch
    + unsupported vendor) — both checking the messages name the
    actual/expected/modality so callers get an actionable signal.
  - agentflow-llm lib: 115 / 115 passing (was 113). Workspace
    clippy clean.

- DONE P-LLM.3 Refactor 5 multimodal nodes to dispatcher:
  - `agentflow-nodes/src/nodes/{asr,tts,text_to_image,
    image_to_image,image_edit}.rs` all dropped their direct
    StepFun coupling. Each now resolves the provider through
    `AgentFlow::<modality>(&self.model).await?` (registry-driven
    vendor selection) and submits a modality-level request.
  - Node YAML surface unchanged — `model:` / input keys / output
    keys all preserve identical shape. User-visible behavior
    identical for StepFun models.
  - Modality request types (`Text2ImageRequest`,
    `Image2ImageRequest`, `ImageEditRequest`) name-collide with
    the StepFun-internal types at the crate root, so node code
    imports them via the full module path
    (`agentflow_llm::providers::modality::Text2ImageRequest as
    ModalityText2ImageRequest`). P-LLM.4 will demote the StepFun
    types and let the modality ones win at the crate root.
  - Known limitation surfaced: `TextToImageNode::style_reference`
    was StepFun-specific; the cross-vendor `Text2ImageRequest`
    trait doesn't carry it today. The field stays on
    `TextToImageNode` for API compat but is dropped when
    constructing the trait request — a future trait extension
    (vendor extras map, P-LLM.5 follow-up if needed) can route
    it back through.
  - Removed `STEPFUN_API_KEY` direct env lookups from all 5 node
    files — the dispatcher resolves API keys via
    `LLMConfig::get_api_key(vendor)` with the same precedence the
    chat path uses, so YAML configs and `~/.agentflow/.env`
    handling stays consistent.
  - agentflow-nodes lib: 25 / 25 passing (4 ignored are pre-existing
    STEPFUN_API_KEY-gated integration tests). Workspace clippy
    clean.

- DONE P-LLM.4 Clean up `lib.rs` re-exports:
  - Removed the crate-root `pub use providers::stepfun::*` block
    (ASRRequest, TTSBuilder, TTSRequest, Text2ImageBuilder,
    Text2ImageRequest, Image2ImageRequest, ImageEditRequest,
    ImageGenerationResponse, VoiceCloningRequest /Response,
    VoiceListResponse, StepFunSpecializedClient).
  - Removed `AgentFlow::stepfun_client` / `stepfun_client_with_base_url`
    / `text2image(...)` / `text_to_speech(...)` — these handed
    callers a StepFun-internal builder, bypassing the dispatcher.
  - Modality types now win at the crate root: `agentflow_llm::
    Text2ImageRequest` / `Image2ImageRequest` / `ImageEditRequest`
    / `ImageGenerationResponse` all resolve to the
    `providers::modality::*` variants. P-LLM.3 node imports that
    used the full path can be shortened in a follow-up, but the
    short path now points at the right type.
  - StepFun specialized types stay reachable via the long path
    `agentflow_llm::providers::stepfun::*` for the live integration
    tests that intentionally exercise StepFun-specific wire shapes
    (`tests/provider_consistency_live.rs` — gated on
    `AGENTFLOW_LIVE_LLM_TESTS=1`). Not promoted at the crate root,
    so new external code that wants to bypass the modality surface
    has to make the long-path access explicit and discoverable in
    review.
  - Pure cleanup of the CLI's remaining dead StepFun direct calls:
    `agentflow-cli/src/commands/audio/{asr,clone,tts}.rs` and
    `commands/image/generate.rs` were the last consumers — all now
    go through the dispatcher. The `clone.rs` voice-cloning CLI
    stays a documented stub (no `VoiceCloningProvider` trait yet;
    flagged for a future P-LLM follow-up).
  - agentflow-llm lib: 115 / 115 passing. agentflow-nodes lib:
    25 / 25 passing. Workspace `cargo clippy --workspace
    --all-targets -- -D warnings` clean. Live integration tests
    (`provider_consistency_live`) still compile.

- DONE P-LLM.5 Second vendor for trait shape validation:
  - New `agentflow-llm/src/providers/openai_asr.rs` ships
    `OpenAIAsrProvider`. Hits `POST {base_url}/audio/transcriptions`
    with `multipart/form-data` per the OpenAI API spec. Supports
    `whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`.
  - Trait-shape calibration: `AsrRequest` gained a `prompt:
    Option<String>` field (Whisper's bias-prompt for domain
    vocabulary — capped at 224 tokens by Whisper, silently ignored
    by StepFun). Updated the 2 call sites (`agentflow-nodes/src/
    nodes/asr.rs` + `agentflow-cli/src/commands/audio/asr.rs`).
    `language` and `temperature` already on the trait now actually
    flow through to Whisper as documented.
  - `default_models.yml` gained three OpenAI ASR entries:
    `whisper-1`, `gpt-4o-transcribe`, `gpt-4o-mini-transcribe`
    (all `vendor: openai, type: asr, accepts: [audio]`).
  - `modality_dispatch::asr_provider` routes `vendor == "openai"`
    to `OpenAIAsrProvider`; StepFun still wins for `"stepfun" |
    "step"`. Anyone else still gets `UnsupportedProvider`.
  - MIME mapping helper covers OpenAI's 7 documented formats
    (mp3 / mp4 / mpeg / mpga / m4a / wav / webm) plus flac / ogg
    / opus that reqwest will happily send (server reads codec
    from bytes). Unknown extensions fall back to
    `application/octet-stream`.
  - Response parsing dispatches by `response_format`: `json` /
    `verbose_json` parse JSON and pull the `text` field, with the
    full payload preserved in `AsrResponse::metadata` for
    timestamps / segments / language detection. `text` / `srt` /
    `vtt` use the response body verbatim, no metadata.
  - Tests (8 new in `openai_asr::tests`):
    - `mime_for_filename_covers_documented_formats` — every OpenAI-
      documented audio extension plus the unknown-extension fallback,
      case-insensitive.
    - `parse_json_response_pulls_text_and_preserves_metadata`,
      `parse_verbose_json_response_pulls_text_and_preserves_segments`
      — JSON paths return text + carry metadata (language /
      duration / segments).
    - `parse_text_response_uses_body_verbatim_without_metadata`,
      `parse_srt_response_uses_body_verbatim` — non-JSON paths.
    - `parse_json_with_missing_text_field_returns_typed_error`,
      `parse_json_with_invalid_payload_returns_typed_error` —
      typed-error contracts so callers can handle response-shape
      drift.
    - `empty_api_key_is_rejected_at_construction`,
      `build_form_smoke_test` — construction + form-building
      smoke without HTTP.
  - Live end-to-end test:
    `provider_consistency_live::whisper_via_modality_dispatcher_transcribes_audio`
    gated on `AGENTFLOW_LIVE_AUDIO_TESTS=1` + `OPENAI_API_KEY`.
    Walks the full dispatcher → registry → OpenAIAsrProvider →
    HTTP → response-parsing path. Uses StepFun TTS to produce
    the audio fixture when StepFun is configured; otherwise
    falls back to a 1-second silent WAV so the multipart path
    still exercises against Whisper.
  - agentflow-llm lib: 124 / 124 passing (was 115; +8 from
    openai_asr + 1 live test slot). Workspace clippy clean.

- DEFERRED P-LLM.6 Video modality:
  - Trigger: Veo / Sora / Runway becomes Rust-callable + stable
    AND a concrete agentflow workflow needs video.
  - Add `Text2VideoProvider` / `VideoUnderstandProvider` traits +
    `text_to_video` / `video_understand` nodes at that time.

---

## M — Maintenance Tasks (NEW)

Ongoing housekeeping that should ride along with feature work but doesn't
fit a P-segment.

- DONE M.1 `CLAUDE.md` sync after worker/ui.

- DONE M.2 `docs/AGENT_SDK.md` trait-change sync:
  - New `cargo xtask check-agent-sdk-doc` subcommand walks every
    backtick-quoted CamelCase identifier in `docs/AGENT_SDK.md` and
    asserts a matching `pub (trait|struct|enum|type|fn) Ident`
    declaration exists under any `agentflow-*/src/**/*.rs`. Catches
    doc rot when a trait or type referenced in the SDK guide is
    renamed or removed without updating the doc. An explicit
    allowlist (`Err`, `None`, enum variants `Step` / `Plan` /
    `Reflect` / `Failure` / `Critique` / `Final` / `FailureReason`,
    and the doc's inline example type `EchoTool`) covers known
    non-type mentions.
  - Heuristics: CamelCase = leading uppercase + alphanumerics + at
    least one lowercase (skips acronyms like `JSON` / `URL`).
    Pub-decl scanner handles `pub`, `pub(crate)`, `pub(super)`,
    and `pub(in path)`.
  - CI: new `check-agent-sdk-doc` Quality job listed in
    `release-gate.needs`. Today's clean state: 35 mentions
    cross-referenced, 9 ignored via allowlist, all pass.
  - Tests: 5 unit (happy path, missing-type failure, allowlist
    honored, extractor edge cases, visibility-restricted decls) +
    1 integration (real workspace exits 0).
  - Doc maintenance checklist itself is enforced by the xtask, not
    a separate written checklist — the CI gate fails any PR that
    introduces drift, which is the same outcome as a checklist
    review step but is mechanically enforced.

- DONE M.3 Test coverage gaps (db + memory parts; worker part deferred
  to P5.5–P5.7):
  - `agentflow-db/tests/repositories.rs` grew from 2 to 12 tests.
    New coverage:
    - `run_repo_list_isolates_tenants` — tenant-scoped reads don't
      bleed across tenants.
    - `run_repo_update_status_errors_when_missing` — missing-id
      update errors with the id in the message.
    - `step_repo_list_for_run_returns_in_seq_order` — out-of-order
      inserts still surface seq-ascending.
    - `artifact_repo_create_and_list_round_trip` — full CRUD +
      cross-run isolation.
    - `skill_install_repo_upsert_replaces_on_conflict` — UPSERT
      semantics + multi-version coexistence.
    - `mcp_session_repo_open_and_close_lifecycle` — open + close
      lifecycle + missing-id error path.
    - `harness_session_repo_create_get_list_update` — full CRUD on
      the harness session table.
    - `harness_event_repo_append_list_max_seq` — append, list_after,
      and `max_seq` (used by `:resume` mode=append).
    - `harness_session_reset_for_resume_wipes_events` — rerun
      resume wipes prior events as documented.
    - `harness_session_reset_for_append_resume_keeps_events` —
      append-mode resume keeps prior events.
  - Test-infrastructure fix: removed the per-test `TRUNCATE` from
    `fresh_db()` (it was racing parallel tests that share the same
    DB and wiping each other's seeded rows mid-test). Every test
    now uses a per-invocation UUID-suffixed tenant / skill name so
    re-runs against the same DB don't accumulate noise into the
    `assert_eq!(len, 1)` invariants. Migration roundtrip stays in
    `tests/migrations.rs`.
  - `agentflow-memory` part already closed by P4.7 (37 hermetic
    tests covering Session / Semantic / Preference / Entity facts
    backends + cross-layer integration test).
  - Worker (P5) coverage tracked under P5.5–P5.7.

- DONE M.4 Historical eval doc cleanup.

- DONE M.5 CI workflow audit (see `docs/CI_WORKFLOWS.md`).

- DONE M.7 Fix broken minimal feature combinations:
  - DONE `agentflow-llm --no-default-features --features openai`:
    `tracing` was declared optional under `logging`/`observability`
    but source used `tracing::*` unconditionally. Aligned with the
    rest of the workspace by making `tracing` a hard dep;
    `tracing-subscriber` stays optional under `logging` (it's the
    heavy part); `observability` is kept as an empty alias for
    backwards compat.
  - DONE `agentflow-nodes` `factories` feature: the gated module
    referenced constructors (`LlmNode::new(&str, &str)`, etc.) that
    haven't existed since the unit-struct rewrite. The module was
    unused anywhere else in the workspace. Deleted the module and
    the feature flag; `NodeRegistry::default()` now just returns
    `Self::new()`.
  - DONE `agentflow-nodes` `conditional`: stale `FlowValue::String`
    pattern + `value.as_f64()` call. Dropped the dead arm
    (`FlowValue::Json(Value::String)` covers it) and added a small
    `flow_value_as_f64` helper.
  - DONE `agentflow-nodes` `batch`: `#[derive(Debug)]` on `BatchNode`
    pulled in `dyn AsyncNode: Debug` which isn't on the trait.
    Hand-rolled `Debug` impl that prints `<AsyncNode>` for the
    `child_node` field. Also fixed the batch result serialization
    so downstream consumers get a plain JSON array instead of a
    `{type, value}`-wrapped envelope. Clippy let-chain cleanup.
  - DONE CI matrix: dropped the broken-combo comment block from
    Quality `features` job and added `llm-openai-only` +
    `nodes-batch-conditional` rows. Both now run on every PR.
  - Tests: `cargo test -p agentflow-nodes --features
    batch,conditional` → 30 / 0 pass; `cargo test -p agentflow-llm
    --no-default-features --features openai --lib` → 98 / 0 pass.

- DONE M.6 Workspace edition pin:
  - New `xtask/` workspace member with a single `verify-edition`
    subcommand walks every member's `Cargo.toml` and asserts
    `edition = "2024"`. Stable iteration order, structured failure
    output, and a synthetic-workspace test matrix (pass / wrong-
    edition fail / missing-edition error) live alongside the
    implementation.
  - `.cargo/config.toml` ships the canonical `xtask = "run --package
    xtask --quiet --"` alias so `cargo xtask verify-edition` Just
    Works from any subdirectory of the workspace.
  - Quality CI workflow gains a `verify-edition` job listed under
    both `release-gate.needs` and its `test` summary call, so a
    drift fails the release gate. Today's clean state:
    `cargo xtask verify-edition` reports `checked 17 workspace
    member(s) against edition "2024" — OK`.
  - Tests: 3 unit (synthetic workspace) + 3 integration (real
    workspace, unknown subcommand, missing subcommand) — all
    hermetic.

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
